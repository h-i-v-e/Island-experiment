# Player-Centred Ocean Wave System Plan

## Implementation Status

Phases 1-4 now have an initial implementation behind the geometric-wave toggle:

- a reusable `OceanWaveProfile` supplies bounded geometry, wave, and coastal
  attenuation settings;
- `OceanSurfaceController` builds one crack-free graded clipmap with a dense
  near-player grid and progressively coarser horizon cells;
- `SeaWater.shader` evaluates four stable world-space directional waves,
  derives a per-fragment lighting normal from the same function, and retains
  those normals after geometric displacement fades across the distant ocean;
- `OceanWaveMaskComposer` combines every overlapping active island sea mask
  into one player-centred GPU attenuation texture and derives a second texture
  containing signed X/Z onshore directions from an averaged depth-and-distance
  gradient;
- `IslandRuntime` registers and unregisters coastal masks as it activates,
  becomes dormant, or disposes;
- `OceanWaveSandbox` provides an island-free flight scene for rapid tuning.

Remaining work is visual play-mode tuning, Metal profiling, and any adjustments
identified at real coastlines. Horizontal choppiness defaults to zero until the
vertical-wave and shoreline behavior have been visually approved.

The first hardening pass also exposes bounded mesh and coastal-mask composition
statistics in the ocean sandbox. Automated validation checks the one-metre
default density, exact outer extent, finite attributes, upward winding,
non-degenerate triangles, expanded bounds, and mask-anchor grid alignment.
The shared wave field now uses the existing periodic coherent-noise texture to
warp each wave component differently and give every component an independent
local height envelope. This forms coherent groups of tall and subdued waves,
breaking up the regular interference grid without separating distant normals
from nearby geometric displacement.
Whitecaps are selected from high, breaking crests and fragmented with a moving
broad coherent noise field. A finer noise layer moves against the primary swell
to erode that envelope into smaller counter-moving fragments. Their colour
remains subject to sunlight, cloud cover, and shadows instead of acting as
emissive foam. As coastal attenuation flattens a wave, its foam height threshold
drops and its minimum coverage rises while both noise layers remain visible; a
narrow final allowance band then fades all foam to zero on the completely flat
shoreline surface.
The average of the five-metre depth allowance and sixteen-metre
distance-to-shore allowance now supplies a configurable additional swell
component. Its RG channels encode the normalised direction towards the shore,
its blue channel gates the effect to the reliable sloping coastal band, and
alpha retains the continuous coast-relative phase coordinate. Unlike the
ordinary swells, this component is not multiplied by the minimum of those two
allowances, so it begins farther offshore while still responding to bathymetry.
This avoids phase seams as the direction bends. The result remains a
player-centred GPU field rather than duplicating direction data into the sea
mesh, so curved shores and multiple loaded islands share the same wave mesh and
the field is rebuilt only when coastal attenuation is recomposed.
The composed texture also retains the uncurved five-metre depth and raw
submerged-river-carve allowance. The complete wave field is scaled where necessary
so its conservative maximum vertical amplitude never exceeds reconstructed
local water depth. Displacement and analytic derivatives use the same scale,
avoiding the flat shelves and lighting creases produced by clamping individual
troughs against a seabed-clearance plane.
The generated per-island mask now uses RGBA rather than RG: blue marks finalized
river-bed coverage plus every earlier river pass that lowered channel terrain
below sea level, only where the finished terrain remains submerged. That
accumulated field includes outlet cuts created when river routing escapes an
inland basin and is interpolated through later terrain tessellations. The
player-centred composer turns the union into a smoothly interpolated allowance,
suppressing both directional swell and the separate onshore component in deep
carved channels without another global-ocean texture lookup.

## Goal

Add geometric ocean waves to the global deep-ocean surface while preserving the
current multi-island ownership model and keeping shallow water visually stable.
The system must:

- provide enough vertices around the player for the sea shader to displace the
  surface into visible waves;
- follow the player indefinitely without rebuilding geometry or revealing the
  finite ocean boundary;
- sample waves in stable world coordinates so an ocean-anchor snap does not
  move the pattern through the water;
- attenuate displacement to zero near every loaded island using the generated
  sea depth and land-distance masks;
- meet the existing flat coastal overlays without cracks, double water, or
  transparent-sorting artifacts;
- retain the current reflection, refraction, sun/moon, cloud-shadow, fog, and
  horizon behavior;
- have a bounded and measurable CPU, GPU, memory, and draw-call cost.

The first version is visual only. Buoyancy and physics wave queries are an
explicit later extension.

## Current System and Constraints

### Global deep ocean

`OceanSurfaceController` currently creates one Unity primitive plane, scales it
to the environment diameter, and parents it to the player-relative environment
anchor. `WorldEnvironmentController` keeps the ocean at global sea level and
snaps the environment in XZ as the player travels.

The current primitive has too few vertices for geometric waves when stretched
across the whole environment. Replacing it with one uniformly dense plane would
waste most vertices beyond the range where displacement is visible.

### Per-island coastal water

Each `IslandRuntime` owns a bounded flat coastal overlay and its own generated
RGBA sea mask. The mask contract is:

- red: shallow-water weight, 1 at land/sea level and falling to 0 at 5 metres
  of water depth;
- green: offshore distance from land, 0 at the coast and rising to 1 over 16
  metres;
- blue: finalized river-bed coverage plus accumulated submerged river-carve
  coverage from every terrain LOD pass, used to suppress waves in carved channels;
- alpha: reserved and always 1;
- above-sea terrain has red 1, green 0, and blue 0.

The overlay uses that mask for shallow tint, foam, and shore-wave animation.
The global `SeaWater` material deliberately has no `_SeaMask`, island size, or
island transform because several islands may be visible at once.

### Depth texture distinction

The camera depth texture used for refraction is screen-space, view-dependent,
and produced after vertex transformation. It cannot reliably decide how far a
sea vertex may move. Geometric-wave attenuation must use the generated island
sea masks, sampled in stable world/island coordinates before displacement.

### Reflection plane

`PlanarWaterReflection` reflects around the mean sea-level plane. That remains
the correct stable clipping plane even when the visible surface is displaced.
The wave normal can perturb reflection lookup without moving the reflection
camera every frame.

## Recommended Architecture

Use a fixed, player-centred ocean geometry clipmap rather than creating and
destroying independent GameObjects as the player moves.

The clipmap is logically divided into tiles/rings, but its stable sections
should be combined into one mesh and one renderer where practical. This gives
the desired high detail near the player and low detail far away without turning
each sea tile into a draw call or a lifecycle allocation.

```text
WorldEnvironmentController
└── OceanSurfaceController
    ├── OceanWaveProfile
    ├── OceanClipmapMesh
    │   ├── dense centre
    │   ├── progressively coarser stitched rings
    │   └── flat horizon skirt/ring
    └── OceanWaveMaskComposer
        ├── player-centred attenuation RenderTexture
        ├── derived onshore-direction RenderTexture
        └── active IslandCoastalWaterBinding values
```

The global ocean still belongs to `WorldEnvironmentController`. Islands only
register the data needed to suppress waves near their coasts; they never own or
replace the ocean mesh.

## 1. Configuration and Ownership

Add an `OceanWaveProfile : ScriptableObject` for reusable world-ocean settings.
The environment-authority island configuration may reference it initially, but
`WorldEnvironmentController` must receive an immutable validated settings
snapshot and remain the runtime owner. This keeps the wave system global even
while the existing scene still selects its environment through an
`IslandGenerator`.

Suggested settings:

- enable geometric waves;
- fine-grid vertex spacing;
- dense-centre diameter;
- number and scale of outer clipmap rings;
- distance at which displacement begins fading to zero;
- maximum displacement used to expand renderer bounds;
- four wave components: direction, wavelength, amplitude, speed, and optional
  choppiness;
- broad swell strength and local ripple strength;
- shoreline flattening response for the red depth and green land-distance
  channels;
- wave-mask coverage, resolution, and anchor-snap distance;
- a debug mode for clipmap rings, attenuation, and displacement amplitude.

Validation must clamp impossible combinations. In particular:

- the ocean anchor snap must be an integer multiple of fine-grid spacing;
- the attenuation texture must cover the entire displaced region plus its
  shore-transition padding;
- total Gerstner steepness must remain below a conservative fold-over limit;
- maximum displacement must include the sum of configured amplitudes.

Keep weather strength out of the first pass. A later weather controller can
interpolate between calm and storm profiles without changing mesh topology.

## 2. Player-Centred Clipmap Mesh

Replace `GameObject.CreatePrimitive(PrimitiveType.Plane)` in
`OceanSurfaceController` with a generated mesh that has:

1. a dense square or circular centre around the player;
2. concentric rings whose vertex spacing doubles at each level;
3. explicit stitch triangles or a morph band between different resolutions;
4. an outer flat ring/skirt extending beyond the useful camera range and under
   the opaque sky dome.

The initial target should favour predictable cost over maximum density. A
reasonable profiling baseline is 1-2 metre vertex spacing in the nearest
128-256 metres, followed by two or three progressively coarser rings. Exact
values remain profile settings rather than code constants.

The mesh is created once when the profile or environment diameter changes. It
is not regenerated during travel. The ocean root continues to follow the
player, snapped to the fine-grid spacing.

### Crack prevention

Do not rely on overlapping transparent planes or downward skirts between wave
LODs. They can expose seams through refraction and sorting. Prefer one of:

- stitch indices joining each fine ring to the next coarser ring; or
- an outer morph band that snaps fine boundary vertices onto the coarser grid.

Wave displacement must use the same function on both sides of every stitched
boundary. At the outermost ring, smoothly reduce displacement to exactly zero
before it meets the static horizon geometry.

### Draw calls

The preferred result is one mesh renderer and one sea material. If Unity mesh
index limits or culling make two renderers materially faster, split only into a
near clipmap and a far horizon ring. Do not create one renderer per logical
tile without profiler evidence.

### Bounds and culling

Set mesh bounds explicitly to include maximum horizontal and vertical wave
displacement. Otherwise Unity can cull crests whose undisplaced vertices are
outside the camera frustum.

## 3. Stable Wave Function

Add shared wave functions to a new include such as `OceanWaves.cginc` and call
them from `SeaWater.shader`.

The first implementation should use a small deterministic sum of directional
waves:

- one or two long, low-frequency swells;
- two shorter crossing components that break up repetition;
- stable logical world XZ and `_Time` as inputs;
- analytic or finite-derivative normals from the same height function.

Begin with vertical displacement only. It produces convincing swells while
being robust where shoreline attenuation changes quickly. Once flattening and
seams are proven, optionally add conservative horizontal Gerstner displacement
in deep water. Horizontal displacement must fade sooner and more gently than
vertical displacement so vertices cannot bunch up or fold near shore.

The vertex shader must output the displaced world position, clip position,
surface eye depth, and wave normal. The fragment shader must then use those
values for:

- sun/moon and cloud-shadow lighting;
- Fresnel and planar reflection distortion;
- refraction and opacity depth;
- fog.

Existing texture noise remains useful for small reflection/refraction ripples;
it should complement rather than duplicate the geometric normal.

### Coordinate continuity

Never sample the wave function from object-local clipmap coordinates. Use the
stable environment/logical world coordinate contract. When the ocean transform
snaps to follow the player, overlapping points before and after the snap must
evaluate to the same wave phase.

Before floating origin is introduced, normal world XZ is sufficient. The API
should nevertheless accept the existing bounded environment offset so Phase 7
can later provide high/low logical coordinates without rewriting the wave
function.

## 4. Player-Centred Shoreline Attenuation

The global sea shader cannot bind one island's mask. Introduce an
`OceanWaveMaskComposer` owned by `OceanSurfaceController` which produces one
player-centred attenuation texture covering the displaced clipmap.

### Island registration contract

Add an explicit `IslandCoastalWaterBinding` containing:

- the sea-mask texture;
- island world-to-local transform;
- island world size;
- island sea level if it can differ during transition code;
- active/runtime identity and bounds.

`IslandRuntime` registers this binding when activation completes and
unregisters it before dormancy, unload, or resource disposal. The composer must
not discover masks by scraping materials or searching scene objects.

Only bindings whose bounds overlap the attenuation coverage plus padding are
drawn. This naturally handles zero, one, or several nearby islands.

### Composition texture

Create a clamped single-channel R8 or RHalf render texture, depending on tested
Metal support. It stores final permitted geometric-wave amplitude:

- clear value 1 means full open-ocean displacement;
- each overlapping island projects its sea mask into player-centred world XZ;
- the projection shader decodes red depth and green land distance;
- the output approaches 0 on land and in shallow water;
- overlapping islands combine with minimum blending, so the most restrictive
  coast wins.

Recompose only when:

- the attenuation anchor moves by its configured snap distance;
- an island binding activates, moves, sleeps, or unloads;
- wave-flattening settings change.

Do not read the texture back to the CPU. The sea vertex shader samples it with
an explicit LOD, and the fragment shader may sample the same value for debug or
foam continuity.

### Flattening curve

Sample attenuation at the undisplaced world XZ position. A safe starting curve
uses both mask channels:

```text
depth allowance = inverse of the red shallow-water weight
distance allowance = smooth increase across the green offshore distance
wave allowance = min(depth allowance, distance allowance)
```

Tune the smoothstep ranges so amplitude is exactly zero at the shore, remains
small through the beach shallows, and reaches one before the 5 m depth / 16 m
distance mask ranges saturate. Square or otherwise ease the allowance before
applying it to horizontal displacement.

If visual testing shows that 5 m depth and 16 m distance are too narrow for the
configured wavelengths, extend the Rust mask contract deliberately and export
its physical ranges as metadata. Do not silently reinterpret the current byte
channels in Unity.

### Outside loaded islands

An unloaded island has no rendered terrain and therefore must not suppress
waves. Residency distances are far larger than the near wave-clipmap radius, so
an island should be active before its coast enters the displaced region. Add an
assertion and debug counter for the unexpected case where visible terrain has
no registered coastal binding.

## 5. Deep Ocean and Coastal Overlay Boundary

The coastal overlay remains responsible for shallow tint, foam, and its
existing shore-wave bands. The global deep-ocean surface remains responsible
for opacity, refraction, reflection, lighting, and geometric displacement.

To prevent the two surfaces separating:

- global geometric displacement must be exactly zero in the shallow overlay
  zone;
- the overlay stays at its current small vertical offset;
- the overlay samples the same global wave time/coordinates if its foam motion
  needs to follow offshore swells, but it does not duplicate deep-water
  displacement in the first version;
- the composed attenuation transition must finish before the overlay fades out
  at its patch edge;
- render queue and depth bias remain explicit and are verified from low grazing
  angles.

After the stable version works, a small attenuated displacement may be applied
to the outer part of the coastal overlay, using the identical wave function and
mask. Its inner edge must remain flat at the beach.

River surfaces and waterfalls are not part of this change.

## 6. Reflection, Refraction, and Lighting

Keep the planar reflection camera on the mean sea-level plane. Update only the
surface shading:

- perturb planar-reflection coordinates with the geometric wave normal plus
  the existing fine ripple;
- use displaced surface eye depth for scene-depth/refraction calculations;
- preserve shadow and cloud-light attenuation at the displaced world position;
- preserve the night exposure behavior that prevents water glowing;
- ensure the reflection replacement camera still excludes the Water layer.

Large geometric normals can make distortion unstable at grazing angles. Clamp
reflection and refraction offsets independently and fade their high-frequency
component with distance.

## 7. Performance Contract

The system should meet these initial constraints:

- zero mesh creation, destruction, or managed allocation during ordinary
  player travel;
- no per-frame island searches;
- one global ocean renderer if practical, two at most without profiling proof;
- attenuation recomposition only on snapped movement or island lifecycle
  changes;
- no attenuation CPU readback;
- a fixed upper bound on clipmap vertices and indices;
- no hardware tessellation dependency, keeping the Metal and general Unity
  path portable;
- shader variants limited to geometric waves on/off and an optional debug mode.

Expose counters for vertex count, rendered sections, mask recompositions,
overlapping coastal bindings, and last composition time. Measure the sea pass
with and without waves on the same camera path before increasing density.

## 8. Implementation Phases

### Phase 0: Capture the flat-ocean baseline

1. Capture screenshots and frame timings at open sea, beach approach, between
   two islands, elevated flight, sunset, and night.
2. Record current ocean draw calls, triangle count, sea shader GPU time, and
   reflection cost.
3. Preserve the existing flat ocean behind a profile toggle for A/B testing.

Gate: disabling geometric waves is visually and structurally identical to the
current ocean.

### Phase 1: Configuration and crack-free clipmap

1. Add `OceanWaveProfile` and a validated runtime settings snapshot.
2. Add a pure clipmap mesh builder with deterministic vertices and indices.
3. Replace the primitive plane in `OceanSurfaceController`.
4. Render it flat first; add ring/debug colours during development.
5. Add explicit expanded bounds and correct water-layer/reflection behavior.

Gate: the flat clipmap has no cracks, LOD laddering, z-fighting, horizon gap, or
anchor-snap jump.

### Phase 2: Deep-water geometric waves

1. Add shared wave evaluation and analytic normals.
2. Displace the dense and middle rings in stable world coordinates.
3. Fade displacement to zero in the outer ring.
4. Feed displaced position and normal through all current water shading paths.
5. Start vertical-only; enable horizontal choppiness only after fold-over tests.

Gate: waves remain continuous while stationary, walking, running, flying, and
crossing ocean-anchor snap points.

### Phase 3: Multi-island attenuation compositor

1. Add explicit coastal-binding registration to `IslandRuntime` lifecycle.
2. Add the player-centred attenuation render texture and projection shader.
3. Compose all overlapping active island masks with minimum blending.
4. Sample the result in the sea vertex shader before displacement.
5. Add attenuation and registration debug views.

Gate: waves flatten smoothly at every loaded beach and between two overlapping
island mask regions, with no dependence on which island is focused.

### Phase 4: Coastal, reflection, and lighting integration

1. Tune the deep-ocean/coastal-overlay transition.
2. Combine geometric normals with existing ripple distortion.
3. Verify reflection, refraction, shadows, cloud shadows, sun/moon lighting,
   fog, and night exposure.
4. Test low grazing camera angles and transparent ordering.

Gate: no double surface, shore gap, bright seam, detached foam, or beach
intersection is visible in the validation matrix.

### Phase 5: Profiling and hardening

1. Profile candidate grid spacings and ring counts on Metal.
2. Confirm zero steady-state allocations and bounded recomposition frequency.
3. Add automated mesh, lifecycle, and shader-contract validation.
4. Retain conservative defaults and document higher-quality profiles.
5. Remove debug-only shader paths from release variants if they add cost.

Gate: the selected default produces a measured acceptable frame-time increase
and no regression in island streaming or reflection-camera frame pacing.

### Optional Phase 6: Gameplay wave queries

If boats, swimmers, or floating objects need to follow the visible surface,
mirror the small deterministic wave function in C# or a Burst job using the same
validated component parameters and logical time. Shore attenuation can be
sampled approximately from nearby island data or a deliberately asynchronous
CPU field. Do not read the rendered displacement texture synchronously.

## 9. Automated Validation

Add focused tests for:

- deterministic clipmap vertex/index counts;
- finite vertices, normals, UVs, and bounds;
- valid winding and no degenerate triangles;
- exact positional agreement across every LOD stitch;
- zero outer-boundary displacement;
- anchor snapping aligned to fine-grid spacing;
- identical wave phase before and after an anchor snap at the same world point;
- finite wave output and conservative maximum displacement;
- attenuation clear value with no islands;
- correct world-to-mask projection for translated islands;
- minimum composition for overlapping island masks;
- registration/unregistration during activation, dormancy, and disposal;
- global sea shader requiring `_WaveAttenuationTex` but still not requiring an
  island `_SeaMask` or `_IslandWorldToLocal`;
- old flat-ocean behavior when waves are disabled.

Retain the existing Unity batch validation and add a small generated attenuation
fixture so validation does not depend on entering play mode.

## 10. Visual and Runtime Test Matrix

Test at:

- open deep sea;
- a broad shallow beach;
- a cliff coast;
- a river mouth;
- between two simultaneously visible islands;
- an island activating and unloading at the edge of residency;
- sea-level first person and elevated fly mode;
- stationary, slow travel, fast travel, and repeated anchor snaps;
- noon, sunset, cloud shadow, moonlight, and dark night;
- normal and planar-reflection views;
- calm default profile and deliberately exaggerated diagnostic waves.

Look specifically for:

- cracks or laddering between mesh resolutions;
- wave phase swimming when the ocean follows the player;
- crests intersecting beaches or terrain;
- lateral vertex fold-over in the attenuation gradient;
- coastal-overlay separation or double refraction;
- culling of high crests at screen edges;
- horizon gaps at high altitude;
- abrupt wave appearance when an island activates;
- changed night brightness or shadow response.

## Acceptance Criteria

The wave feature is ready when:

1. the player always has a sufficiently detailed displaced ocean surface around
   them without runtime tile creation;
2. wave phase is stable across player movement and environment-anchor snaps;
3. waves reach exactly flat water before intersecting loaded beaches and
   coastal overlays;
4. multiple nearby island masks suppress waves correctly at the same time;
5. the far ocean still reaches the opaque sky dome with no visible square edge;
6. reflection, refraction, lighting, fog, cloud shadow, and night behavior are
   preserved;
7. steady travel creates no managed allocations and keeps mesh/draw-call cost
   within the configured fixed budget;
8. disabling waves restores the current flat-ocean output for direct A/B
   comparison.

## Current Test Sequence

1. Open `Assets/Scenes/OceanWaveSandbox.unity` to tune the four vertical wave
   components without waiting for island generation.
2. Confirm the graded mesh is visually continuous while flying across anchor
   snaps and viewing it from sea level and high altitude.
3. Test the normal multi-island scene at a broad beach, a cliff coast, a river
   mouth, and between two simultaneously resident islands.
4. Profile the sea pass on Metal before enabling any horizontal choppiness or
   increasing the fine-grid radius.

The original isolated Phases 1-2 experiment is complete. The initial Phase 3
compositor is now present as well, so visual testing can distinguish open-sea
wave quality in the sandbox from coastal attenuation behavior in generated
islands.
