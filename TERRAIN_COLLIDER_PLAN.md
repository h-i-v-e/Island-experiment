# LOD 1 Terrain Collider Replacement Plan

## Implementation Status (2026-08-17)

The core replacement is implemented:

- Rust exports a deterministic, row-parallel global collider heightfield with
  validated 33, 65, and 129-sample tile contracts and paired ownership release.
- Unity prepares the production 8193x8193 lattice off the main thread, releases
  native ownership immediately after copying, and retains one normalized
  managed source array.
- First-person collision uses a hidden 3x3 set of 129x129 `TerrainCollider`
  tiles keyed to LOD 1 cells. Incoming tiles are installed before outgoing
  tiles are disabled and destroyed.
- First-person entry is gated on successful collider creation and raycast snap;
  LOD 0 rendering and grass streaming no longer trigger collider work.
- The legacy moving `MeshCollider`, bake/support fallback, and overlay toggle
  are removed from the Unity runtime.
- Unity's batch validator covers the exact production lattice, finite samples,
  bit-identical shared edges, a real hidden-terrain raycast, a 3x3 LOD 1
  transition, retained overlapping tiles, destination snapping, and cleanup.

Still requiring interactive acceptance on representative generated terrain:
the full walk/run/jump traversal matrix, percentile geometry-error capture, and
Unity Profiler comparison of the old and new collider paths. Those measurements
cannot be established by the batch interop validator alone.

## Objective

Replace the moving LOD 0 `MeshCollider` with heightfield-backed Unity
`TerrainCollider` objects, one logical collider tile for each 64x64 LOD 1
terrain square. Keep a hidden 3x3 neighbourhood active around the player so
collision is already present before the player crosses a tile boundary.

The collision heightfields must follow the final authoritative LOD 0 terrain
as closely as Unity's single-valued terrain representation allows. The visible
free-form terrain meshes, terrain LOD streaming, rivers, materials, and grass
remain unchanged.

## Current State

- The world is 2,000 metres square.
- The visible hierarchy is 8x8 LOD 2 tiles, 64x64 LOD 1 tiles, and 512x512
  LOD 0 tiles.
- Each LOD 1 collision tile therefore covers 31.25 x 31.25 metres.
- `TerrainTileStreamer.MoveCollider` removes and cooks a `MeshCollider` every
  time the player enters a new 3.90625-metre LOD 0 cell.
- The render mesh is tried first. If Unity cannot cook it, Rust exports a
  separate support mesh and Unity creates a collider from that instead.
- Collider removal happens before replacement. A failed or delayed cook can
  temporarily leave no collision beneath the player.
- Rust already owns an indexed final LOD 0 terrain and can sample its surface
  at arbitrary normalized XY coordinates. `CreateHeightMap` already exposes a
  whole-island float height map, but the Unity viewer does not consume it.

## Representation Constraint

A Unity terrain collider is a heightfield. It can represent exactly one height
for each horizontal XZ coordinate. It cannot reproduce overhangs, caves,
vertical faces, or separate surfaces stacked at the same XY position.

The authoritative collision surface will therefore be the top surface returned
by the existing final LOD 0 terrain sampler. This is the same sampling rule used
by terrain queries and map generation. Visible free-form geometry remains the
source of truth for rendering; the heightfield is a deliberately collision-only
approximation.

## Chosen Design

### One seam-safe global sampling lattice

Generate one whole-island height lattice from the final LOD 0 surface and split
it into overlapping per-tile views in Unity.

Current production target, increased after the 65x65 field visibly floated
above coarse parts of the free-form surface:

- 64 LOD 1 tiles per world edge;
- 128 height intervals per tile;
- 129x129 samples per Unity `TerrainData`;
- `64 * 128 + 1 = 8193` samples per world edge; and
- approximately 0.244140625 metres between samples.

Every tile includes both boundary rows and columns. Adjacent tiles read their
shared edge from the same global sample indices, making numerical edge equality
an invariant rather than relying on two independent sampling operations.

The generation worker retains the global height array. It does not create Unity
objects. A tile's 129x129 `float[,]` is copied only when its hidden terrain is
materialized on the main thread.

The previous 65x65 setting corresponds to a 4097x4097 global lattice and
approximately 0.48828125-metre spacing. A 33x33 setting corresponds to a
2049x2049 global lattice and approximately 0.9765625-metre spacing. Retain
129x129 unless profiling shows its generation or memory cost is unacceptable.

### Shared vertical normalization

Scan the generated lattice once for finite minimum and maximum elevations in
world metres. Give every tile the same vertical origin and height range:

```text
tile origin Y = global minimum height - safety margin
TerrainData.size.y = global maximum - tile origin Y + safety margin
normalized height = (world height - tile origin Y) / TerrainData.size.y
```

Using one range for all tiles prevents per-tile quantization and placement
differences. The margin must be explicit and small; it is not a replacement for
valid finite samples.

### Hidden Unity terrain objects

Each loaded collision tile owns:

- one GameObject under a dedicated `Terrain Colliders` root;
- one `TerrainData` with no layers, details, trees, or material data;
- one disabled or non-drawing `Terrain` component so the object remains hidden;
  and
- one enabled `TerrainCollider` referencing the same `TerrainData`.

The tile origin is:

```text
x = -worldSize / 2 + tileX * worldSize / 64
y = shared vertical origin
z = -worldSize / 2 + tileY * worldSize / 64
```

`TerrainData.size.x` and `.size.z` are both `worldSize / 64`. Height arrays are
indexed as `[z, x]`; Rust's normalized `(u, v)` maps to Unity `(x, z)`.

The visible mesh remains separate. No `Terrain` renderer is allowed to add a
draw call, shadow, reflection, foliage, or material lookup.

### Collision streaming

Collision follows the existing LOD 1 cell coordinate system, not the LOD 0
render cell coordinate system.

- On first-person entry, synchronously materialize and enable the valid 3x3
  LOD 1 neighbourhood before enabling the `CharacterController`.
- When the player enters another LOD 1 cell, create/enable every incoming tile
  first, then disable and release outgoing tiles.
- Never remove the current or destination collider before its replacement is
  live.
- Keep only the nearby 3x3 set enabled in PhysX.
- Cache a bounded number of recently created tile objects or `TerrainData`
  instances only if profiling proves reuse is cheaper than reconstruction.
  Correctness must not depend on the cache.
- `TrySnapToCurrentCollider` raycasts the collider containing the requested XZ
  coordinate, with neighbouring tiles as a boundary fallback.
- Clearing first-person focus disables/destroys the hidden collision
  neighbourhood and releases its `TerrainData` objects.

This changes collider work from every 3.90625 metres to every 31.25 metres and
keeps overlapping coverage during transitions.

## Data and Ownership Flow

```text
Rust final LOD 0 Terrain + TriangleIndex
    -> sample global 8193x8193 lattice on generation worker
    -> FFI-owned float buffer plus dimensions
    -> copy to PreparedColliderHeightMap
    -> release native buffer immediately
    -> retain prepared global samples with PreparedIsland/TerrainTileStreamer
    -> extract 129x129 shared-edge tile on demand
    -> create hidden TerrainData + TerrainCollider on Unity main thread
    -> destroy Unity objects on eviction/regeneration/exit
```

There must be exactly one owner at each native and managed boundary. A failed or
cancelled generation must release the native height map. Regeneration must not
leave the previous island's `TerrainData`, `TerrainCollider`, or hidden terrain
objects alive.

## Implementation Phases

### Phase 1 - Establish measurable baselines

1. Add development-only counters and timings around the existing
   `MoveCollider`, `Physics.BakeMesh`, support export, and collider replacement.
2. Reproduce a representative first-person route across multiple LOD 0 and LOD
   1 boundaries at walk and run speeds.
3. Record:
   - collider replacements;
   - main-thread time spent cooking and swapping;
   - frames in which no collider covers the player XZ position;
   - `CharacterController` falls below the sampled ground by more than its skin
     width; and
   - managed/native allocations attributable to collision.
4. Preserve the route or an automated equivalent as a regression scenario.

Acceptance:

- The current problem is reproduced or the existing unsafe remove-before-add
  transition is demonstrated directly.
- Baseline measurements are saved with the implementation notes so the final
  performance claim is comparative.

### Phase 2 - Formalize the Rust heightfield export

1. Keep the final LOD 0 `Terrain::sample`/triangle index as the only source of
   collision heights.
2. Give the existing height-map API explicit collider semantics, or add a
   dedicated `CreateTerrainColliderHeightMap` export if changing the existing
   API would make ownership or dimensions ambiguous.
3. Export:
   - width and height;
   - contiguous row-major `f32` samples;
   - enough information to validate the world-space elevation convention; and
   - an opaque owner released by a matching function.
4. Require a `64 * intervalsPerTile + 1` dimension, currently 8193.
5. Reject invalid dimensions and return a default/null export without leaking.
6. Parallelize row sampling only if profiling shows this stage is material;
   preserve deterministic output ordering.

Rust tests:

- dimensions and buffer length are correct for 2049, 4097, and 8193;
- first/last samples cover normalized 0 and 1 exactly;
- all samples are finite;
- selected samples equal direct final LOD 0 surface queries;
- shared tile-edge index calculations address the same source values;
- null handles and invalid dimensions return safe empty outputs; and
- release clears or safely disposes every allocation exactly once.

Acceptance:

- The exported field is a deterministic sampling of the authoritative final
  LOD 0 surface, not LOD 1 render geometry.
- No per-tile Rust sampling call or per-tile native allocation is required.

### Phase 3 - Prepare collision data off the Unity main thread

1. Add the native heightfield structure and release call to `MotuNative.cs`.
2. Add `PreparedColliderHeightMap` to `PreparedIsland` with dimensions, global
   samples, minimum height, maximum height, and vertical normalization values.
3. Copy and validate the native buffer inside `PrepareIsland`, alongside surface
   maps and river tiles, before transferring the island handle.
4. Release native ownership in `finally`, including cancellation and exception
   paths.
5. Pass prepared collision data into `TerrainTileStreamer.InitializeAsync`.
6. Keep all `TerrainData`, GameObject, `Terrain`, and `TerrainCollider` creation
   on Unity's main thread.

Acceptance:

- Generation cancellation and regeneration produce no native or Unity object
  leaks.
- The current island remains visible while the replacement island and its
  heightfield are prepared.
- No Unity API is called from the background worker.

### Phase 4 - Add the hidden LOD 1 terrain-collider neighbourhood

1. Replace `currentCollider`, `currentColliderMesh`, `MoveCollider`, and
   `RemoveCollider` with a dictionary keyed by global LOD 1 `Vector2Int` cells.
2. Add a small collision-tile type that owns the GameObject, `TerrainData`,
   hidden `Terrain`, and `TerrainCollider`.
3. Extract 129x129 tile samples using:

   ```text
   globalX = tileX * 128 + localX
   globalY = tileY * 128 + localY
   ```

4. Normalize heights with the shared world-space vertical range.
5. Set `heightmapResolution` before `size` and height assignment; verify the
   actual resolution Unity accepted.
6. Create/enable incoming tiles before removing outgoing tiles.
7. Update collision only when `WorldCell(position, Lod1Resolution)` changes.
8. Keep render LOD 0 streaming and grass updates on their existing schedules.
9. Update `TrySnapToCurrentCollider` to choose the tile under the point and try
   adjacent tiles for exact-edge coordinates.
10. Destroy the collider root and all owned `TerrainData` on first-person exit,
    regeneration, component disposal, and failed initialization.

Acceptance:

- No runtime path creates a terrain `MeshCollider`, calls `Physics.BakeMesh`, or
  requests `CreateSupportMesh` for player collision.
- At most nine terrain colliders are enabled in ordinary interior movement;
  fewer are allowed at world edges.
- The current player cell and all valid immediate neighbours have enabled
  collision before movement resumes.
- Hidden terrain contributes zero visible renderers/draw calls.

### Phase 5 - Geometry fidelity and seam validation

Build a deterministic validator that compares the Unity heightfield against the
Rust/free-form reference at sample points and between samples.

Measure separately:

- all terrain;
- walkable terrain at or below the controller's 55-degree slope limit;
- river beds and banks;
- coastlines;
- steep cliffs; and
- LOD 1 tile edges and corners.

Required checks:

- every pair of adjacent tiles has bit-identical normalized shared-edge input;
- Unity's interpolated world height agrees on both sides of every tile edge;
- no NaN, infinity, out-of-range normalized height, or transposed/flipped tile;
- sample points match the Rust height after world conversion within floating
  point tolerance; and
- between-sample error is reported as maximum, mean, and 95th/99th percentile.

Initial fidelity targets for walkable terrain:

- 95th percentile absolute vertical error <= 0.10 metres;
- 99th percentile absolute vertical error <= 0.25 metres; and
- maximum edge disagreement <= 0.001 metres.

Cliffs and other non-heightfield geometry must be reported separately rather
than weakening the walkable-surface target. Runtime observation showed the
65x65 field floating above coarse surface features, so production now uses the
validated 129x129 field.

Acceptance:

- The selected resolution is justified by measured collision error and memory/
  creation cost.
- Tile boundaries have no crack, step, or double-hit detectable by downward and
  shallow-angle raycasts.

### Phase 6 - Falling-through and transition regression

1. Add a play-mode or deterministic runtime harness that drives the controller:
   - across LOD 0 boundaries without changing collision tiles;
   - repeatedly across LOD 1 boundaries in both directions;
   - diagonally through four-tile corners;
   - downhill at run speed;
   - while jumping at a boundary; and
   - after a deliberate frame hitch or regeneration cancellation.
2. Track the expected ground from the prepared heightfield independently of
   collider raycasts.
3. Fail if:
   - no active collider covers the player;
   - the controller falls more than 0.25 metres below expected ground while
     inside world bounds;
   - a transition temporarily removes both old and new coverage; or
   - collider/object counts grow without bound on a repeated route.
4. Verify first-person entry snaps to the new terrain collider before the
   controller is enabled.

Acceptance:

- Zero falling-through events in repeated automated boundary traversals.
- Zero uncovered transition frames.
- Re-entering first-person and regenerating islands does not retain stale
  collision data.

### Phase 7 - Performance comparison and tuning

Profile a release/development player using the same route and hardware as the
baseline.

Compare 33x33, 65x65, and 129x129 tile data for:

- generation-worker duration;
- managed heightfield memory;
- native peak memory during transfer;
- main-thread time to create the initial neighbourhood;
- worst and average LOD 1 transition time;
- PhysX memory and simulation time;
- garbage collection allocations during steady movement; and
- collision fidelity metrics from Phase 5.

Targets:

- no collider cooking or managed allocation on LOD 0 boundary crossings;
- no uncovered collision frame on LOD 1 crossings;
- no recurring GC allocation during movement inside one LOD 1 tile;
- materially lower collision-update frequency and main-thread cost than the
  current moving mesh collider; and
- initial first-person entry remains responsive, with any multi-frame upload
  occurring before the controller is enabled and clearly represented in UI
  status.

Retain 129x129 unless profiling demonstrates an unacceptable cost and a lower
resolution can meet the fidelity targets without visible floating.

### Phase 8 - Remove the legacy path and document the limitation

1. Remove the `UseRenderCollider` toggle and overlay checkbox.
2. Remove runtime support-mesh collider export calls. Keep the native support
   mesh API only if another consumer or validation test still requires it.
3. Update status text and `island-unity/README.md` to describe the hidden LOD 1
   terrain-collider neighbourhood and its resolution.
4. Document the heightfield limitation for overhangs and stacked/vertical
   geometry.
5. Add collision-heightmap ownership and lifecycle checks to the editor/native
   validation routine.

Acceptance:

- Documentation matches actual collision resolution and streaming behaviour.
- There is one unambiguous player-collision implementation with no dormant mesh
  fallback toggle.

## Files Expected to Change

Rust:

- `island-rs/src/terrain.rs` - heightfield sampling contract and tests.
- `island-rs/src/ffi.rs` - owned export, validation, release, and ABI tests.
- `island-rs/include/motu.h` - C declaration if the export changes.
- `island-rs/README.md` - native heightfield/collision export documentation.

Unity:

- `island-unity/Assets/Scripts/MotuNative.cs` - native structure and functions.
- `island-unity/Assets/Scripts/IslandViewer.cs` - background preparation,
  ownership, status, and validation.
- `island-unity/Assets/Scripts/TerrainTileStreamer.cs` - hidden terrain
  creation, neighbourhood lifecycle, snapping, and disposal.
- `island-unity/Assets/Scripts/FirstPersonController.cs` - only if explicit
  readiness gating is needed before enabling movement.
- `island-unity/README.md` - runtime behaviour and limitations.
- Unity tests or validation code for height orientation, seams, lifecycle, and
  movement regressions.

No shader, visible terrain mesh, river mesh, or terrain generation parameter
change is required for this work.

## Verification Matrix

Rust checks:

```text
cargo fmt --all -- --check
focused heightfield and FFI ownership tests
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Run the full Rust suite as a separate signal and report known unrelated baseline
failures independently.

Unity checks:

- Unity 6 editor compile with no C# or shader errors;
- native ABI validation for dimensions, ownership, and finite samples;
- edit-mode tests for tile extraction, orientation, shared edges, and disposal;
- play-mode traversal tests from Phase 6;
- Physics Debug inspection showing only the nearby terrain-collider set; and
- Profiler captures for the baseline and final implementation.

Manual acceptance:

- Enter first-person on flat ground, slopes, river banks, coast, and near a
  cliff.
- Walk/run/jump across cardinal and diagonal LOD 1 boundaries.
- Revisit prior cells, exit/re-enter first-person, and regenerate several seeds.
- Confirm the visible free-form mesh is unchanged and no hidden terrain renders.
- Confirm the controller does not fall through and does not visibly step at tile
  seams.

## Definition of Done

The work is complete when:

1. Player collision uses only hidden Unity terrain colliders derived from the
   final LOD 0 surface.
2. Each logical LOD 1 square maps to a seam-safe heightfield tile.
3. The nearby LOD 1 collision neighbourhood is installed before movement and
   swapped add-before-remove.
4. Automated traversal records zero uncovered frames and zero falling-through
   events.
5. Fidelity and performance measurements justify the selected heightmap
   resolution.
6. Native and Unity ownership tests show no leaks across cancellation,
   regeneration, first-person exit, or disposal.
7. The legacy moving mesh-collider path and its UI toggle are removed.
8. Documentation states both the performance behaviour and the unavoidable
   heightfield limitation.

## Reference Constraints

- Unity 6 clamps `TerrainData.heightmapResolution` to supported values including
  33, 65, and 129, so per-tile resolutions use `2^n + 1` samples.
- `TerrainData.SetHeights` consumes normalized `[0, 1]` samples indexed as
  `[y, x]`.
- `TerrainCollider` collision is generated from the assigned `TerrainData`
  heightmap.

Official references:

- <https://docs.unity3d.com/6000.0/Documentation/ScriptReference/TerrainData-heightmapResolution.html>
- <https://docs.unity3d.com/6000.0/Documentation/ScriptReference/TerrainData.html>
- <https://docs.unity3d.com/6000.0/Documentation/Manual/terrain-colliders-introduction.html>
