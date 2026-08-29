# Terrain Shader Optimization and Simplification Plan

## Objective

Replace the current accumulated terrain-shader blend chain with one explicit,
measurable material pipeline. The terrain has eight visible material classes:

1. dirt;
2. grass;
3. rock;
4. river bed;
5. beach;
6. fallen stones;
7. forest floor; and
8. snow.

Grass keeps its close-range fur passes. Dirt, rock, river bed, beach, fallen
stones, and forest floor use height-aware blending. Grass and snow use
coherent-noise boundaries without height blending. Wetness is calculated once
and applied after the final surface has been selected.

The refactor must preserve the Rust-to-Unity terrain-data contract deliberately,
not accidentally. It must also reduce texture slots, texture samples, duplicated
coverage code, shader variants, and serialized material properties.

## Canonical material model

### Base height-blended materials

Use fixed layer indices shared by Rust, Unity C#, and HLSL:

| Index | Layer | Ownership source | Height map |
| --- | --- | --- | --- |
| 0 | Dirt | fallback plus loose-cover soil | required |
| 1 | Forest floor | exported tree-support switch | required |
| 2 | Rock | geology, slope, cliff, and forced-rock data | required |
| 3 | River bed | exported river-bed channel | required |
| 4 | Beach | connected-sea proximity, height, and loose cover | required |
| 5 | Fallen stones | exported settled-stones switch | required |

Dirt is the fallback layer, so the six base weights always normalize to a
valid result. Underwater ground is wet beach/rock/river bed rather than a hidden
ninth `deep` material.

### Non-height overlays

- Grass is a coherent-noise coverage overlay on eligible dirt. Its visible
  ground colour and its fur clip mask must come from exactly the same function.
- Snow is a coherent-noise elevation overlay. It may cover grass and ordinary
  base materials but retains the existing policy of excluding true vertical
  cliffs.
- Wetness is a lighting/surface treatment, not a material layer.

### Forest floor contract

Forest floor remains a distinct height-blended material with its own recipe,
albedo, normal, height, occlusion, parallax, and coherent boundary. Retain the
exported tree-support switch through Rust, Bevy, Unity mesh data, validation, and
documentation.

Simplification means feeding forest floor through the same candidate,
height-blend, texture-array, normal, and occlusion pipeline as every other base
material. It must not retain a parallel blend implementation or a duplicated
grass-only approximation.

## Target texture contract

Replace individual material samplers and runtime RGBA repacking with three
`Texture2DArray` resources whose slices share resolution and mip count:

- `_TerrainAlbedoArray`: sRGB RGB/RGBA slices for dirt, forest floor, rock,
  river bed, beach, and stones;
- `_TerrainNormalArray`: linear normal slices in the engine-requested Unity
  convention; these supplement rather than replace the preserved procedural
  normal fields described below; and
- `_TerrainMaskArray`: linear RG slices, `R = height` and `G = occlusion`.

This changes six height-blended materials from up to eighteen separately named
textures to three shader resources. It also removes the two packed-mask render
textures, their startup blits, `PackDualMaterialMasks.shader`, its always-included
shader entry, and all legacy per-material mask properties.

All array slices must be baked at the requested runtime resolution. The Rust
library continues to own recipe evaluation and returns texture bytes; Unity owns
palette selection and array construction. Add dirt and beach runtime recipes so
height blending is real for those materials rather than synthesized from a flat
constant.

Store per-layer scalar settings in compact vectors or constant arrays rather
than repeated properties:

- world size;
- height-blend influence;
- normal strength;
- parallax depth; and
- occlusion strength.

Retain individual names only for values that artists genuinely tune
independently and that remain part of the intended public material contract.

## Shared coverage pipeline

Create `TerrainCoverageCommon.cginc`, used by both `TerrainDetail.shader` and
`TerrainGrassCommon.cginc`. It must contain the canonical noise coordinates,
noise channel offsets, signed-distance construction, and material ownership
rules. Neither shader may carry a copied version of those formulas.

The pipeline is:

1. Decode geometry and exported fields once: elevation, slope, hardness, loose
   cover, river bed, connected-sea proximity, forest floor, and stones.
2. Sample a small shared coherent-noise basis once at broad, medium, and fine
   scales.
3. Build one signed candidate distance for each material. Noise offsets the
   distance before antialiasing; it does not tint the result or get reapplied in
   later blend stages.
4. Convert signed distances into candidate coverages using one derivative-aware
   helper.
5. Apply physical eligibility rules once. Examples: beach requires connected
   coast and low elevation; grass requires eligible dirt; snow uses elevation;
   river bed comes from the exported channel.
6. Height-blend and normalize the six base candidates.
7. Apply grass and snow overlays using their shared coherent coverage.
8. Blend albedo, normal, occlusion, and other surface values from the same final
   weights.
9. Apply wetness once.
10. Light and fog the completed surface.

The coherent noise should use stable, named channels per boundary so different
materials do not reveal the same stencil. Its scale and amplitude are world-space
values; they must not change with terrain tessellation or LOD.

### Coherent boundary invariant

Every visible ground-material boundary must include a non-zero coherent-noise
component before antialiasing and, where applicable, before height blending.
There are no straight interpolation-only boundaries.

| Material | Boundary signal | Coherent component |
| --- | --- | --- |
| Dirt | fallback against every competing layer | inherits the unique noise of the competing layer; dirt/grass also uses grass patch noise |
| Forest floor | exported tree-support field | dedicated fine forest-edge channel |
| Rock | geology, slope, cliff, and forced-rock fields | dedicated broad rock and finer cliff/forced-rock channels |
| River bed | exported river-bed field | dedicated fine bank channel with a broader secondary octave |
| Beach | connected-coast proximity, elevation, and loose cover | dedicated broad sand-patch channel plus finer shore-edge variation |
| Fallen stones | exported stones field | dedicated clustered-stones edge channel |
| Grass | eligible loose-cover field | dedicated coherent grass-patch channel shared by ground and fur |
| Snow | elevation relative to snow line | dedicated macro snowline channel plus finer edge variation |

Each channel must be deterministic from island-local world position and the
island seed, zero-centred, and continuous across tiles. Give each material a
different seed offset/channel combination while sharing the same small set of
noise texture samples. Avoid white-noise dithering, screen-space noise, UV-space
noise tied to a material tile, or noise derived from terrain triangle indices.

Noise perturbs only a finite transition band. It must not punch isolated holes
through a material interior, defeat connected-sea eligibility, move the exact
sea-level wetness rule, or change hard geometry/safety classifications. Height
maps then decide which eligible base surface wins locally inside the noisy
overlap band.

## Height-aware compositor

Replace the ordered series of `lerp` calls and special-case restoration passes
with a symmetric multi-layer height blend. For each eligible base material:

```text
height_score[i] = candidate[i]
                + (height[i] - 0.5) * height_influence[i]
                  * transition_activity[i]

peak = max(height_score)
weight[i] = candidate[i]
          * saturate((height_score[i] - peak + blend_depth) / blend_depth)
weight = weight / max(sum(weight), epsilon)
```

`transition_activity` prevents a height map from weakening an uncontested
material interior. `blend_depth` is the one common softness control, with an
optional compact per-layer multiplier only if visual tests prove it necessary.

This gives one answer at three-way junctions and avoids order-dependent cases
such as rock being applied, covered by beach, restored over river, then restored
again for cliffs. Cliff and forced-rock inputs strengthen the rock candidate;
they do not trigger later colour overrides.

Use the final normalized weights for albedo, normal, and occlusion. Do not derive
separate coverage approximations for each output.

## Preserved procedural normal contract

The refactor must preserve the current noise-perturbed normal response for dirt,
rock, beach, and grass. Height blending and texture arrays change material
composition; they do not replace these established normal fields.

### Dirt

- Preserve the current coherent 3D soil-detail perturbation, scale, coordinate
  system, and `_DirtNormalStrength` response.
- Apply it to the final dirt weight, including visible bare dirt beneath the
  close grass radius.
- Begin the array migration with a flat dirt normal slice. A recipe normal may
  only be added later as a measured, explicitly approved layer on top of the
  preserved procedural result.

### Rock

- Preserve the current broad/detail coherent rock perturbation controlled by
  `_CliffNormalStrength` and `_CliffNoiseDetailScale`.
- Preserve the existing authored rock normal-map contribution on top of that
  procedural result.
- Forced rock, geology rock, and cliff rock must feed the same rock-normal
  function instead of acquiring different normal implementations during the
  compositor refactor.

### Beach

- Preserve the current coherent sand-detail perturbation, scale, coordinate
  system, `_SandNormalDetailScale`, and `_SandNormalStrength` response.
- Begin the array migration with a flat beach normal slice so the new beach
  height/albedo recipe does not silently change its lighting. Any later recipe
  normal is an additive visual change requiring comparison approval.

### Grass

- Preserve the current coherent grass-detail perturbation controlled by
  `_GrassNormalDetailScale` and `_GrassNormalStrength` on the distant terrain
  surface.
- Preserve `MotuGrassWindSample` and the current tangent-wind normal bend,
  `_GrassWindNormalStrength`, world-space sampling, and coverage weighting.
- Preserve the fur shader's current wind-lighting normal behavior, including its
  shell-layer weighting and root-to-tip response.
- Apply wind only to the final grass coverage. Dirt, forest floor, rock, river
  bed, beach, stones, and snow must remain stationary.

For the base compositor, evaluate each material's complete normal function first,
then blend those world-space normals with the same normalized weights used for
albedo and occlusion, and normalize once. Apply the grass wind bend after the
static grass normal has been composed, matching the current lighting order.

Add a normal-only debug view with wind frozen and animated modes. Fixed-camera
normal captures must prove that the refactor has not flattened, rescaled, moved,
or phase-shifted the existing dirt, rock, beach, or grass detail.

## Sampling and parallax strategy

The present shader can execute several independent eight-step parallax searches
before it knows which material is visible. Replace this with:

1. cheap candidate classification;
2. mask-height samples only for candidates active in a transition;
3. selection of the dominant and runner-up base layers;
4. one parallax search for the dominant layer; and
5. a cheaper single offset, or no offset, for the runner-up after visual and GPU
   timing comparison.

Dynamic `Texture2DArray` slice indexing should allow the dominant layer to be
sampled without declaring every material texture separately. Confirm generated
Metal code and timing rather than assuming a branch saves work.

Parallax UVs must remain periodic: all ray steps sample repeating array slices,
and every layer uses the same wrapped local-space convention. Parallax affects
surface detail only; it must not change the coarse material ownership field.

Do not add parallax to grass or snow. Preserve parallax for rock, river bed,
forest floor, and stones. Enable shallow dirt or beach parallax only if the
measured visual gain justifies the samples.

## Grass ground and fur contract

Grass has two representations but one coverage:

- `TerrainDetail.shader` uses `GrassCoverage(...)` for the distant green ground;
- every fur pass uses the same `GrassCoverage(...)` and clips at the same
  threshold; and
- radial fur fading changes only the presence of shells, never the ground
  material boundary.

The shared function includes loose cover, coherent patch noise, base-material
eligibility, elevation, and snow exclusion. Forest floor and stones must not have
parallel grass-only formulas. Grass is excluded using the same final
forest-floor and stones ownership used by the ground shader.

To keep that exclusion exact without evaluating every base material in every fur
fragment, provide a lightweight shared exclusion function that samples only the
dirt/forest-floor and dirt/stones height pairs when those exported switches are
active. Measure this cost across all shell passes; if it is material, compare a
generated grass-eligibility mask, but do not accept a visibly mismatched boundary.

Keep wind and individual-blade noise in the fur shader. Move only terrain
classification into the shared include. The fur passes must not repeat albedo,
normal, parallax, or occlusion sampling from the terrain shader.

## Snow contract

Snow is a final colour/normal overlay driven by elevation plus shared broad and
edge coherent noise. It does not sample a height map and does not participate in
the base height normalization.

Keep one explicit cliff exclusion rule. Remove any other hidden precedence
rules. Snow coverage must be calculated once and reused for colour, normal,
wetness exclusion, and grass clipping.

## Wetness contract

Calculate wetness once from physical sources:

- river-bed proximity from the existing exported river field; and
- a coastal wetness varying produced by the terrain vertex shader.

Use the broader river interpolation for immediate banks, rather than introducing
a dedicated Rust/mesh wetness channel. Multiply wetness by the final wettable
coverage so snow and visible grass are not accidentally varnished.

In the vertex shader, derive coastal wetness from island-local elevation:

```text
coastal_wetness_vertex = vertex_elevation <= 0.05 ? 1.0 : 0.0
```

This deliberately marks every vertex below the sea plane, on the plane, or up to
five centimetres above it with wetness `1`. Higher vertices receive `0`. Pass the
value to the fragment shader as a normal interpolated varying; do not use
`nointerpolation`, and do not bake it into exported vertex data.

In the fragment shader, add a dedicated coherent coastal-noise channel to the
interpolated boundary before antialiasing:

```text
coastal_distance = interpolated_coastal_wetness
                   - (0.5 + coastal_noise * coastal_noise_strength)
coastal_transition = max(coastal_blend_width, fwidth(coastal_distance))
coastal_wetness = smoothstep(
    -coastal_transition,
    coastal_transition,
    coastal_distance)
```

The noise must be deterministic island-local world-space coherent noise. It
perturbs the interpolated wet/dry edge without changing vertices whose
interpolated value is solidly zero or one.

After combining river and coastal sources, apply one hard fragment-space
sea-plane gate:

```text
above_sea = step(0.0, elevation)
wetness = max(river_bank_wetness, coastal_wetness)
         * above_sea
         * wettable_coverage
```

`step(0.0, elevation)` deliberately keeps fragments exactly on the sea plane wet
while making every fragment with negative elevation completely dry. Do not
antialias, noise-perturb, or blend this cutoff across the sea plane. The gate is
applied after the `max`, so submerged river-bed fragments cannot reintroduce
wetness.

Apply wetness after material composition:

- darken final albedo once;
- increase smoothness/specular response once;
- preserve final normals and occlusion; and
- use one Fresnel/highlight implementation.

There must be no material-specific wetness tint or a second coastal code path.
The water shader provides the appearance of water over submerged ground; the
terrain wetness response remains disabled below it.

## Unity runtime contract

Centralize shader property IDs and runtime texture ownership in a small terrain
material binding type instead of spreading `SetTexture`, `SetFloat`, and
`HasProperty` calls through `IslandGenerator`.

That binding owns:

- creation and disposal of the three texture arrays;
- fixed layer-index validation;
- palette and per-layer settings;
- assignment to terrain and grass materials;
- debug-view selection; and
- a strict startup contract check.

Remove the temporary runtime mask packing path after the array implementation is
validated. Do not keep both systems behind a compatibility switch.

Update the Rust runtime-material response from the current rock, river, forest,
and stones set to dirt, forest floor, rock, river, beach, and stones. The engine
still passes the island dirt and stone palette. Recipes must use those colours
directly so a shader boundary cannot expose a different authored base hue.

## Remnant complexity to remove

After the replacement path is visually accepted, delete rather than deprecate:

- `_RockMaskMap`, `_RiverBedMaskMap`, `_ForestFloorMaskMap`, and `_StonesMaskMap`;
- `_RockRiverMaskMap` and `_ForestStonesMaskMap`;
- `PackDualMaterialMasks.shader` and the Graphics Settings inclusion;
- runtime mask `RenderTexture` creation and blitting;
- `_GrassThinDepositColor` as an alias for dirt;
- the fixed `deep` underwater colour;
- repeated `AntialiasedMask`, height-weight, rock, river, beach, forest, stones,
  and snow calculations in the grass shader;
- sequential rock restoration and cliff override `lerp` passes;
- separate colour, normal, and occlusion coverage approximations;
- properties retained only for abandoned texture paths; and
- validation code that checks the removed legacy properties or imported editor
  textures.

Run a property-usage audit after deletion: every property in the material,
shader, validation, and C# binding must have one documented consumer.

## Implementation sequence

### Phase 0: Baseline and instrumentation

- Capture fixed-camera screenshots for dirt/grass, dirt/forest floor, dirt/rock,
  river banks, beach/rock, stones/dirt, snow line, coast wetness, and a three-way
  junction.
- Capture normal-only baselines for dirt, rock, beach, still grass, and grass at
  several points in the current wind cycle.
- Record Unity shader compiler output, sampler count, interpolator count,
  instruction estimates, variant count, and representative Metal frame timing.
- Add temporary debug views for raw exported fields and current final coverages.
- Add a boundary-noise debug view that can isolate the signed coherent
  contribution for every material listed above.
- Record runtime texture allocation size and material-build time.

### Phase 1: Define and test the contract

- Add fixed material-layer indices to Rust, Unity, and HLSL-facing documentation.
- Add dirt and beach recipes with engine palette parameters and meaningful
  height/occlusion output.
- Add Rust tests proving all six runtime layers have identical dimensions and
  the requested normal convention.
- Preserve and document the exact forest-floor vertex-channel semantics across
  Rust, Bevy, Unity, and HLSL.

### Phase 2: Build array resources

- Create albedo, normal, and RG height/occlusion arrays in Unity.
- Validate colour space, mip generation, repeat wrapping, and layer order.
- Bind arrays to an additive experimental shader path while the current path is
  still available for A/B screenshots.
- Measure Metal support and generated shader behavior before proceeding.

### Phase 3: Extract shared classification

- Introduce `TerrainCoverageCommon.cginc`.
- Move noise sampling, signed distances, antialiasing, and eligibility rules into
  it.
- Make terrain and fur consume the same grass/snow/base candidate structure.
- Add debug outputs for each candidate and verify terrain/fur boundary identity.
- Add assertions or image tests proving every material boundary changes when its
  coherent amplitude is switched between zero and its production value.

### Phase 4: Replace the blend chain

- Implement normalized six-way height blending.
- Drive albedo, normal, and occlusion from the same weights.
- Route dirt, rock, beach, and grass through their preserved procedural normal
  functions before blending; use flat dirt/beach array normals initially.
- Replace cliff restoration with rock candidate strength.
- Remove the `deep` branch and express submerged appearance through beach/rock,
  water, and lighting without terrain wetness below the sea plane.
- Confirm all six base-material boundaries react visibly to height maps.

### Phase 5: Optimize parallax and sampling

- Select dominant and runner-up layers after classification.
- Reduce parallax to the measured minimum described above.
- Remove redundant texture and 3D-noise samples.
- Compare full, reduced, and disabled parallax timing and screenshots on Metal.
- Keep the cheapest version that preserves visible depth and seamless wrapping.

### Phase 6: Unify grass and snow

- Switch distant grass and every fur pass to the shared grass coverage.
- Replace the current duplicated forest-floor/stones fur calculations with the
  smallest shared height-aware exclusion path that still matches the ground.
- Preserve the existing distant-ground and fur `MotuGrassWindSample` normal
  deformation, including shell weighting and `_GrassWindNormalStrength`.
- Switch all snow consumers to one snow coverage.
- Verify transitions while moving through the fur fade radius.

### Phase 7: Apply one wetness stage

- Emit the binary five-centimetre coastal wetness value in the terrain vertex
  shader and pass it through an ordinary interpolated varying.
- Apply the dedicated coherent coastal boundary noise to that interpolated value
  in the fragment shader.
- Gate the combined source by final wettable coverage and then by the hard
  fragment-space sea-plane cutoff.
- Remove earlier per-branch wetness logic.
- Validate dry beach above the coastal band, dry submerged surfaces, wet river
  beds/banks above sea level, snow, and grass.
- Add vertex-stage checks below sea level, at sea level, at 2.5 and 5
  centimetres above it, and immediately above 5 centimetres.
- Add fragment checks proving ordinary interpolation and coherent boundary noise
  shape the above-sea fade while the final below-sea result is exactly zero.

### Phase 8: Delete the legacy path

- Remove the standalone forest-floor blend path after forest floor is handled by
  the common base-material compositor. Retain its recipe and vertex data.
- Remove mask packing and old shader properties.
- Collapse C# copying/validation into the terrain material binding.
- Update Unity material assets, README contracts, Rust tests, Bevy conversion,
  and native FFI validation together.
- Run a dead-property and dead-function search before final commit.

## Validation gates

Each phase must keep the following green:

- `cargo fmt --all -- --check` in every changed Rust crate;
- generator/library tests, including runtime palette and terrain-channel tests;
- strict clippy for the changed Rust targets;
- Bevy compilation when the shared runtime-material response changes;
- Unity C# compilation and native interop validation;
- Unity shader compilation for Metal with no unsupported array indexing or
  sampler-limit errors;
- rendered debug views for every raw candidate, final base weight, grass, snow,
  and wetness;
- fixed-camera comparison screenshots at every named boundary; and
- a deployed native-library checksum match whenever the Rust FFI changes.

## Completion criteria

The work is complete when:

- the only visible terrain classes are dirt, grass, rock, river bed, beach,
  stones, forest floor, and snow;
- dirt, rock, river bed, beach, stones, and forest floor all use height maps in
  one normalized blend;
- grass and snow use coherent-noise boundaries without height blending;
- every boundary has coherent world-space breakup and no LOD-dependent shift;
- dirt, forest floor, rock, river bed, beach, stones, grass, and snow each have a
  named, independently inspectable coherent boundary contribution;
- dirt, rock, beach, and grass retain their current coherent procedural normal
  detail at the same world-space scale and strength;
- distant grass and fur retain the current animated wind effect on their
  lighting normals;
- distant grass and fur end at the same boundary;
- wetness is one post-composition effect covering river beds/banks above sea
  level and the coherently perturbed interpolation from coastal vertices marked
  through five centimetres above the coast;
- every fragment below the sea plane has exactly zero terrain wetness, with a
  hard cutoff at the plane and no river-field override;
- forest floor remains a distinct recipe-driven material using the common base
  compositor;
- no runtime mask-packing shader or obsolete texture properties remain;
- dirt and stone palette colours match across recipes and shader-generated
  surfaces;
- shader sampler, instruction, variant, allocation, and frame-time measurements
  are all no worse than the recorded baseline, with the expected large reduction
  in parallax samples; and
- the fixed-camera visual set is accepted before the old path is deleted.
