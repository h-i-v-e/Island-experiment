# Streamed Stone and Boulder Decoration Plan

## Status

Implemented through the visual streaming phases on 2026-08-17. The Unity side
now copies the native rock anchors during background preparation, derives
deterministic appearance, builds 20 shared procedural prototypes, and streams a
fixed pool of 128 renderers with the active LOD0 neighbourhood. The dedicated
rock shader shares the terrain's rock colour and coherent 3D geology noise.

Primitive boulder collision remains deliberately deferred until the visual
system has been inspected and profiled in play mode. Shader-driven transition
fades also remain an optional follow-up only if cell-boundary popping proves
visible.

The initial deployed native plugin produced only 261 rocks for seed 666 and 270
for seed 2018. Those historical counts justified the original 128-slot pool,
but no longer describe the dense feature-triangle source list below.

Play-mode review then showed that successive rule-based placement strategies
still looked artificial. Placement was therefore replaced on 2026-08-17 by a
deterministic baked settling simulation. The generator traverses every final,
post-carve LOD0 triangle and considers faces steeper than 45 degrees regardless
of their grass, soil, rock, river, or waterfall context. True 3D face area and
four-octave coherent noise at a 72x spatial scale determine each face's unbiased
share of the fixed body budget. Noise contrast is increased by 30 percent and a
15 percent density floor keeps low-noise faces eligible, spreading drops across
more, finer cliff regions. The simulated body budget is seven times the terrain
decoration target, increased from three times to aim for at least twice as many
settled anchors while preserving unbiased placement. Seeded
barycentric samples place bodies across the selected faces, 2-14 metres above
the surface. There is deliberately no river or waterfall weighting in this
first pass. Rocks reach channels and gentler ground by rolling from those faces.
Bodies integrate gravity at a fixed 60 Hz, collide with the exact LOD0 surface,
approximately collide with one another through a reusable spatial grid, lose
energy on contact, and sleep when motion remains below a fixed threshold.

Bodies may sleep only on support no steeper than 25 degrees, and the final pass
rejects any anchor whose underlying LOD0 normal exceeds that limit. After 360
bounded simulation steps, unsupported bodies are culled. Grass-settled rocks
remain; instead, all three vertices of each supporting terrain triangle have
their source loose-soil depth set to zero before the final shader material field
is built. The native anchor may be above the terrain for a physical pile. A
parallel appearance ID preserves the deterministic Unity size/prototype hash
when earlier bodies are culled. Trees and bushes retain their existing
generation path.

Release diagnostics after increasing the simulation budget retain 4,838 anchors
for seed 666 and 6,605 for seed 2018, respectively 2.34 and 2.37 times the prior
populations. Whole-island generation, including eager settling and soil
clearing, takes approximately 25.78 and 22.45 seconds in the local diagnostic
environment. The runtime continues to render only the nearest 128 candidates;
the baked source list must not create one Unity object per simulated body. The
3x3-neighbourhood diagnostic must still be refreshed in play mode before
changing that fixed pool size.

The Rust generator already produces one deterministic `rocks` position list and
exports it through `GetDecoration`. The first implementation will consume that
existing list in Unity, classify each entry visually as either a stone or a
boulder, and stream a fixed pool of reusable renderers with the active LOD0
terrain. A subsequent placement refinement changes where the native rock list
is generated but does not add trees or bushes or create one Unity object per
generated rock.

## Objective

Decorate the detailed terrain around the first-person player with varied stones
and boulders while preserving the current streaming and performance model:

- use the rock positions already produced by Rust;
- generate a small deterministic library of irregular sphere-derived meshes;
- render those meshes with the same stone colour and procedural surface
  character as exposed terrain rock;
- reuse a fixed Unity object pool as the player moves;
- show rocks only where the corresponding 3x3 LOD0 neighbourhood is active;
- keep the same rock visually stable when it leaves and later re-enters the
  pool; and
- allocate no managed memory and make no native calls during steady player
  movement.

In this document, "rotate into and out of the pool" means reassigning fixed pool
slots to nearby rock records. Rocks must not visibly spin as part of streaming.

## Current Architecture

The relevant existing path is:

1. `Decorations::generate` in `island-rs/src/terrain.rs` generates deterministic
   tree, bush, and rock positions while the island still owns mutable source
   soil depths.
   Rock points come from the bounded deterministic drop-and-settle simulation
   described above. The simulation owns transient bodies and a reusable spatial
   grid; only accepted settled anchors remain on the island. Grass-settled
   supporting triangles are returned as compact vertex IDs and applied once to
   the source soil field before the final material field is created.
2. `GetDecoration` in `island-rs/src/ffi.rs` exposes borrowed arrays of tree,
   bush, and rock positions plus parallel rock appearance IDs. The arrays remain
   owned by the island and are valid until `ReleaseMotu`; there is no separate
   decoration-release call.
3. `IslandViewer.PrepareIsland` already performs native generation and
   native-to-managed copies on a background task.
4. `TerrainTileStreamer` keeps a 3x3 neighbourhood of LOD0 parent cells around
   the player's current 64x64 LOD1 cell. Each parent cell contains an 8x8 batch
   of LOD0 render tiles.
5. `RiverParticlePool` and `RiverEmitterIndex` demonstrate the desired pattern:
   immutable prepared candidates, a packed 64x64 spatial index, fixed runtime
   slots, deterministic nearest selection, lifecycle cleanup, and debug gizmos.
6. `Motu/Terrain Unified` currently hard-codes its exposed-rock colour and uses
   the shared `_CliffNoise3D` texture for coherent stone-scale colour boundaries
   and normal perturbation.

The stone system should reuse these ownership and streaming patterns without
coupling itself to particle-system behaviour.

## Scope

### Included

- The existing Rust `rocks` positions.
- Two visual size classes: stones and boulders.
- Deterministic procedural prototype meshes generated from icospheres.
- A dedicated rock material sharing the terrain rock palette and 3D noise.
- A packed Unity spatial index aligned to the 64x64 LOD1-cell grid.
- A fixed pool of reusable `MeshFilter`/`MeshRenderer` objects.
- LOD0-neighbourhood activation, stable reassignment, cleanup, diagnostics, and
  validation.
- Optional primitive collision for sufficiently large active boulders after the
  visual system is proven.

### Excluded from the first implementation

- Trees, bushes, grass placement, or a general vegetation system.
- New decoration categories or unrestricted island-wide rock scatter.
- Separate native categories for stones and boulders.
- Per-rock unique meshes, textures, or materials.
- Runtime mesh deformation.
- `MeshCollider` generation for decorative rocks.
- Saving streamed pool state. Appearance is derived deterministically instead.
- Rendering rocks in overview, LOD2-only, or LOD1-only terrain regions.

## Non-Negotiable Behaviour

- A generated island's source rock positions remain deterministic for its seed
  and terrain options.
- Each source rock index always maps to the same class, prototype, scale,
  rotation, tint, and embed depth for a given island seed.
- All native positions are copied before background preparation returns. Player
  movement never calls `GetDecoration`.
- Prototype meshes and pool objects are created once, then reused.
- No `Instantiate`, `Destroy`, mesh generation, material creation, LINQ, or
  managed collection growth occurs during steady movement.
- A rock is eligible only while its 64x64 cell belongs to the same active 3x3
  neighbourhood used by `UpdateLod0Neighborhood`.
- Incoming LOD0 terrain is ready before newly eligible rocks are enabled.
- Overview return, generation cancellation, regeneration, and viewer
  destruction disable or release every managed and native owner correctly.
- The Rust-to-Unity coordinate conversion is applied exactly once.
- Shared meshes are never mutated after prototype construction.
- Stones do not receive colliders. If boulder collision is enabled later, it
  uses pooled primitive colliders rather than runtime mesh cooking.

## Key Design Decisions

### Export settled anchors with stable appearance IDs

Add C# layouts for the already exported native data:

```csharp
[StructLayout(LayoutKind.Sequential)]
internal struct ExportDecoration
{
    internal Vector3Array trees;
    internal Vector3Array bushes;
    internal Vector3Array rocks;
    internal UInt32Array rockAppearanceIds;
}

[DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
internal static extern void GetDecoration(
    IntPtr handle,
    out ExportDecoration output);
```

`GetDecoration` returns borrowed pointers, not an owned export handle. Unity
must copy `output.rocks` and `output.rockAppearanceIds` while the island handle
is alive and must not free any borrowed pointer. The two rock arrays must have
identical lengths. No new native allocation/release pair is required.

### Treat each rock position as a stable anchor

The Rust API has one `rocks` list, not separate stone and boulder lists. Unity
will derive appearance from a stable 64-bit hash of:

```text
island seed + exported appearance ID + a rock-decoration domain constant
```

Do not use `UnityEngine.Random`, global random state, array order after spatial
packing, or pool-slot index. A candidate must look identical after pool
reassignment and across repeated generation of the same seed.

Initial classification and size ranges are tuning defaults, not new terrain
generation settings:

| Property | Stone | Boulder |
| --- | ---: | ---: |
| Initial share | 85% | 15% |
| Longest diameter | 0.10-0.30 m | 0.30-0.60 m |
| Relative vertical scale | 0.35-0.75 | 0.70-1.20 |
| Surface embed | 10-50% of height by slope | 10-50% of height by slope |
| Collision | Never | Deferred, primitive only |

Keep class thresholds and size ranges as named Unity constants until visual and
profiling passes establish useful ranges. They do not belong in `MotuOptions`
because they do not change terrain or source placement.

Both classes use the same deterministic slope curve. Flat ground embeds 10
percent of rock height, increasing linearly to 50 percent at the native 25-degree
maximum settling slope. Values are clamped, so normal-map quantization cannot
push the visual embed beyond that range.

### Copy and enrich candidates during background preparation

Add an immutable managed `PreparedRockDecoration` containing:

```text
sourceIndex
worldPosition
worldNormal
class
prototypeIndex
scale
rotation
tint
embedDepth
```

Copy the native position using the existing mapping:

```text
Rust (x, y, z) -> Unity ((x - 0.5) * 2000, z * 2000, (y - 0.5) * 2000)
```

The existing 2048x2048 LOD0 surface-normal map is already copied during the
same background preparation task. Bilinearly sample that managed RGB array at
the original normalized rock XY coordinate, decode from `[0, 255]` to
`[-1, 1]`, and apply the existing axis swap:

```text
Rust normal (x, y, z) -> normalize(Unity(x, z, y))
```

Fall back to `Vector3.up` only for a zero-length or non-finite decoded normal.
This avoids a new native query, avoids physics raycasts, and gives each rock a
surface orientation coherent with the LOD0 shading map.

The source position remains authoritative for contact with the final free-form
mesh. Do not replace its Y coordinate with the hidden TerrainCollider's sampled
height, because that collider is an approximation and can visibly float above
or cut below the LOD0 render mesh.

### Generate a small shared prototype library

Add a `RockPrototypeLibrary` responsible only for immutable mesh construction
and disposal. Build prototypes once on the Unity main thread before pool slots
are assigned.

Use a base icosahedron rather than Unity's latitude/longitude sphere so
deformation has uniform vertex spacing and no pinched poles. Keep it
unsubdivided: 12 vertices and 20 triangles per prototype. At the revised
10-60 cm scale this intentionally faceted geometry is sufficient, and the
directional deformation keeps the silhouettes from looking identical.

Initial library:

- 12 stone prototypes;
- 8 boulder prototypes; and
- one fixed prototype seed independent of the island seed, allowing the same
  mesh library to be reused for every regenerated island.

For each prototype:

1. Build and normalize the 12-vertex icosahedron.
2. Choose deterministic ellipsoid axis scales within the class range.
3. Apply two or three coherent, low-frequency directional-noise octaves to the
   radial distance.
4. Clamp radial displacement to a conservative range, initially `0.72-1.28`,
   so no vertex crosses the centre or creates a spike.
5. Apply a low-order directional bias so shapes are not all evenly noisy
   spheres.
6. Weld shared vertices by construction, recalculate normals and bounds, and
   validate finite vertices, normals, triangles, and non-zero volume.
7. Keep the pivot at the undeformed centre. Ground support is calculated when
   assigning a pooled instance.

Do not add independent random displacement per vertex. That produces crystalline
spikes rather than weathered stones and causes neighbouring triangle normals to
flicker.

### Seat rotated rocks against the terrain surface

For each candidate, align prototype local up to the sampled terrain normal,
then apply deterministic yaw and a small deterministic tilt. Initial maximum
extra tilt is 12 degrees for stones and 8 degrees for boulders.

After choosing prototype, rotation, and scale, scan that prototype's vertices
once during slot assignment and find the minimum projection onto the world
surface normal:

```text
support = min(dot(rotation * scaledVertex, surfaceNormal))
position = sourcePosition - surfaceNormal * (support + embedDepth)
```

This places the lowest supporting part of the irregular mesh against the local
tangent plane and then embeds it slightly to hide tiny terrain curvature or
normal-map errors. The scan is bounded by 12 vertices and happens only when a
slot is reassigned, not every frame.

Reject non-finite transforms. A bad candidate is skipped and recorded by the
debug counter rather than placing a rock at the world origin.

### Share the terrain's rock appearance

The exposed-rock colour in `TerrainDetail.shader` is currently a hard-coded
`(0.34, 0.32, 0.29)`. Promote this to a `_RockColor` material property and set
it from one `IslandViewer` constant. The terrain and decoration materials must
receive that same value.

Add `Motu/Rock Decoration`, an opaque lit shader that:

- uses `_RockColor` as its base;
- samples the existing shared `_CliffNoise3D` texture in world space;
- uses the same `_CliffNoisePeriod`, `_CliffNoiseDetailScale`, and
  `_CliffNormalStrength` contract as terrain rock;
- applies modest broad colour variation rather than re-running terrain surface
  classification;
- accepts a small per-instance `_RockTint` from a `MaterialPropertyBlock`;
- supports the current forward light, ambient light, shadows, fog, and depth;
  and
- supports GPU instancing.

The standalone rocks are always rock material. They must not sample sand,
grass, snow, river, or geology coverage masks from the terrain shader. Sharing
the rock palette and procedural noise makes them belong to the same geology
without treating them as terrain fragments.

Create one rock material and share it across every pool slot. Do not clone a
material per instance. Small stable tint differences should remain within about
eight percent of the common palette so pooled rocks do not look like unrelated
assets.

### Use a packed LOD1-cell index

Add `RockDecorationIndex`, following the allocation and layout pattern of
`RiverEmitterIndex`:

- immutable `PreparedRockDecoration[] candidates`;
- `int[4097] cellOffsets` for the 64x64 LOD1-cell grid; and
- `int[] candidateOrder` grouping candidate indices by cell.

Build it once during pool initialization. Cell coordinates come from world XZ
using the same clamp and resolution as `TerrainTileStreamer.WorldCell`.

Do not refactor the working river index into a generic abstraction during the
first rock implementation. The two records and query rules differ, and a
premature generic index would broaden the risk. A later cleanup may extract a
shared cell-coordinate helper after both systems are tested.

### Use a fixed renderer pool tied exactly to active LOD0 cells

Add `RockDecorationPool` under the terrain root. Begin with 128 fixed slots.
The diagnostic phase must confirm whether this covers representative high-rock
seeds; if it does not, adjust the fixed constant before visual tuning rather
than silently dropping nearby rocks.

Each slot owns:

- one inactive child `GameObject`;
- one `MeshFilter` referencing a shared prototype;
- one `MeshRenderer` referencing the shared material;
- one reusable `MaterialPropertyBlock` or a pool-level reusable block applied
  at assignment; and
- an optional disabled primitive collider reserved for the later collision
  phase.

Pool updates occur after `UpdateLod0Neighborhood` completes. Eligibility is
cell-based, not merely distance-based:

```text
abs(candidateCell.x - currentLod1.x) <= 1
and
abs(candidateCell.y - currentLod1.y) <= 1
```

At each LOD1-cell transition:

1. Retain assignments whose source candidates remain in the new 3x3 cell set.
2. Disable slots whose candidates left the set.
3. Visit only the nine indexed cells in deterministic row-major order.
4. Fill free slots with unassigned candidates, ordered by distance to the
   player and then source index for stable ties.
5. If eligible candidates exceed pool capacity, retain the nearest 128 and
   expose the dropped count in diagnostics.
6. Assign mesh, transform, tint, shadows, and optional collider before enabling
   the slot.

The pool does not need a per-frame update. It is refreshed when the player
crosses a 64x64 LOD1-cell boundary, when entering first-person mode, when the
visibility toggle changes, or when a new island is installed. Pool assignment
arrays and query scratch are allocated once.

The first version may hard-enable and hard-disable slots with the LOD0 cell
transition. If popping is objectionable in runtime review, add a short
shader-driven dither fade using a fixed per-slot state array. Do not animate
scale from zero, because growing boulders make ground contact visibly slide.

### Keep lifecycle ownership explicit

Extend `PreparedIsland` with the managed rock candidate array. The native
decoration pointer is never stored there.

`TerrainTileStreamer.InitializeAsync` receives the prepared rocks, creates the
prototype library and pool on the main thread, and initializes the packed
index. `SetPlayerPosition` updates the rock pool only after LOD0 terrain update.

Lifecycle behaviour:

- `ClearPlayerFocus`: disable every slot and clear assignments;
- rock visibility off: disable every slot without destroying the pool;
- rock visibility on: force one assignment refresh at the current player
  position if first-person focus exists;
- regeneration: dispose the old pool, material references, and prototype meshes
  through `ClearGeneratedContent`;
- cancellation before upload: managed arrays become ordinary garbage and the
  island handle follows the existing `PreparedIsland.Dispose` path;
- `TerrainTileStreamer.Dispose`: clear slots, destroy pool objects and prototype
  meshes exactly once, and drop candidate/index references; and
- `IslandViewer.OnDestroy`: destroy the shared rock material without touching
  terrain-owned noise textures until both users have stopped rendering.

## Initial Runtime Budgets

These are acceptance targets to validate, not claims about current performance:

| Resource | Initial budget |
| --- | ---: |
| Prototype count | 20 shared meshes |
| Prototype topology | 12 vertices / 20 triangles each |
| Renderer pool | 128 fixed slots |
| Active rendered rocks | At most 128 |
| Long-lived spatial index | 4,097 offsets plus one integer per rock |
| Native calls during movement | 0 |
| Steady-state managed allocation | 0 bytes |
| Cell-transition pool update | < 0.25 ms p95 |
| Main-thread prototype plus pool construction | < 10 ms target |
| Additional first-person draw calls | <= prototype groups actually visible |

Enable GPU instancing on the shared material. Grouping is naturally limited by
the prototype count; runtime profiling must confirm that Unity batches slots
sharing the same mesh and material. If it does not, replace individual renderer
submission with grouped `Graphics.DrawMeshInstanced` arrays while preserving
the same fixed candidate pool and optional separate collider pool.

## Implementation Phases

### Phase 0: Baseline diagnostics and visual scale

1. For representative seeds including 2018, record total tree, bush, and rock
   counts from `GetDecoration` without changing generation.
2. Record rock counts per 64x64 cell and the maximum, median, and 95th percentile
   totals across every possible 3x3 neighbourhood.
3. Confirm that source rock positions are finite, inside normalized XY bounds,
   above the sea threshold, and on the final LOD0 surface within a small
   epsilon.
4. Inspect several source positions on cliffs, high ground, river-adjacent
   ground, and cell boundaries.
5. Use temporary gizmos or wire spheres to establish sensible stone and boulder
   diameter ranges before generating final meshes.

Exit criteria:

- The source list is non-empty for representative normal seeds.
- The measured 3x3 maximum supports a justified fixed pool size; 128 remains
  the default only if the data supports it.
- Source positions match the final terrain closely enough that support offset
  and modest embedding can hide contact errors.
- The 75/25 class split and proposed size ranges look plausible at the current
  2 km world scale.

### Phase 1: Native binding and background preparation

1. Mirror `ExportDecoration` in `MotuNative.cs` and bind `GetDecoration`.
2. Add `PreparedRockDecoration` and a deterministic, self-contained hash helper.
3. Call `GetDecoration` in `PrepareIsland` after the LOD0 surface map is copied
   and while the native island handle remains valid.
4. Copy only the `rocks` array. Do not copy trees or bushes into long-lived
   managed arrays yet.
5. Decode bilinearly sampled surface normals, convert positions/normals, and
   derive stable appearance data on the generation worker.
6. Add candidates to `PreparedIsland` and pass them through initialization.
7. Extend native interop validation for empty and non-empty borrowed arrays,
   finite bounds, determinism, and pointer-lifetime assumptions.

Exit criteria:

- No decoration marshaling occurs during player movement or on the Unity main
  thread.
- Same seed produces byte-equivalent prepared appearance fields and ordering.
- No native decoration pointer outlives the island or is incorrectly released.
- Invalid length/pointer combinations fail preparation with a clear error.

### Phase 2: Procedural prototype mesh library

1. Add an icosphere builder with indexed, welded topology.
2. Add coherent seeded deformation and separate stone/boulder shape ranges.
3. Generate the initial 12 stone and 8 boulder prototypes once.
4. Recalculate normals and bounds and mark meshes non-readable only after all
   support data needed for placement has been copied into compact prototype
   records. If support scans use vertices at assignment, keep the small shared
   vertex arrays in those records rather than calling `mesh.vertices` later.
5. Add deterministic mesh tests using vertex/index hashes.
6. Add disposal that destroys each generated `Mesh` exactly once.

Exit criteria:

- Every mesh is manifold, finite, outward-facing, and has valid bounds.
- Same prototype seed produces identical vertex and index buffers.
- Shapes are visibly varied without spikes, inverted triangles, or pole seams.
- All instances share one of the fixed prototype meshes; no instance owns a
  unique mesh.

### Phase 3: Shared rock material

1. Add `_RockColor` to the unified terrain shader and replace its hard-coded
   rock constant without changing the default appearance.
2. Define the shared rock palette once in `IslandViewer` and apply it to terrain
   and decoration materials.
3. Add `Motu/Rock Decoration` with shared world-space noise, normal detail,
   lighting, shadows, fog, and instancing support.
4. Reuse the existing cliff noise texture and matching scale/strength settings.
5. Add stable per-candidate tint through a property block without creating
   material instances.
6. Extend shader validation to assert required properties and resource binding.

Exit criteria:

- Default terrain rendering is visually unchanged before any rock is enabled.
- A neutral decoration sphere matches nearby exposed terrain rock under the
  same light.
- All active rocks reference one material.
- Regeneration does not increase material or texture counts.

### Phase 4: Spatial index and fixed pool

1. Add `RockDecorationIndex` with packed 64x64 cells.
2. Add `RockDecorationPool` with the measured fixed slot count and preallocated
   selection scratch.
3. Implement deterministic retain, release, nearest-fill, and overflow logic.
4. Implement prototype assignment and tangent-plane support placement.
5. Create slots inactive and enable only after every renderer property is set.
6. Add candidate, active, rejected, and overflow counters.
7. Add debug gizmos for source anchors, sampled normals, transformed support
   points, active slots, and the current 3x3 eligible cell boundary.

Exit criteria:

- Pool object count never changes during walking.
- The active candidate set equals a brute-force 3x3-cell reference.
- Re-entering an area restores the exact prior rock appearance and transform.
- No rock appears at `(0, 0, 0)` because of invalid data.
- Every active mesh touches or modestly intersects its local terrain surface;
  none visibly floats.
- Pool overflow, if any, consistently retains the nearest candidates.

### Phase 5: Streamer and visibility integration

1. Create the pool under the generated terrain root during streamer
   initialization.
2. Update it after `UpdateLod0Neighborhood` in `SetPlayerPosition`.
3. Clear it from `ClearPlayerFocus` so overview mode contains no rocks.
4. Add a `Stones and boulders` viewer toggle that disables assignments without
   destroying prototypes or slots.
5. Dispose the pool from `TerrainTileStreamer.Dispose` and destroy the shared
   material from `IslandViewer` ownership.
6. Include total candidates, active slots, and pool capacity in the generation
   status/debug display.

Exit criteria:

- Rocks are present only in first-person mode and only over active LOD0 cells.
- LOD0 cell transitions reveal no one-frame rocks over missing terrain.
- Visibility toggles, overview return, regeneration, cancellation, and viewer
  destruction leave no orphan renderers, meshes, materials, or native owners.
- Terrain, collider, river, grass, and particle streaming behaviour remains
  unchanged.

### Phase 6: Optional pooled boulder collision

Do this only after visual placement and pool performance pass.

1. Enable a pooled `SphereCollider` or `CapsuleCollider` only for boulders above
   a chosen visual diameter, initially 1.25 m.
2. Fit the primitive conservatively to the deformed prototype's central mass;
   avoid covering thin protrusions.
3. Disable the collider before moving/reassigning a slot and enable it only
   after the new transform is complete.
4. Keep every stone collider disabled.
5. Verify that the first-person controller cannot become trapped between a
   boulder and the terrain during a cell transition.

Exit criteria:

- No runtime `MeshCollider` or `Physics.BakeMesh` call is introduced.
- Active collider count is bounded by the fixed pool and materially lower than
  the number of active decorative stones.
- Walking into a large boulder feels plausible; walking near small stones is
  unaffected.
- Reassignment cannot impart forces, teleport the player, or leave a collider
  at an old position.

### Phase 7: Profiling, tuning, and documentation

1. Test representative low-rock and high-rock seeds in first person.
2. Profile prototype construction, index construction, LOD1-cell transitions,
   rendering batches, shadows, and optional collision.
3. Record managed allocation during continuous movement and repeated boundary
   crossings.
4. Tune class ratio, prototype count, deformation, size, embed depth, pool size,
   tint, shadow distance, and optional collision threshold from evidence.
5. Decide whether hard cell transitions are acceptable. Add a bounded dither
   fade only if profiling and runtime review justify it.
6. Update both READMEs with native ownership, visual classification, streaming
   rules, tuning constants, debug controls, and validation commands.

Exit criteria:

- The initial runtime budgets are met or revised with recorded evidence.
- There is no steady-state GC allocation and no movement-time native call.
- GPU instancing/batching is demonstrated in the Unity profiler.
- Stones and boulders read as part of the same geology as exposed terrain rock.
- Cell transitions, overview return, and regeneration show no visible or owned
  leftovers.

## Validation Strategy

### Native and preparation validation

- `ExportDecoration` C/Rust/C# sizes and field order agree.
- A zero-length array is accepted without dereference.
- A positive length requires a readable non-null pointer.
- All copied rock positions are finite and plausibly bounded.
- Same seed produces identical source ordering and prepared appearance data.
- Borrowed pointers are copied before `ReleaseMotu` and never freed by Unity.
- Normal-map bilinear sampling handles corners and clamps exact `1.0`
  coordinates to the final texel.

### Prototype validation

- Expected icosphere vertex/triangle counts for each subdivision.
- Indices stay within bounds and every triangle has non-zero area.
- Vertex positions and normals are finite.
- Radial displacement stays within the configured clamp.
- Repeated prototype construction hashes identically.
- Stone and boulder bounds remain within their class constraints.

### Index and pool validation

- Candidate-to-cell mapping matches `TerrainTileStreamer.WorldCell` at world
  edges and cell boundaries.
- Packed cell ranges contain every candidate exactly once.
- Nine-cell queries match a brute-force reference.
- Distance and source-index ties resolve deterministically.
- Retained slots are not unnecessarily reassigned.
- Overflow selects the nearest fixed-capacity set.
- No assignment-time managed allocation occurs after initialization.

### Placement validation

- Rust-to-Unity positions match sampled terrain points.
- Decoded normals use the correct XZY axis conversion.
- Flat, steep, and boundary anchors seat against the terrain.
- Support projection remains correct under non-uniform scale and rotation.
- Embed depth hides tiny gaps without swallowing most of a stone.
- Candidate appearance is independent of pool-slot and packed-index order.

### Lifecycle and rendering validation

- Exactly the configured number of slots and prototypes exists after
  initialization.
- Overview mode, visibility off, cancellation, regeneration, and disposal leave
  zero active assignments.
- Materials and generated meshes do not accumulate across repeated generation.
- Existing terrain, grass, rivers, particle emitters, and terrain colliders pass
  their current batch validation.
- Unity reports shared materials and successful GPU instancing for compatible
  visible prototypes.

## Test Matrix

| Area | Required proof |
| --- | --- |
| Source positions | Existing Rust rocks copied exactly once and remain deterministic |
| ABI ownership | Borrowed decoration arrays are neither leaked nor freed by Unity |
| Coordinate mapping | Rock position and sampled normal match LOD0 world orientation |
| Classification | Same seed/index always yields the same stone or boulder |
| Prototype generation | Deterministic, finite, manifold shared meshes with bounded deformation |
| Ground contact | Rotated/scaled support point meets the terrain tangent plane with bounded embed |
| Material parity | Standalone rock palette/noise matches exposed terrain rock |
| Spatial index | Packed nine-cell result equals brute force |
| Pool cap | Runtime slot count is fixed and overflow selects nearest candidates |
| Stability | Leaving and returning restores identical transform and appearance |
| LOD visibility | Rocks render only over the active 3x3 LOD0 neighbourhood |
| Overview | No rocks render without first-person focus |
| Lifecycle | Toggle, cancellation, regeneration, and destruction release all owners |
| Performance | Zero steady-state GC, zero movement-time FFI, bounded transition time |
| Collision | If enabled, only large boulders use pooled primitive colliders |

## Risks and Mitigations

### Settled drop groups may greatly exceed the renderer pool

The generator can retain several bodies from every settled drop group, so a 3x3
streaming neighbourhood may contain many more candidates than renderers. Keep
the immutable source list and packed index, but render only the nearest 128 with
the existing fixed pool. Do not increase the pool to match source density
without first replacing per-rock GameObjects with a measured instanced-rendering
path.

### A fixed class split may create oversized boulders on narrow ledges

Use conservative initial boulder sizes, surface alignment, and embedding.
Diagnostics should show slope and nearby terrain context. If ledges still look
wrong, derive the boulder probability or maximum size from sampled slope in the
prepared candidate; keep that rule deterministic and test it explicitly.

### Normal-map sampling is approximate

The normal map is 2048x2048 and derived from LOD0, but it is still rasterized.
The source position remains authoritative, and support embedding masks small
normal errors. If runtime evidence shows unacceptable orientation on sharp
ridges, add a dedicated owned native rock record with exact sampled normal as a
separate ABI change rather than using physics raycasts against the approximate
TerrainCollider.

### Large irregular boulders can intersect curved terrain

A tangent plane cannot model terrain curvature across a three-metre footprint.
Keep early boulders modest, embed them, and inspect high-curvature sites. A
later refinement may sample several native support points, but must remain a
background preparation operation rather than a movement-time native query.

### Renderer objects may not batch as expected

Use shared meshes, one instanced material, and property blocks limited to
instanced properties. Confirm with the profiler. If automatic instancing is
insufficient, move visual submission to fixed per-prototype matrix arrays and
`Graphics.DrawMeshInstanced`; keep separate pooled GameObjects only for the few
collidable boulders.

### Cell transitions may visibly pop

Retain assignments across overlapping neighbourhoods and update only after
incoming LOD0 geometry is ready. If the remaining outer-row transition is
visible, use a short dither fade with fixed state. Do not dynamically expand
the pool or show rocks over terrain that has not been installed.

### Per-instance material changes may clone materials

Never access `renderer.material`. Use `sharedMaterial` plus a reusable
`MaterialPropertyBlock`, and validate material counts across regeneration.

### Boulder colliders can destabilize player movement

Keep collision out of the initial visual phases. If enabled, use conservative
primitive colliders, disable before transform changes, and test boundary
transitions around the player. Never use moving non-convex mesh colliders.

## Required Validation Commands

After implementation, run the repository's existing checks plus focused rock
tests:

```text
cargo fmt --all -- --check
cargo test --lib
cargo test --test generation
cargo test --test seam_diagnostic
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
Unity BatchValidateNativeInterop
```

This settling implementation changes Rust and extends the decoration ABI.
Rebuild and copy `libmotu.dylib`, then restart Unity before play-mode review.

Follow batch validation with an interactive Unity profiler pass covering:

- entry into first person;
- poor-soil, exposed-rock, grassy, and steep terrain locations;
- river banks and in-channel river triangles;
- repeated LOD1-cell boundary crossings;
- visibility off/on;
- overview return and re-entry;
- several island regenerations;
- a deliberately high-rock seed; and
- optional boulder collision near a cell boundary.

Record total candidates, maximum eligible candidates in any 3x3 neighbourhood,
pool overflow, active prototype groups, draw calls, triangles, transition time,
steady-state allocation, material/mesh counts, and screenshots with placement
gizmos.

## Definition of Done

The feature is complete when Unity copies the existing deterministic Rust rock
positions during background preparation, derives stable stone/boulder
appearances, and displays nearby entries through a fixed pool synchronized with
the active 3x3 LOD0 terrain neighbourhood. Every instance must use a shared
irregular sphere-derived prototype and a shared material visibly related to the
terrain's exposed rock, sit convincingly on the free-form surface, and return
with identical appearance after streaming out and back in.

Completion also requires zero steady-state managed allocation, no movement-time
native calls, bounded pool and draw costs, clean visibility/regeneration
lifecycle behaviour, passing native and Unity batch validation, and a recorded
interactive profiler pass. Trees, bushes, and exact native surface-normal
export remain separate follow-up work.
