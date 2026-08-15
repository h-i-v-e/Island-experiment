# Hydraulic Hardness and Deposited-Material Plan

## Status

Implemented on 2026-08-12. The generator and installed Unity native plugin now
use the material model described below. The public Rust API, save inputs, C ABI,
and Unity controls remain compatible.

Implementation validation:

- `cargo clippy --all-targets --all-features -- -D warnings` passes;
- optimized tests pass: 53 library/FFI, 2 CLI, 22 generation, and 3 seam tests;
- the default seed produces 2,082,726 final terrain vertices and 96 rivers;
- a same-machine warmed comparison measured 39.33 seconds versus 30.67 seconds
  for the untouched generator, a 28.2% total increase while producing 17.8%
  more adaptive vertices (8.9% more time per generated vertex); and
- the installed Unity `libmotu.dylib` is byte-identical to the tested release
  artifact, SHA-256
  `8aae367ab6487584ce4f41d0de3bbbea158f3947d2de5be5566339c5a44582db`.

The measured total overhead is slightly above the plan's 25% review threshold.
Most of the increase is additional adaptive detail produced by resistant relief;
the remaining per-vertex cost is below 10%. Repeated fractal-noise sampling and
the first duplicate tessellation edge map were removed during implementation.

On 2026-08-12 the steep-slope movement direction was refined without changing
the erosion amount: retreat follows the surface normal through 45 degrees, then
blends smoothly toward the Z axis as the surface approaches vertical. A default
release run completed in 38.76 seconds with 2,069,165 vertices, compared with
39.33 seconds immediately before this change, so the hybrid has no measurable
performance penalty.

## Objective

Make hydraulic erosion respond coherently to the same deterministic noise field
that establishes the island's initial large-scale terrain, while tracking loose
material deposited on each terrain vertex separately from bedrock.

The completed system should:

- erode coherent soft regions faster than coherent hard regions;
- always erode loose deposited material before underlying bedrock;
- treat loose material as maximally erodible regardless of bedrock hardness;
- make river beds, banks, and outer valley rings respond to the same bedrock
  hardness while preserving a minimum-drainage incision rate;
- carry area-weighted material removed by river-valley formation through the
  downstream river network;
- carry the loose-material field through every terrain tessellation;
- initialize each newly inserted vertex from only vertices in the previous mesh
  generation;
- conserve total loose material when tessellation changes vertex density;
- retain the cached projected-face inversion guard;
- remain deterministic for a fixed seed and options; and
- add no allocation inside the hydraulic path walk.

## Interpretation of "the original noise field"

The Rust generator currently creates the initial land/sea score from
`coast::terrain_noise(seed, xy).height_component()`. That component combines
the continental and detail samples with weights `0.78` and `0.22`, before the
radial island falloff is applied.

Use that exact pre-falloff height component as the canonical bedrock signal:

```text
material_noise(xy) = terrain_noise(seed, xy).height_component()
```

The radial falloff must not contribute to hardness. It controls island shape,
not geology. The former 32/64/96/128/192-frequency surface perturbations did
not contribute either and have since been removed; otherwise small
render-scale bumps would have become tiny, isolated hard inclusions.

The legacy C++ generator used four randomized `NoiseLayer`s for its initial sea
classification and a binary rock decoration during hydraulic erosion. The Rust
port does not retain those exact four C++ layer offsets. This plan therefore
targets consistency with the current Rust initial terrain field, not
byte-for-byte reconstruction of the unavailable legacy field.

## Pre-implementation constraints

This section records the starting point that motivated the implementation; it
is retained for design history and no longer describes the current generator.

### Existing geology

`GeologyField` already derives a coherent hardness value from the same two
terrain-noise samples, but its private raw mix is currently `0.80/0.20`, not the
exact initial-height mix of `0.78/0.22`. It calibrates the 8th and 92nd
percentiles to `0..1` and applies smoothstep twice to strengthen the hard and
soft ends.

### Existing hydraulic sediment

Hydraulic sediment is currently a scalar local to one downhill path.
`exchange_sediment` returns a signed geometric shift:

- positive values raise Z and reduce carried sediment;
- negative values retreat along the current surface normal and add to carried
  sediment.

Once material is deposited, there is no persistent record that it is loose. A
later path therefore erodes it as if it were bedrock.

### Existing topology changes

Terrain refinement retains all old vertices and appends shared edge midpoints.
There are two relevant forms:

- returning refinement through `tessellated_*`; and
- in-place selective refinement through `tessellate_incident_to`, used by
  coastal and river refinement.

The returning methods currently discard midpoint lineage. The in-place method
returns endpoint/midpoint triples, which is sufficient for edge interpolation
but not for averaging all old surrounding vertices.

### Existing river-valley sediment

River shaping already records some excavated terrain, but only as a height-like
scalar local to each river:

- centreline bed lowering, river-mouth grading, and surrounding bank/valley
  lowering add `old_z - target_z` to the river's sediment value;
- when delta formation is enabled, tributary values are accumulated into their
  downstream rivers and terminal rivers spend part of the total on alluvial
  valleys and offshore deltas; and
- the final carve-only river pass deliberately does not form another delta, so
  its calculated sediment is currently discarded instead of being recorded as
  exported to the sea.

This is not yet a conserved material model. The scalar is not multiplied by a
vertex control area, waterfall shelf raises do not spend it, lateral smoothing
is not included, and all lowered vertices are cut directly to their target
without consulting bedrock hardness or persistent loose cover.

## Implemented material model

Introduce an internal generation-only type aligned one-to-one with terrain
vertices:

```rust
struct SurfaceMaterial {
    deposited_depth: Vec<f32>,
    bedrock_hardness: Vec<f32>,
}
```

`deposited_depth[i]` is non-negative loose-cover depth in normalized terrain
units. It is not exported in `Mesh`, the C ABI, textures, or Unity meshes. It is
owned beside each working terrain mesh inside `Island::generate` and passed by
mutable borrow to topology-changing and material-moving stages.

Do not put this field in public `Mesh`. Keeping it separate avoids increasing
every sliced/render/river mesh allocation and prevents an internal simulation
attribute leaking into the native ABI.

### Bedrock hardness

`GeologyField` remains the canonical deterministic source, but the implemented
model samples it once on the base mesh and stores the resulting material
identity in `SurfaceMaterial::bedrock_hardness`. New vertices interpolate that
identity through the same old-generation stencils as loose cover. Hydraulic and
river stages derive one contiguous erosion-rate cache from it:

```text
normalized_hardness = calibrated(material_noise(vertex.xy))
bedrock_rate = minimum_rate
             + (1 - minimum_rate) * (1 - normalized_hardness)^contrast
```

Recommended initial constants:

- `minimum_rate = 0.05`, so hard rock is resistant but never immortal;
- `contrast = 2.0`, giving broad soft basins and distinctly resistant ridges;
- loose deposited material rate = `1.0`.

Use constants for the first implementation. Add public/Unity controls only
after fixed-seed comparisons establish useful ranges.

### Loose-cover precedence

When hydraulic flow requests erosion at a vertex:

1. Calculate the unmodified erosion demand from capacity, slope, and hydraulic
   strength.
2. Apply the cached projected-face movement cap.
3. Satisfy as much of the permitted removal as possible from
   `deposited_depth[vertex]` at the loose-material rate.
4. If demand remains after the loose layer is exhausted, multiply only the
   bedrock portion by the cached bedrock erosion rate.
5. Move the vertex by the combined accepted distance along the negative live
   surface normal through 45 degrees, blending toward negative Z on steeper
   surfaces while the erosion amount itself continues to fade to zero.
6. Reduce loose cover by the accepted loose portion and add both loose and
   bedrock removal to carried sediment.

This must be represented by a result struct rather than another signed scalar,
for example:

```rust
struct HydraulicTransfer {
    normal_retreat: f32,
    vertical_deposit: f32,
    loose_removed: f32,
    bedrock_removed: f32,
}
```

That makes geometry, carried sediment, and the loose account independently
testable and prevents a partial safety-cap adjustment from silently creating or
destroying sediment.

### Depositing material

Every hydraulic deposition operation must update both geometry and the loose
account:

```text
vertex.z += deposited_height
deposited_depth[vertex] += deposited_height
```

This includes normal path deposition and every vertex touched by
`deposit_sediment_fan`. Fan distribution must calculate each vertex's accepted
deposit first, then subtract the exact applied total from carried sediment.

Deposits remain vertical because the current deposition model deliberately
settles material onto gentle slopes. The erosion model may later remove that
cover along the live normal; at the configured deposition slopes the difference
between vertical and normal depth is small.

## Hardness-aware river valleys and downstream sediment

River shaping must use the same `GeologyField` and `SurfaceMaterial` as
hydraulic erosion. Rebuild or extend a contiguous bedrock-rate cache after any
river tessellation and before carving. The centreline, bank, and outer-valley
walks must only index that cache; they must not sample fractal noise or allocate
inside the walk.

### Apply hardness to every lowered valley vertex

For every centreline, mouth, bank, shelf-edge, or outer valley vertex whose
current height is above its requested river target:

```text
requested_drop = max(current_z - target_z, 0)
loose_drop = min(requested_drop, deposited_depth[vertex])
remaining_drop = requested_drop - loose_drop
bedrock_drop = remaining_drop * effective_bedrock_rate
accepted_drop = loose_drop + bedrock_drop
```

Then lower the terrain only by `accepted_drop`, subtract `loose_drop` from the
loose account, and add both removed components to the river's carried-material
budget. This approaches the valley target over repeated river passes instead
of forcing resistant vertices to the complete profile in one operation.

The hardness effect must vary with channel distance:

- the centreline and graded outlet use a nonzero drainage floor so an isolated
  hard patch cannot dam the river;
- the first bank rings retain a smaller drainage floor to keep the active
  channel connected while allowing resistant banks; and
- outer valley rings use the full geological bedrock rate, allowing hard spurs
  and steep valley walls to survive beside wider soft corridors.

Use `effective_bedrock_rate = max(bedrock_rate, drainage_floor)` rather than a
separate hardness model. Recommended starting floors are `0.30` on the
centreline, smoothly decaying through the immediate banks to `0.0` on the outer
valley rings. These are internal constants until fixed-seed tests establish
safe tuning ranges.

Loose cover always has rate `1.0`, including old hydraulic deposits, river
alluvium, beaches, delta deposits, and material previously placed on waterfall
shelves. A thin loose-depth epsilon must be consumed and snapped to zero before
bedrock erosion is calculated.

### Use a volume budget, not a raw height sum

Replace the per-river sediment scalar with an explicit internal budget using
`f64` accumulators:

```rust
struct RiverSedimentBudget {
    carried: f64,
    loose_eroded: f64,
    bedrock_eroded: f64,
    deposited: f64,
    exported: f64,
}
```

For vertical river carving, approximate removed volume as:

```text
removed_volume = accepted_drop * projected_vertex_control_area
```

Calculate control areas once for the current topology and reuse them throughout
that carve stage. Geometry and `deposited_depth` remain `f32`; statistics and
conservation arithmetic use `f64`. Keep the budget as a compact value per river
and reuse existing river scratch storage so the change adds no allocation to a
bank or centreline walk.

### Carry, deposit, and export downstream

Material handling must be explicit for every river pass:

1. Add bed, mouth, bank, and outer-valley removal to the local river budget.
2. Transfer the remaining carried volume from every tributary to its joined
   downstream river regardless of whether delta formation is enabled.
3. In delta-enabled passes, let downstream alluvial-valley and delta placement
   spend the available carried volume. Every terrain raise must subtract the
   exact area-weighted volume applied and add the corresponding loose depth to
   `SurfaceMaterial`.
4. At an ocean outlet, record all unplaced material as exported.
5. In the final carve-only pass, perform no alluvial or delta terrain raising;
   aggregate the network normally and record every outlet balance as exported
   rather than silently discarding it. This preserves the existing anti-damming
   behavior.

Waterfall shelf or bank raises must no longer create geometry for free. They
must spend carried sediment and record loose cover, be limited to the available
volume, or be implemented as an explicitly volume-neutral redistribution from
nearby excavation. River smoothing and jiggle operations that change surface
volume must likewise either apply a local volume-neutral correction or report
their signed area-weighted change to the same budget.

## Tessellation propagation

### Return explicit old-generation stencils

Add an internal tessellation result without changing the existing public mesh
API:

```rust
struct TessellationResult {
    mesh: Mesh,
    new_vertices: Vec<NewVertexStencil>,
}

struct NewVertexStencil {
    vertex: u32,
    surrounding: [u32; 4],
    count: u8,
}
```

For a midpoint on old edge `(a, b)`, `surrounding` contains unique vertices
from the one or two old source triangles incident to that edge:

- boundary edge: `a`, `b`, and the one old opposite vertex;
- interior edge: `a`, `b`, and both old opposite vertices.

All stencil indices must be less than the old vertex count. No new vertex may
depend on another new vertex from the same tessellation. This gives the exact
"average surrounding vertices from the previous generation" behavior and
makes results independent of midpoint creation order.

Keep the current public `tessellated*` methods as thin wrappers which discard
the stencils. Add internal attributed variants used by terrain generation.
Extend `tessellate_incident_to` to return the same stencil information so river
and coastal refinement cannot lose loose-cover state.

### Initialize new loose-cover values

After tessellation:

1. Keep every old vertex's deposit value unchanged initially.
2. For each `NewVertexStencil`, set the new value to the arithmetic mean of its
   old surrounding values.
3. Calculate old and provisional new total deposited material.
4. Multiply every new-mesh deposit value by `old_total / new_total`.

### Define "total" as volume, not raw vertex sum

Because this is an irregular adaptively refined mesh, a raw sum of deposit
depths depends on vertex count. Conserving that sum would move material toward
regions merely because they received more triangles.

Use a barycentric vertex control area:

```text
control_area[v] = sum(abs(projected_face_area(face)) / 6)
deposit_total = sum(deposited_depth[v] * control_area[v])
```

`projected_face_area` is twice triangle area, hence division by six assigns one
third of actual triangle area to each vertex.

The required scale is therefore:

```text
scale = old_volume / provisional_new_volume
new_deposited_depth[v] *= scale
```

This is the requested old-total/new-total correction, with a physically useful
definition of total.

Edge cases:

- if `old_volume == 0`, fill the new deposit vector with zero and do not divide;
- clamp interpolated negative round-off to zero before totaling;
- if `old_volume > 0` but provisional volume is effectively zero, return an
  internal invariant error in tests and fall back to endpoint interpolation in
  release builds;
- calculate totals in `f64`, while retaining `f32` storage;
- target relative conservation error below `1e-6` per tessellation.

## Ownership and pipeline integration

Create `SurfaceMaterial::empty(vertex_count)` immediately after the base
Delaunay mesh. Carry it beside the active mesh through the complete topology
pipeline:

```text
base material
  -> LOD2 refinement/material propagation
  -> LOD1 refinement/material propagation
  -> second LOD1 refinement/material propagation
  -> broad LOD0 refinement/material propagation
  -> land LOD0 refinement/material propagation
  -> coarse coast in-place refinement/material propagation
  -> detail LOD0 refinement/material propagation
  -> river in-place refinement/material propagation
```

Every function which appends terrain vertices must either accept
`&mut SurfaceMaterial` or return stencils which its caller applies immediately.
Add debug assertions after each topology mutation:

```text
mesh.vertices.len() == material.deposited_depth.len()
```

The coarser exported LOD meshes do not need to retain material after they have
served as the source of the next generation. `correct_lods` continues to copy
positions only; the loose account belongs to the active simulation lineage, not
the exported render data.

## Implementation phases

### Phase 0: Baseline and fixtures - complete

- Record generation time, peak resident memory, vertex/face counts, and final
  terrain hashes for representative seeds at hydraulic strengths 1, 4, and 8.
- Add a test-only projected-area and material-volume diagnostic.
- Save fixed-seed images or mesh statistics for a soft basin, a hard ridge, a
  river valley, and a delta.

Acceptance:

- baseline commands and measurements are written into this file or a companion
  baseline document;
- existing inversion and determinism tests remain green before behavior changes.

### Phase 1: Canonical hardness field - complete

- Replace `TerrainNoiseSample::raw_hardness` with a canonical material component
  derived from the exact `height_component` mix.
- Retain percentile calibration and the double-smoothstep shaping initially.
- Make coastal and hydraulic erosion consume the same `GeologyField` result.
- Cache the hydraulic bedrock erosion rate once per vertex per stage; never
  evaluate fractal noise inside a path walk.

Acceptance:

- initial-height noise and hardness have strong positive fixed-seed
  correlation;
- coastal and hydraulic callers return identical hardness for the same seed/XY;
- hard and soft synthetic vertices show the expected rate ordering;
- no allocation or noise sampling occurs inside the hydraulic inner loop.

### Phase 2: Persistent loose-cover state - complete

- Add `SurfaceMaterial` with one deposit value per working terrain vertex.
- Thread it through `Island::generate`, `refine_lod1_again`, and hydraulic stages.
- Split `exchange_sediment` into explicit deposit/loose-erosion/bedrock-erosion
  results.
- Update normal deposition and deposit fans to record loose material.
- Remove loose cover before applying bedrock hardness.
- Keep the projected-face guard as the final geometric cap on combined retreat.

Acceptance:

- deposited material is removed before bedrock at a mixed test vertex;
- identical loose layers erode at the same rate over hard and soft bedrock;
- after loose exhaustion, soft bedrock erodes faster than hard bedrock;
- every stored deposit remains finite and non-negative;
- carried plus deposited material changes only by measured bedrock removal and
  deliberate outlet export.

### Phase 3: Tessellation stencils - complete

- Add fixed-size old-generation `NewVertexStencil`s to returning and in-place
  terrain tessellation.
- Keep public mesh-only wrappers for callers which do not own material fields.
- Assert that every stencil references only old vertices and contains no
  duplicate indices.
- Initialize new deposits from the old surrounding-vertex mean.

Acceptance:

- boundary midpoint stencils contain three old vertices;
- interior midpoint stencils contain four old vertices;
- selective/conforming refinement returns one stencil per appended vertex;
- output is independent of edge visitation order.

### Phase 4: Conservative deposit rescaling - complete

- Implement projected dual-area calculation for old and new meshes.
- Calculate old and provisional new volumes in `f64`.
- Apply the single global `old_volume / new_volume` multiplier.
- Cover zero-material and very-small-total cases explicitly.

Acceptance:

- uniform, land-only, displacement-selected, coastal, and river tessellation
  preserve loose-material volume within `1e-6` relative error;
- a zero deposit field remains bitwise zero;
- repeated tessellations do not drift systematically upward or downward;
- no NaN or infinity can be introduced by rescaling.

### Phase 5: All topology call sites - complete

- Propagate material through every `tessellated_displaced` call in
  `terrain.rs`.
- Propagate material through coast `tessellate_incident_to` calls.
- Propagate material through river-bed and waterfall refinement calls which
  modify the terrain mesh.
- Add length assertions immediately after every call.

Acceptance:

- no terrain topology mutation can leave material shorter than the vertex list;
- full generation succeeds for coast and river heavy fixed seeds;
- LOD prefix correction and Unity export remain unchanged.

### Phase 6: Hardness-aware river-valley excavation - complete

- Pass `GeologyField`, the stage-cached bedrock rates, and
  `&mut SurfaceMaterial` into river centreline, mouth, bank, and outer-valley
  carving.
- Remove loose cover first and apply bedrock hardness only to the remaining
  requested drop.
- Add a centreline drainage floor which decays across the bank rings and reaches
  zero before the outer valley rings.
- Approach resistant valley targets incrementally rather than assigning target
  Z unconditionally.
- Rebuild or extend the rate cache after river tessellation; do not sample noise
  or allocate inside the river walks.

Acceptance:

- a soft and hard synthetic valley with the same requested profile removes the
  same loose cover first, then cuts more deeply into the soft bedrock;
- hard centreline vertices continue to descend by at least the drainage floor
  and cannot create an isolated upstream-facing dam;
- hard outer rings retain visibly steeper banks or spurs than soft outer rings;
- every accepted terrain drop is represented in the river material budget;
- fixed-seed river carving remains deterministic and adds no inner-loop heap
  allocation.

### Phase 7: Conserved downstream river sediment - complete

- Replace height-only per-river sediment with area-weighted `f64` volume
  budgets.
- Transfer tributary balances downstream in both delta-enabled and carve-only
  passes.
- Make alluvial valleys, deltas, and waterfall shelf raises spend exact carried
  volume and record the result as loose cover.
- Record unused outlet material as exported.
- Keep the final river pass carve-only, but export rather than discard its
  sediment balance.
- Add stage statistics for local excavation, tributary input, deposition, and
  outlet export.

Acceptance:

- a branched synthetic network delivers the sum of both tributary balances to
  the main stem within `1e-6` relative error;
- delta-enabled passes balance eroded volume against deposited, carried, and
  exported volume;
- the final carve-only pass performs no terrain raise and exports its complete
  outlet balance;
- changing local tessellation density does not materially change generated
  sediment volume;
- no waterfall shelf or alluvial raise occurs without a matching reduction in
  carried material and increase in loose cover.

### Phase 8: Integrate other unconsolidated deposits - complete

Recommended after hydraulic-only behavior is stable:

- mark thermal material received by a lower vertex as loose;
- feed coastal beach deposition into the same loose-cover field instead of a
  short-lived private `soft_cover` only;
- mark river alluvium, raised valleys, and delta deposits as loose;
- remove loose material first in coastal erosion as well as hydraulic erosion.

This avoids contradictory material semantics where hydraulic deposits are soft
but visually identical beach or delta deposits are treated as bedrock.

Acceptance:

- each material-moving subsystem has an explicit source, carried, deposited,
  and exported volume balance;
- no subsystem silently raises terrain without declaring whether the material
  is bedrock or loose cover.

### Phase 9: Diagnostics and tuning - complete for internal diagnostics

- Add test-only summaries for hardness distribution, loose-cover volume,
  bedrock removal, loose re-erosion, river-network transfer, and exported
  sediment.
- Optionally expose hardness and loose-cover debug textures in Unity, without
  adding gameplay controls.
- Compare fixed seeds with hardness disabled, enabled, and exaggerated.
- Tune `minimum_rate` and `contrast` only after checking cliffs, valleys,
  beaches, deltas, and inversion counts together.

Acceptance:

- hard noise ridges remain visibly more resistant across multiple hydraulic
  stages;
- loose deposits do not armour hard ridges indefinitely;
- deposits migrate toward gentle slopes instead of disappearing during
  refinement;
- river-valley width and bank steepness follow coherent hard and soft geology
  without blocking centreline drainage;
- final carve-only river excavation is visible in exported-volume diagnostics
  and does not raise or dam river mouths;
- no material projected-face reversals exceed the existing `1e-10` tolerance.

### Phase 10: Performance, documentation, and Unity plugin - complete with reviewed overhead

- Run formatting, the complete Rust test suite, strict Clippy, and release build.
- Benchmark the same fixed cases used for the projected-face guard.
- Confirm hardness noise is stage-cached and tessellation creates only bounded
  stencil/material buffers.
- Update README behavior and rebuild/copy the macOS `libmotu.dylib`.
- Run Unity native interop and streamed seam validation.

Acceptance:

- no per-path or per-vertex heap allocation is added to hydraulic erosion;
- target total generation overhead is below 15%, with 25% requiring explicit
  review before acceptance;
- native allocation/release tests and Unity mesh generation remain balanced;
- the installed plugin is byte-identical to the verified release artifact.

## Suggested improvements beyond the core request

### 1. Use area-weighted conservation from the beginning

This is the most important improvement. Raw vertex sums are resolution
dependent and would bias sediment toward highly tessellated land. Projected
control-area weighting makes the same rescaling rule conserve an approximation
of real material volume.

### 2. Use continuous hardness with a nonzero erosion floor

The original C++ hydraulic pass stopped erosion entirely on binary rock
vertices. A continuous field will form broader coherent ridges and headlands
without creating immortal single-vertex dams. The nonzero hard-rock floor is
important for drainage stability.

### 3. Keep hardness fixed as material identity

Hardness is sampled once on the base mesh and then interpolated when topology
changes. Vertices can move laterally without changing material identity, and no
fractal-noise sampling occurs in hydraulic or river walks.

### 4. Track explicit material budgets

Return stage statistics:

```text
bedrock_eroded
loose_re_eroded
deposited
exported
loose_remaining
```

These make conservation regressions immediately visible and will be more useful
than judging the result only from screenshots.

### 5. Unify all loose sediment eventually

Hydraulic, thermal, coastal, river-valley, beach, and delta deposits should
ultimately share one unconsolidated layer. Implementing hydraulic deposits
first is the safest scope, but the common `SurfaceMaterial` type should be
designed so the other stages can join without another data-model rewrite.

### 6. Preserve a carve-only final river pass

The final pass exists to reopen outlets after earlier delta/alluvial shaping.
Do not spend its sediment on another terrain raise. Conserving this pass means
measuring and exporting its material, not reintroducing the outlet deposition
that previously formed dams.

### 7. Add hysteresis around exposed bedrock

Use a small loose-depth epsilon when switching between loose and bedrock rates.
Without it, floating-point traces of deposit can repeatedly switch erosion
rates. Values below the epsilon should be consumed and snapped to zero.

### 8. Do not add Unity sliders initially

First validate fixed constants against several seeds. Exposing hardness contrast
and minimum bedrock rate too early would make it difficult to distinguish a
model defect from poor tuning. Add controls only if one parameter set cannot
cover the desired terrain range.

## Principal risks

- Averaging and rescaling loose depth changes the internal bedrock/cover boundary
  without changing surface geometry; area-weighted totals minimize but do not
  eliminate this discretization effect.
- Strong hardness contrast can create drainage dams. The minimum erosion floor
  that decays across the bank rings and the existing final carve-only river
  pass must remain enabled.
- Area-weighted river sediment changes the scale of existing delta and alluvial
  budgets. Recalibrate placement rates using fixed-seed volume diagnostics
  rather than preserving the old height-sum constants numerically.
- If resistant outer rings are allowed to miss their target in one pass, too
  few river passes may leave narrow valleys. Validate convergence across the
  existing sequence before increasing rates or adding more passes.
- Threading material through coast and river tessellation touches several APIs;
  missing one call site would misalign vertex attributes.
- Converting carried river sediment from the current height-like scalar to
  physical volume is a larger behavioral change. Implement it only after the
  depth account is stable, as Phase 7 specifies, and retain comparative fixtures
  for valley and delta scale.
- Coastal `soft_cover` used to be local to one coastal stage; Phase 8 replaced
  it with the shared persistent loose-material field.

## Definition of done

1. Bedrock hydraulic erosion uses the canonical initial terrain-noise field and
   produces deterministic coherent hard/soft regions.
2. Every terrain vertex has a persistent non-negative hydraulic deposit account
   throughout generation.
3. Deposited material erodes before bedrock and at the full soft-material rate.
4. River centreline, banks, and outer valley rings use the same hardness field,
   with a decaying drainage floor that prevents hard-rock dams.
5. River excavation, tributary transfer, alluvial/delta deposition, and outlet
   export balance area-weighted material volume; the final carve-only pass
   exports material without raising the outlet.
6. New tessellation vertices use only the previous generation's surrounding
   vertices to initialize deposits.
7. Area-weighted deposited volume is conserved across every terrain
   tessellation within `1e-6` relative error.
8. The projected-face inversion guard remains active for every inward hydraulic
   move.
9. No terrain topology path can desynchronize mesh vertices and material state.
10. Determinism, mesh validity, rivers, textures, LOD seams, native ownership,
   and Unity loading tests pass.
11. Release performance remains within the accepted budget and the Unity plugin
   is rebuilt from the verified Rust artifact.
