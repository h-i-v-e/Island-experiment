# Forest placement, combined meshes, and terrain LOD integration

## Status and authority

This document is the source of truth for the island forest-placement phase.
It is intentionally separate from `PROCEDURAL_TREE_AND_FOREST_PLAN.md`, which
describes the earlier standalone procedural-tree milestone. Where that older
document calls forest placement "deferred", this document defines the work
that is to happen after the standalone tree generator and its LOD0/LOD1 wood
and foliage outputs are validated.

## Scale-aware placement revision — 2026-08-26

Tree scale is now derived from coherent coverage relative to the configured
forest threshold. Coverage at the threshold maps to the configured minimum;
coverage `1.0` maps to the configured maximum. The next finer octave perturbs
the middle of that range and fades to zero at both endpoints, preserving the
exact minimum and maximum while adding local variation. Default placement scale
is `1.0` through `2.0`, replacing the unrelated per-tree random `0.85` through
`1.15` range.

The same scale is carried into each foliage crown. Branch-tip supports,
connection limits, crown padding, canopy height, boundary expansion, and tip
clearance all scale together; a uniformly doubled tree therefore produces a
uniformly doubled coarse canopy rather than a large trunk inside a mostly
fixed-size foliage shell.

Each accepted tree reserves a centre-to-centre clearance of `3 metres *
scale`. A pair is rejected when its distance is within the larger of its two
clearances, so both trees' individual zones are respected. Higher coarse
coverage still wins contested positions. Changing the forest threshold now
intentionally remaps tree scale and its corresponding clearance.

## Implementation status — 2026-08-24

The code phases in this plan are implemented. The verified state is:

- Rust placement walks final LOD0 vertices exactly once and applies the sea,
  snowline, 22-degree slope, exact river-marker, final zero-soil, strict
  shader-equivalent final-anchor beach, coherent-noise-threshold,
  deterministic triangle-fan displacement, and actual 2-metre final-anchor
  exclusion rules.
- Settled-rock soil clearing runs before forest selection. There is no rock
  intersection query, river clearance radius, or trunk-footprint test.
- Rust owns four deterministic combined streams (`lod0_wood`,
  `lod0_foliage`, `lod1_wood`, and `lod1_foliage`) plus whole-tree ranges and
  owner-tile extraction.
- The native ABI preserves the historical 16-float `CreateMotu` options block
  and adds `MotuForestOptions`, `CreateMotuWithForest`, and fixed-grid wood and
  foliage exports.
- Save format version 17 persists all forest settings; versions 3 through 16
  load the documented defaults.
- Unity prepares forest arrays off the main thread and streams LOD2 low
  foliage, LOD1 low foliage and wood, and LOD0 full foliage and wood without
  per-tree GameObjects.
- Rust formatting, forest/tree tests, strict all-target Clippy, release build,
  deployed checksum comparison, the full ignored FFI lifecycle test, Unity
  6000.5.6f1 runtime/editor compilation, and fresh-process Unity native
  interop validation passed.
- The 2-metre beach-exclusion revision passes 190 library tests with one
  intentionally ignored slow lifecycle test, 18 focused forest tests, 11
  executed FFI tests, strict all-target Clippy, and isolated Unity 6000.5.6f1
  native interop validation. Its release and deployed macOS plugin SHA-256 is
  `36c6eb0fbb1ef7322e38fb23763f30d8e0a732bf7d78a4bb7c2752dfe02ea2ea`.
- For seed 666 with the IslandSandbox settings, the final-anchor beach and
  spacing rules accept 9,771 trees. The shared canopy contains 400 disconnected
  coarse components, 371,396 coarse triangles, and 1,485,584 detailed triangles.

Remaining validation is live visual and performance QA in the user's open
Unity project: restart that editor so it reloads the deployed dylib, inspect
multiple seeds, traverse both LOD boundaries, and profile preparation memory,
upload time, frame time, and draw calls. The full Rust all-target run had 200
passing tests but two pre-existing terrain/river integration assertions still
failed individually (`rivers_are_continuous_flowing_terrain_submeshes_with_waterfalls`
and `terrain_topology_is_free_form_delaunay`). Forest generation runs only
after terrain geometry is finalized and does not mutate that geometry, so
those failures were recorded rather than changed as part of this forest task.

At the time this plan was written, the working tree already contained
unfinished tree-generator, FFI, Unity preview, material, shader, scene, and
native-plugin changes. Those changes are user work and must be preserved.
Before implementing this plan, inspect `git status`, `git diff HEAD`, and all
relevant untracked files again. Do not broadly stage or replace them.

## Required outcome

Generate deterministic forests from coherent noise and the final LOD0 terrain
vertices.

The placement and rendering contract is:

1. Visit every unique vertex of the finalized global terrain LOD0 mesh exactly
   once.
2. A vertex is a tree candidate when coherent forest noise at its XY position
   is strictly greater than a configurable threshold.
3. Eliminate a candidate when its source terrain vertex is:
   - in the sea;
   - in a river;
   - on rock;
   - on terrain whose slope is greater than 22 degrees;
   - at or above the snowline.
4. Give each remaining candidate a deterministic final anchor by selecting one
   incident triangle from its terrain-vertex fan and interpolating zero to 50%
   of the way from the source vertex to that triangle's centroid. A vertex with
   no incident triangle remains undisplaced.
5. Resolve displaced candidates by descending coherent-noise coverage. Reject
   a candidate when its final anchor is beach, or when any already accepted
   final anchor is within the larger tree's `3 metres * scale` clearance in XY,
   regardless of terrain adjacency. Break equal-coverage ties by
   ascending vertex index, then emit accepted placements in ascending vertex
   order. Do not apply a target count, separate random thinning, or silent
   geometry cap.
6. Combine all trunks and branches into the wood mesh stream.
7. Combine all leaf clusters into the foliage mesh stream.
8. Treat those streams like the existing combined stones/boulders and river
   features: Rust owns authoritative combined geometry and exports spatial
   batches for Unity streaming. Do not create one GameObject or Mesh per tree.
9. Render the forest at terrain LODs as follows:

   | Terrain visual LOD | Foliage geometry | Wood geometry |
   | --- | --- | --- |
   | LOD2 | LOD1 low-poly foliage | hidden |
   | LOD1 | LOD1 low-poly foliage | LOD1 low-poly wood |
   | LOD0 | LOD0 full foliage | LOD0 full wood |

LOD2 and LOD1 intentionally reuse the same low-poly foliage topology. LOD2 is
cheaper because it does not render wood.

## Definitions that must remain unambiguous

### Final LOD0 vertex

"LOD0 vertex" means a vertex in the one global LOD0 terrain mesh after:

- final river carving and waterfall work;
- terrain LOD correction;
- final terrain height changes; and
- final LOD0 normal calculation.

Do not generate candidates from random UV samples, an image grid, LOD1/LOD2,
Unity terrain tiles, or vertices created later by mesh slicing. Iterating the
global mesh before export prevents duplicate candidates on tile boundaries.

### Tree anchor

The tree root anchor is the deterministic final position selected from the
LOD0 vertex fan. The tree grows toward Rust world Z/Unity world Y. A tree may
receive a seeded yaw rotation and uniform scale, but it must not be tilted to
match the terrain normal. During wood assembly, every open trunk-base vertex
is sampled against the final LOD0 surface at its transformed XY position. This
conforms the root perimeter to slopes without bending the whole tree or moving
its foliage supports.

### Sea

Sea level is normalized Rust height zero. Reject `vertex.z <= 0.0`. This is a
hard placement rule and must not depend on whether the sea renderer is visible.

### Slope

Use the finalized, normalized LOD0 vertex normal. Avoid `acos` and compare its
up component directly:

```text
maximum slope = 22 degrees
minimum normal.z = cos(22 degrees) = approximately 0.92718387
reject when normal.z < minimum normal.z
```

Exactly 22 degrees is eligible. Anything steeper is not.

### Snowline

The authoritative placement snowline is a physical height in metres. Convert
the normalized Rust vertex height with `ISLAND_WORLD_METRES` and reject when:

```text
height_metres >= snowline_metres
```

Unity terrain and grass materials must receive the same base snowline setting.
The current shaders add visual noise around that line; the placement rule is
based on the shared base line unless a later task explicitly makes the shader's
noisy edge CPU-authoritative too. Do not independently hard-code `100` metres
in Rust and Unity.

### River

Use the final per-LOD0-vertex `river_bed` mask produced by river generation.
Reject a candidate immediately when `river_bed[vertex_index]` is true. This
makes the root test follow the carved river rather than an approximate
centerline.

For this phase, "the trunk is not in the river" is defined solely by the root
vertex not being marked in `river_bed`. Do not test the area of the trunk foot,
the scaled trunk radius, the final river triangles, or a distance from the
river. Branches and foliage are explicitly allowed to extend over the river,
and a tree rooted on an unmarked bank vertex remains eligible even when some of
its geometry overhangs the water.

### Rock

Use final deposited soil depth as the only rock-placement proxy. Reject a tree
when the candidate LOD0 vertex has zero soil. In floating-point code, treat
`deposited_depth <= LOOSE_DEPTH_EPSILON` as zero rather than requiring exact bit
equality.

Settled stones and boulders already clear loose soil from their supporting
terrain vertices. Apply that soil clearing before evaluating tree candidates,
so those newly zero-soil vertices are rejected naturally. Do not build a rock
collision index and do not test tree geometry, trunk radius, stone radius,
boulder radius, or combined rock mesh bounds.

Evaluate the river rule before the zero-soil rule for diagnostic counts because
river-bed vertices may also have zero soil. The final accepted set is the
conjunction of all rules, so diagnostic precedence must not affect placement.

This is intentionally a practical proxy rather than exact collision avoidance
or exact visual-rock parity. Some tree geometry may intersect a rock whose
supporting zero-soil vertex is not the tree's own vertex. That is accepted for
this phase. The Unity shader's render-only rock variation is also not an
additional tree rejection source.

### Beach

Evaluate beach suitability at the final displaced anchor. Interpolate the
shader's loose-cover and sea-proximity inputs from the selected incident face,
then apply the terrain shader's two-to-four-metre altitude fade and generated
32-metre sand-patch noise with its existing RG seeds, weighting, UV offset, and
50-percent antialias boundary. A sand-capable coastal deposit remains excluded
when the render-only exposed-rock layer lies over it; rock does not make beach
soil suitable for a tree.

## Coherent forest field

Use the crate's deterministic `noise::fractal` function with a dedicated seed
domain. Do not reuse moisture, geology, grass, river-bank, or tree-shape salts.

The field must use physical metre coordinates so its patch size remains stable
when terrain resolution changes:

```rust
let point_metres = vertex.truncate() * ISLAND_WORLD_METRES;
let raw = noise::fractal(
    island_seed ^ FOREST_NOISE_DOMAIN,
    point_metres.x / options.patch_size_metres,
    point_metres.y / options.patch_size_metres,
    options.octaves,
);
let coverage = raw.mul_add(0.5, 0.5).clamp(0.0, 1.0);
let selected = coverage > options.threshold;
```

Initial tuning values, to be confirmed visually and with geometry counts:

- patch size: 160 to 240 metres;
- octaves: 4;
- normalized threshold: begin around `0.62`;
- comparison: strictly `>` rather than `>=`.

The threshold controls both population and the start of the scale ramp.
Raising it removes candidates below the new cutoff and remaps retained scales;
because clearance follows scale, the final accepted spacing set may also
change.

## Displaced anchors and actual exclusion zone

After habitat and threshold rejection, prioritize candidates by descending
coherent-noise coverage. Derive two independent stable keys from the island
seed and source terrain-vertex index: one selects an incident triangle from the
vertex fan, and one selects an interpolation amount in `[0, 0.5)`. Interpolate
the complete XYZ position toward that triangle's centroid so the root stays on
the source face while straight vertex rows are broken up.

Insert accepted final anchors into a spatial hash whose cell size is the
maximum configured `3 metres * scale` clearance. Reject each candidate when an
accepted neighbour is within the larger tree's clearance. This is a physical
exclusion zone and does not depend on whether the two source vertices share a
terrain edge.

The higher-noise candidate wins a contested pair; equal coverage is resolved
by lower terrain vertex index. Accepted placements are sorted back into
ascending terrain vertex order before appearance generation and mesh assembly.

Changing only the threshold does not change a retained tree's displaced anchor,
yaw, prototype choice, or prototype geometry. It intentionally remaps scale
between the configured minimum and maximum, which also changes the scale-aware
clearance. All stochastic variation remains derived independently from stable
keys rather than a sequential RNG.

## Configuration

Add a cohesive forest configuration rather than scattering constants:

```rust
pub(crate) struct ForestOptions {
    pub patch_size_metres: f32,
    pub noise_threshold: f32,
    pub noise_octaves: u8,
    pub snowline_metres: f32,
    pub prototype_count: u8,
    pub minimum_scale: f32,
    pub maximum_scale: f32,
}
```

The 22-degree maximum slope is initially a named contract constant rather than
a casual magic number. It can become a setting later only through an explicit
change to this requirement.

Validate before generating:

- every float is finite;
- patch size and snowline are positive;
- threshold is in `[0, 1]`;
- octaves and prototype count are non-zero and bounded;
- scales are positive and maximum scale is at least minimum scale; and
- any estimated capacity multiplication is checked.

Add corresponding `IslandForestSettings` fields in Unity:

- show forests;
- forest patch size in metres;
- forest noise threshold;
- snowline in metres;
- prototype count; and
- minimum/maximum tree scale.

Convert metre fields through the same canonical-native-world conversion used
for river widths and depths so changing the Unity island world size does not
change their physical meaning.

If these settings become part of `IslandOptions`, update all of the following
together:

- Rust defaults and validation;
- Rust save/load format, including a format-version bump and old-version
  defaults;
- `MotuOptions` in `src/ffi.rs`;
- the C declaration in `include/motu.h`;
- `MotuNative.Options` in Unity;
- ABI size/layout assertions; and
- example/readme documentation.

## Rust data ownership

Create a focused `src/forest.rs` module. Keep procedural tree-shape generation
in `src/trees.rs`; forest placement and island-wide batching are separate
responsibilities.

Suggested types:

```rust
pub(crate) struct TreePlacement {
    pub terrain_vertex: u32,
    pub anchor: Vec3,
    pub yaw_radians: f32,
    pub scale: f32,
    pub prototype: u8,
}

pub(crate) struct ForestMeshes {
    pub lod0_wood: Mesh,
    pub lod0_foliage: Mesh,
    pub lod1_wood: Mesh,
    pub lod1_foliage: Mesh,
    pub trees: Vec<ForestTreeRanges>,
}

pub(crate) struct ForestTreeRanges {
    pub anchor: Vec3,
    pub lod0_wood: MeshRange,
    pub lod0_foliage: MeshRange,
    pub lod1_wood: MeshRange,
    pub lod1_foliage: MeshRange,
}
```

`MeshRange` must be sufficient to copy a whole placed tree from the combined
source into its owner tile without clipping it. Keep placement order equal to
ascending LOD0 vertex index for stable output and straightforward tests.

Store the completed `ForestMeshes` on `Island`, beside `river_mesh` and
`river_rock_mesh`. Provide focused read-only accessors; do not expose mutable
mesh internals through the public API.

## Placement pipeline

Refactor decoration generation into explicit stages:

1. Generate and settle stones/boulders and collect their cleared-soil vertex
   indices.
2. Clear loose soil on those vertices before evaluating any tree candidate.
3. Generate forest placements from LOD0 vertices, the river-bed mask, and final
   deposited soil depths.
4. Preserve bush generation as its own existing behavior unless a separate
   request changes it.
5. Assemble forest meshes from the accepted placements.
6. Append the already settled stones/boulders to their existing combined rock
   mesh.

For every LOD0 vertex, validate parallel arrays before entering the loop:

- `vertices.len() == normals.len()`;
- `vertices.len() == river_bed.len()`;
- `vertices.len() == material.depths().len()`.
- `vertices.len() == material.sea_proximities().len()`.

The candidate loop should be a glanceable function or stateful builder rather
than a long block inside `Decorations::generate`.

Use this diagnostic precedence:

1. invalid/non-finite input;
2. below or at sea level;
3. at or above snowline;
4. steeper than 22 degrees;
5. river-bed marked vertex;
6. zero-soil vertex;
7. shader beach at the final displaced anchor;
8. forest noise not above threshold;
9. final displaced anchor within the larger tree's scale-aware clearance;
10. accepted.

Noise may be evaluated later in the implementation for performance, provided
the result and diagnostic accounting remain deterministic.

Record generation statistics:

```text
total LOD0 vertices
invalid
sea
snowline
slope
river-bed marked vertex
zero soil
shader beach at final anchor
below/equal noise threshold
final-anchor exclusion zone
accepted trees
```

The sum of exclusive rejection counts plus accepted trees must equal the total
candidate count.

## Zero-soil timing

The zero-soil proxy is useful only if it observes the material after settled
stones and boulders have marked their supporting terrain vertices for soil
clearing. Split the current combined decoration step so the order is:

1. settle rocks and collect `cleared_soil_vertices`;
2. apply `clear_loose_soil` to the authoritative `SurfaceMaterial`;
3. evaluate tree candidates against the updated deposited-depth array; and
4. build the final exported `TerrainMaterialField` from that same material.

Do not compare against a provisional material field captured before soil
clearing. Do not add a follow-up pass that deletes trees based on rock geometry.

## Stable placement variation

Derive a placement key from the island seed and terrain vertex index:

```text
placement_key = hash(island_seed, terrain_vertex, FOREST_PLACEMENT_DOMAIN)
```

Use separate subdomains for:

- prototype selection;
- yaw;
- uniform scale; and
- any future colour/wind phase.

Do not use the forest-mask noise value as a random seed. Forest membership and
tree appearance must remain independent controls. Growth habit is the one
deliberate spatial exception: sample a separate 90-metre coherent field to
form patches of upright, rounded, and spreading trees, then use the placement
key to select a distinct prototype within that habit. This avoids both random
species confetti and repeated identical trees.

## Prototype library

Generate a deterministic prototype library once per island rather than running
the full branching algorithm separately for every placement. Prototype indices
cycle through upright, rounded, and spreading habits; each habit varies trunk
allometry, crown height, bend, branch elevation, internode length, and taper.
Expose the count through validated options.

The existing tree generator already produces:

- LOD0 wood;
- LOD0 foliage;
- LOD1 wood; and
- LOD1 foliage.

Refactor it so the reusable internal tree is centred at local `(0, 0, 0)` with
its root on `z = 0`. The standalone `TreeSandbox` preview may translate that
local tree to the existing preview convention. Forest assembly must not have
to subtract a hidden `(0.5, 0.5, 0)` origin for every vertex.

Prototype seeds must use a tree-prototype domain separate from forest placement
and the standalone preview. Prototype generation must remain deterministic for
the island seed and prototype index.

## Combined forest mesh assembly

Create one logical wood stream and one logical foliage stream, each with LOD0
and LOD1 geometry. For each accepted placement:

1. Fetch its selected prototype.
2. Apply uniform scale around the prototype root.
3. Rotate positions and normals around world Z by the seeded yaw.
4. Translate positions to the final anchor.
5. Pin each open wood base vertex to the final terrain height at its transformed
   XY position, then recalculate the assembled wood normals.
6. Append LOD0 wood to combined LOD0 wood.
7. Append LOD0 foliage to combined LOD0 foliage.
8. Append LOD1 wood to combined LOD1 wood.
9. Append LOD1 foliage to combined LOD1 foliage.
10. Offset triangle indices with checked arithmetic.
11. Record the four contiguous ranges for that placed tree.

Introduce a small mesh-appender helper that owns the repeated index-offset,
reserve, transform, and validation logic. Do not pass several parallel mesh
buffers through long argument lists.

Prototype normals are already calculated. Rotate and copy them; do not
recalculate normals across unrelated trees in the combined mesh. Preserve UVs
if the source contains them.

Before allocation, estimate combined counts with checked multiplication. If
the requested threshold produces geometry that cannot fit the mesh/index
contract, return a clear generation error with accepted-tree and projected
geometry counts. Never wrap indices or silently skip eligible trees.

## Spatial batching without splitting trees

Rust should own the combined source streams, but Unity should consume spatial
tiles just as it does for terrain, rivers, and stones/boulders.

Do not call the generic geometric `Mesh::sliced_grid` for forest meshes. It can
cut trunks and foliage at tile boundaries, allowing one tree to be rendered as
mixed LOD pieces when neighboring cells are at different detail levels.

Instead:

1. Choose the owner tile from the tree root anchor.
2. Use `ForestTreeRanges` to copy the entire tree into that tile's wood and
   foliage outputs.
3. Rebase indices within the output tile.
4. Let the Unity Mesh bounds include geometry that extends beyond the owner's
   nominal XY tile.

This retains combined draw batches while guaranteeing that an individual tree
switches LOD atomically.

## Native export contract

Add island-owned grid exports using the existing `ExportMeshGrid` ownership
model:

```c
CreateForestWoodMeshGrid(handle, area, visual_lod, divisions, output)
CreateForestFoliageMeshGrid(handle, area, visual_lod, divisions, output)
```

Map visual LOD to source geometry exactly:

```text
wood visual LOD0    -> lod0_wood
wood visual LOD1    -> lod1_wood
wood visual LOD2    -> valid empty grid
foliage visual LOD0 -> lod0_foliage
foliage visual LOD1 -> lod1_foliage
foliage visual LOD2 -> lod1_foliage
```

Requirements:

- initialize outputs to defaults before early returns;
- validate handle, output, LOD, bounds, and divisions;
- return the requested fixed grid length even when some or all tiles are empty;
- use `ReleaseMeshGrid` for ownership cleanup;
- ensure an empty LOD2 wood result is valid and releasable; and
- update the C header, Unity declaration, Rust layout tests, and FFI allocation
  lifecycle test together.

Retain `CreateProceduralTree` for the standalone preview. Forest export is
island-owned and must not regenerate the terrain or prototype library.

## Prepared Unity data

Extend the background `PrepareIsland` flow with a cohesive forest container:

```text
LOD2 foliage tiles, coarse 8 x 8 ownership grid
LOD1 foliage tiles, fine 64 x 64 ownership grid
LOD1 wood tiles, fine 64 x 64 ownership grid
LOD0 foliage tiles, fine 64 x 64 ownership grid
LOD0 wood tiles, fine 64 x 64 ownership grid
```

Those resolutions align with the current terrain streamer. LOD2 needs a coarse
full-island representation, while LOD1 and LOD0 transitions are controlled at
the streamer's existing cells.

Copy native buffers on the background preparation path and always release the
native grid in `finally`. Validate:

- grid length;
- non-null handles for owned grids;
- finite vertices and normals;
- normal count equal to vertex count;
- triangle count divisible by three;
- every triangle index in range; and
- Unity `UInt32` index format when required.

Profile the memory cost of retaining all five prepared batches. If eager LOD0
copying is too expensive, change only the preparation strategy—such as cached
background tile preparation—not the placement or LOD contract.

## Unity streaming design

Add a focused `ForestTileStreamer` helper rather than embedding all forest
state into the already large `TerrainTileStreamer`.

It should own:

- the forest root GameObject;
- separate foliage and wood child roots;
- prepared tile arrays;
- active LOD2, LOD1, and LOD0 tile groups;
- shared wood and foliage materials; and
- created Unity meshes and their destruction.

`TerrainTileStreamer` should notify it at the same transition points used for
terrain:

### Initial LOD2 state

- Create the complete coarse LOD2 foliage representation.
- Do not create or render LOD2 wood.

### Entering an LOD1 region

1. Create/activate low-poly LOD1 foliage and wood for the incoming region.
2. Disable the corresponding LOD2 foliage owner tile.

### Leaving an LOD1 region

1. Re-enable the corresponding LOD2 foliage tile.
2. Destroy or deactivate the outgoing LOD1 wood and foliage group according to
   the existing terrain lifetime policy.

### Entering an LOD0 cell

1. Create/activate full LOD0 foliage and wood for the incoming owner cell.
2. Disable the corresponding LOD1 foliage and wood tile.

### Leaving an LOD0 cell

1. Re-enable the corresponding LOD1 foliage and wood tile.
2. Destroy the outgoing LOD0 meshes.

Always make the incoming representation ready before hiding the outgoing one,
so camera movement cannot produce a one-frame forest hole. At steady state,
one and only one visual LOD may own each tree.

Create at most one wood renderer and one foliage renderer per non-empty active
forest tile. Never create tree-level renderers.

## Materials and visibility

Reuse the existing tree wood and foliage materials/shaders in the worktree.
Create per-island runtime material instances only if live island settings must
modify them; otherwise use shared material templates.

Add forest visibility alongside the existing river, grass, and rock settings.
Toggling forest visibility should enable/disable forest roots without forcing
terrain regeneration or destroying prepared data.

Initial shadow policy should remain the same across LOD0 and LOD1 until it is
profiled. LOD2 foliage may later receive a cheaper shadow policy, but that is a
performance tuning task, not part of the required visibility matrix.

## Rust tests

### Placement completeness and exclusion

- Every accepted placement stores a valid, unique LOD0 terrain vertex index.
- Every accepted anchor is either the indexed vertex or lies no more than 50%
  of the way toward one of that vertex's incident triangle centroids.
- Every vertex satisfying noise and all habitat predicates is accepted exactly
  once unless the actual final-anchor exclusion zone rejects it.
- No other vertex is accepted.
- Noise equal to the threshold is rejected; noise greater than it is accepted.
- Coverage at the threshold maps to minimum scale and coverage `1.0` maps to
  maximum scale.
- A sea-level and a below-sea vertex are rejected.
- A vertex exactly below snowline can be accepted; one at snowline is rejected.
- A 22-degree vertex can be accepted; a vertex just steeper is rejected.
- A river-bed vertex is rejected.
- An unmarked bank vertex remains eligible even when the trunk foot, branches,
  or foliage extend across the river boundary.
- Changing trunk radius, branch reach, or foliage/crown size does not change
  river eligibility.
- A vertex whose final deposited soil depth is zero is rejected.
- A positive-soil vertex is not rejected merely because its tree geometry
  intersects a stone or boulder.
- Soil cleared by settled-rock generation is visible to tree placement.
- A low coastal anchor classified as beach by the terrain shader is rejected;
  removing sea proximity or moving above the four-metre beach fade restores
  eligibility.
- Beach material inputs are interpolated to the displaced anchor rather than
  sampled only at its source vertex.
- No accepted final anchor lies inside either tree's `3 metres * scale`
  clearance in XY, regardless of source-vertex topology.
- Scale is deterministic, remains within the configured range, and combines
  coarse forest coverage with its next finer octave.
- Nearby prototype choices share one coherent growth habit while retaining
  different geometry, yaw, and scale.
- Of two competing candidates, the higher-noise vertex wins; equal coverage is
  resolved by lower terrain vertex index.
- Physically close non-adjacent source vertices are covered by the same
  exclusion zone.
- Diagnostic counts are exclusive and sum to the total LOD0 vertex count.

### Determinism

- Same island seed, terrain, masks, rocks, and forest options produce identical
  placements and meshes.
- Representative different seeds produce different coherent forest regions.
- Changing only the threshold remaps scale while retaining stable yaw,
  prototype choice, and displaced anchor for trees present in both results.
- Placement order is ascending terrain vertex index.

### Combined geometry

- Combined vertex and triangle counts equal the sum of appended prototype
  ranges.
- All positions and normals are finite.
- Every triangle index is in range.
- Wood ranges contain only wood geometry.
- Foliage ranges contain only foliage geometry.
- Uniform scale/yaw/translation produce the expected transformed root, and
  every open base vertex matches the sampled final terrain height.
- Every tree range is copied whole into exactly one owner tile.
- No tree is geometrically clipped at a tile boundary.

### LOD contract

- Visual LOD2 returns low foliage and no wood.
- Visual LOD1 returns low foliage and low wood.
- Visual LOD0 returns full foliage and full wood.
- LOD2 and LOD1 foliage are derived from identical low-poly source topology.

### Options and persistence

- Invalid forest options return clear errors.
- Old save versions load documented forest defaults.
- The new save version round-trips all forest settings and reproduces the same
  placements.

## FFI tests

- Null and invalid inputs do not allocate or crash.
- Every valid call returns the requested grid length.
- Empty tiles are represented safely.
- LOD2 wood is empty but releasable.
- Wood and foliage grid handles are independent.
- Releasing a grid resets it to default and does not affect another grid.
- `ffi_allocations_have_matching_release_functions` explicitly covers the new
  exports and passes when run rather than remaining assumed from an ignored
  test.

## Unity validation

### Automated/editor checks

- Native and managed option layouts match.
- Prepared forest arrays have the required 8x8 or 64x64 lengths.
- Invalid mesh data is rejected before creating a Unity Mesh.
- Generated meshes use a suitable index format.
- Forest meshes are disposed on regeneration, cancellation, disable, and
  destruction.
- Existing river-rock and terrain streaming tests continue to pass.

### Live visual QA

Generate several representative seeds and inspect:

- a full-island view at LOD2: low foliage visible, no wood visible;
- an LOD1 region: low foliage and low wood visible;
- an LOD0 region: full foliage and full wood visible;
- movement across LOD2/LOD1 and LOD1/LOD0 boundaries;
- no doubled trees, missing trees, partial trees, or one-frame holes;
- top-down forest coherence rather than independent random scatter;
- no roots in the sea;
- no tree rooted on a vertex marked as river bed;
- tree geometry is allowed to overhang the river from an unmarked vertex;
- no tree rooted on a zero-soil vertex;
- no tree rooted on shader-classified beach;
- no additional rejection solely because tree geometry intersects a settled
  stone or boulder;
- no roots on slopes steeper than 22 degrees; and
- no roots at or above the configured snowline.

Use a debug view or temporary gizmos that colour candidates by rejection reason
if visual failures are difficult to attribute. Remove or gate expensive debug
geometry before finalizing.

## Performance and failure policy

The requirement to place a full tree on every selected LOD0 vertex can create
very large meshes because LOD0 is irregular and locally refined. Instrument
before tuning:

- total LOD0 vertex count;
- accepted tree count;
- prototype counts;
- vertices/triangles in each combined mesh;
- native generation time;
- native forest memory;
- managed prepared memory;
- Unity mesh-upload time; and
- frame time while crossing LOD boundaries.

If the result is too expensive, permitted tuning controls are:

- raise the coherent-noise threshold;
- increase the coherent patch size;
- lower prototype topology cost;
- reduce the prototype count if generation cost is the issue; or
- optimize tile preparation and lifetime.

Do not silently drop additional eligible vertices or introduce a second random
thinning pass. The scale-aware final-anchor clearance is the only spacing pass;
any broader spacing rule must be an explicit requirement change.

Checked capacity or index overflow must return a descriptive error containing
the accepted tree count and projected mesh size. It must never wrap, panic at
an opaque conversion, or leave a partially published FFI result.

## Implementation phases

### Phase 0: protect and validate the tree baseline

- Reinspect the dirty worktree and identify all current tree-related changes.
- Run focused procedural-tree tests.
- Validate all four standalone tree meshes and their FFI release paths.
- Compile and visually inspect the `TreeSandbox` LOD0/LOD1 result.
- Resolve baseline failures before forest integration so placement work is not
  used to hide tree-generator defects.

### Phase 1: forest options and placement-only implementation

- Add forest options, validation, and Unity/native conversion.
- Add `src/forest.rs` placement types and coherent field.
- Refactor decoration ordering so settled-rock soil clearing happens before
  tree placement.
- Generate final `TreePlacement` records from high-noise vertices after the
  sea, snowline, slope, river-marker, and zero-soil exclusions; then displace
  anchors toward deterministic fan centroids, reject shader beach at the final
  anchor, and apply the scale-aware exclusion zone. Do not assemble island
  forest meshes yet.
- Add completeness, exclusion, determinism, soil-ordering, and threshold-scale
  tests.
- Add diagnostic counts and inspect several generated seeds.

### Phase 2: prototype library and combined mesh assembly

- Make the tree generator local-origin-safe.
- Generate the deterministic prototype library once.
- Add stable placement appearance hashing.
- Add the mesh-appender and four combined streams.
- Record per-tree source ranges.
- Add geometry, transform, capacity, and deterministic-output tests.

### Phase 3: owner-based forest tiling and FFI

- Build whole-tree owner-tile extraction from recorded ranges.
- Add wood and foliage grid exports and header declarations.
- Update Unity native bindings.
- Add grid shape, LOD mapping, invalid input, and release tests.
- Explicitly run the full FFI allocation lifecycle test.

### Phase 4: Unity preparation and materials

- Add prepared forest data and background-copy helpers.
- Wire tree material templates and shared base snowline.
- Validate mesh contents before Unity object creation.
- Measure preparation memory and time before continuing.

### Phase 5: three-level forest streaming

- Add `ForestTileStreamer`.
- Create full-island LOD2 foliage.
- Integrate LOD1 low wood/foliage activation.
- Integrate LOD0 full wood/foliage activation.
- Implement reverse transitions and cleanup.
- Add forest visibility without regeneration.

### Phase 6: complete validation and tuning

- Run formatting, focused tests, full Rust tests, and strict Clippy.
- Run FFI lifecycle tests.
- Build the release native library.
- Deploy it to Unity and verify source/deployed checksums.
- Restart Unity so it reloads the native library.
- Compile Unity and run live multi-seed LOD traversal QA.
- Tune threshold/patch size using counts and profiling without changing the
  placement contract.

## Expected file areas

Rust:

- `src/forest.rs` — new placement, prototype batching, statistics, and tiling.
- `src/trees.rs` — local-origin reusable tree generation seam.
- `src/terrain/decorations.rs` — staged decoration/rock ordering and rock
  habitat reuse.
- `src/terrain/generation.rs` — final-mask inputs and `Island` forest storage.
- `src/terrain.rs` — forest options/defaults/validation if placed in
  `IslandOptions`.
- `src/ffi.rs` — forest grid exports and ABI tests.
- `include/motu.h` — matching C declarations.
- `README.md` — placement and LOD contract.

Unity:

- `Assets/Scripts/IslandSettings.cs` — forest settings and visibility.
- `Assets/Scripts/MotuNative.cs` — ABI declarations.
- `Assets/Scripts/IslandPreparedData.cs` — prepared forest container.
- `Assets/Scripts/IslandGenerator.cs` — background preparation, materials,
  snowline synchronization, and lifecycle.
- `Assets/Scripts/ForestTileStreamer.cs` — new focused forest streamer.
- `Assets/Scripts/TerrainTileStreamer.cs` — transition notifications only.
- existing `TreeWood`/`TreeFoliage` materials and shaders — reused, not
  recreated unless validation finds a concrete defect.

## Completion criteria

This forest phase is complete only when all of the following are demonstrated:

- The accepted placement set is every eligible high-noise final LOD0 vertex
  except candidates removed by the shader beach rule or deterministic,
  scale-aware final-anchor exclusion zone, with no count cap or other thinning.
- All habitat exclusions are enforced: sea, a marked river vertex, zero soil,
  slopes over 22 degrees, snowline, and shader-classified beach; deterministic
  fan displacement and the final-anchor exclusion zone are then applied to the
  remaining high-noise candidates.
- River exclusion considers only the root vertex's river marker; tree geometry
  may overhang the water from an unmarked vertex.
- Rock handling considers only final zero soil and does not perform tree/rock
  geometry intersection tests.
- Placement and appearance are deterministic.
- Rust owns combined wood and foliage geometry; Unity does not instantiate
  individual trees.
- Whole trees are assigned to spatial owner tiles without geometric clipping.
- LOD2 shows only low foliage.
- LOD1 shows low foliage and low wood.
- LOD0 shows full foliage and full wood.
- LOD transitions have no missing, doubled, or mixed-detail partial trees.
- FFI ownership and cleanup tests pass.
- Rust format, tests, strict Clippy, and release build pass.
- The deployed native library checksum matches the release build.
- Unity recompiles after deployment and live visual QA passes across multiple
  seeds and LOD boundaries.

## Resume checklist after context compaction

When resuming this work later:

1. Read this entire document.
2. Read `PROCEDURAL_TREE_AND_FOREST_PLAN.md` for the tree-shape baseline, but use
   this document as authority for forest placement and island LOD behavior.
3. Inspect current Git status and diffs; preserve unrelated and unfinished user
   work.
4. Determine which implementation phase is actually complete from code and
   current test evidence rather than assuming progress from this plan.
5. Re-run the narrow validation relevant to the next phase.
6. Continue from the first incomplete completion criterion.
