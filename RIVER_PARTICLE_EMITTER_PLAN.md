# River Rough-Water Particle Emitter Plan

## Status

Implemented on 2026-08-14. The Rust detector and owned C ABI export, background
Unity copy, packed 64x64 index, fixed 32-system pool, LOD1/first-person
visibility rules, debug gizmos, native validation, and documentation are in
place. Final visual tuning and an interactive Unity profiler capture remain
runtime verification tasks.

The first visual tuning follow-up replaced the default standard-particle quad
with `Motu/River Spray Particle`, a dedicated alpha-blended shader with a
procedural feathered circular alpha profile. Launch speed was reduced to
0.30-1.35 m/s, lifetime to 0.35-0.80 s, diameter to 3.5-12 cm, cone radius to
6 cm, and noise force to 0.12 so spray stays close to the water.

The next visual pass broadened the emission cone from 12 to 50 degrees, added
0.18 random-direction mixing, varied each particle between 40 and 100 percent
of the emitter's capped speed, enlarged the source radius to 8 cm, and raised
noise force to 0.18. Maximum speed and lifetime remain capped, so the broader
spray does not regain the former long-range hose behaviour.

The following tuning pass widened the cone again to 80 degrees and exactly
doubled particle lifetime from 0.35-0.80 s to 0.70-1.60 s. Speed and particle
size remain capped at their previous values.

The candidate-suppression spacing passed by Unity was subsequently reduced from
2 metres to 1 metre, allowing denser rows of emitters across waterfall lips
without changing the fixed 32-system runtime pool.

The original normal-tilt detector was replaced after runtime inspection showed
that it selected coplanar vertices halfway down waterfall sheets and its
normal-relative origin offset could put particle sources behind the water. The
implemented detector now measures shared-edge dihedral sharpness, selecting the
top and bottom waterfall transitions, and Unity provides ten centimetres of
vertical clearance above the water surface.

At the then-current 2-metre spacing, the updated release profiling seed produced
11,869 raw qualifying vertices and 2,698 spaced locations, including 862
accepted river-edge locations. Extraction
took 37.38 ms within a 17.49 s island generation (about 0.21%), remaining below
the one-percent extraction budget. This includes the slope-class guard described
below, not just the raw dihedral test.

This document is the implementation contract for deriving fixed rough-water
locations from the final Rust river mesh, exporting them through the native
C ABI, indexing them once in Unity, and serving the nearest locations from a
fixed pool of particle systems as the player moves.

## Objective

Add local spray, mist, and foam around rough river surfaces, especially
waterfalls and noisy constricted river edges, without creating or destroying
particle systems during play and without scanning every river vertex each
frame.

The detector is deliberately geometric:

- use the final unsliced river mesh produced by Rust;
- calculate the dihedral angle between the final face normals on each shared
  river-mesh edge;
- accept the edge's vertices when that local bend exceeds a configurable
  threshold;
- use that normal as the initial particle outflow direction; and
- reduce dense runs of qualifying vertices to deterministic, spatially
  separated emitter locations.

The resulting locations are fixed for the lifetime of an island. Unity will
only change which locations are represented by the fixed particle-system pool.

## Current Architecture

The relevant path currently is:

1. `RiverNetwork::into_parts` builds the final river mesh in
   `island-rs/src/rivers.rs`.
2. The waterfall lip is selectively tessellated and rounded, then
   `Mesh::calculate_normals` produces final area-weighted vertex normals.
3. `Island` owns that authoritative unsliced river mesh.
4. `CreateRiverMeshGrid` slices it into the 64x64 LOD1 river chunks copied by
   Unity's background `PrepareIsland` task.
5. `TerrainTileStreamer` activates river chunks across the current 3x3 LOD1
   group neighbourhood and already receives the player's world position.

Emitter detection must run against the authoritative unsliced mesh, not the
sliced chunks. Slicing creates duplicate boundary vertices and would otherwise
produce duplicate emitters at tile borders.

## Non-Negotiable Behaviour

- A generated island has a deterministic, immutable emitter candidate set.
- Particle-system object count is fixed after initialization.
- Player movement performs no native calls and no full candidate-array scan.
- Steady-state pool updates allocate no managed memory.
- Distant pool members are reassigned to closer fixed locations; particle
  systems are not instantiated or destroyed during this process.
- Sharp river-edge vertices remain fully eligible. Their incident face bends
  often identify noisy constrictions and turbulence that should receive the
  same effects as waterfalls.
- Emitters are active only in first-person mode, only when river rendering is
  enabled, and only where the corresponding river terrain is at least LOD1.
- Island regeneration, cancellation, overview return, and viewer destruction
  release both native export ownership and Unity objects correctly.
- Rust remains Z-up and normalized to a one-unit-wide island; Unity performs
  the existing conversion to a Y-up, 2,000-metre-wide world exactly once.

## Key Design Decisions

### Derive candidates from the final river mesh

Candidate extraction should happen after waterfall-lip rounding and the final
normal calculation. It should be exposed as a borrowed query on `Island` or as
a helper taking `&Mesh`; it must not mutate or clone the river mesh.

Do not persist emitters in the island save format. They are derived data and
can be rebuilt deterministically from the saved river mesh, avoiding a file
format version change and duplicate long-lived Rust storage.

### Define roughness as shared-edge sharpness

For normalized finite face normals `a` and `b` sharing an edge, define:

```text
alignment        = clamp(dot(a, b), -1, 1)
dihedral_radians = acos(alignment)
```

The threshold is converted once from degrees to a dot-product threshold, so the
edge loop needs only a dot product and comparison. A vertex receives the
strongest qualifying dihedral of its incident shared edges. Coplanar horizontal
and vertical patches both score zero; only a local bend qualifies.

To reject incidental bends within an irregular but consistently sloped patch,
the two faces must also straddle slope classes: at least one face is no steeper
than 35 degrees from horizontal and at least one face is 55 degrees or steeper.
This retains the flat-to-falling transitions at waterfall tops and bottoms and
similar constricted-edge turbulence.

The initial threshold is 35 degrees. Flat reaches and the interior of a planar
waterfall face are rejected. The near-right-angle transitions at the top and
bottom are accepted. Invalid faces and non-finite or downward final vertex
normals are rejected rather than treated as maximum-strength emitters.

The threshold is an extraction setting, not a terrain-generation setting. It
should initially be a named constant in Unity passed to the native export. This
allows later tuning without changing `MotuOptions` or regenerating terrain.

### Keep locations sparse and deterministic

Adaptive tessellation can place many qualifying vertices on one waterfall.
Exporting every one would waste memory and make the pool select several nearly
identical points.

Use deterministic non-maximum suppression:

1. Build lightweight candidates containing vertex index, position, normal,
   sharpness strength, and an edge/interior classification for diagnostics only.
2. Sort by descending sharpness strength and then ascending vertex index.
3. Accept a candidate only when no previously accepted candidate is within the
   minimum spacing.
4. Use an XY uniform-bin index to test neighbouring accepted points, followed
   by an exact 3D squared-distance check.
5. Sort the accepted export into stable vertex-index order, or retain an
   explicit stable ID, so repeated exports are byte-for-byte deterministic.

The current minimum spacing is 1 metre in world scale
(`1.0 / 2000.0` in Rust coordinates). This is close enough to allow several
emitters across a wide waterfall while preventing a tessellated patch from
consuming the entire pool.

Do not exclude or down-rank river perimeter vertices. Steep edge vertices are
intentional candidates because they commonly represent noisy, constricted flow.
Record edge/interior classification in Rust-only diagnostics so the two sources
can be compared, but apply the same threshold, strength, spacing, and pool
priority to both.

### Export compact records, not meshes

Add an explicit C-compatible record:

```rust
#[repr(C)]
pub struct RiverEmitterExport {
    pub position: Vec3,
    pub direction: Vec3,
    pub strength: f32,
}

#[repr(C)]
pub struct ExportRiverEmitters {
    pub handle: *mut c_void,
    pub data: *const RiverEmitterExport,
    pub length: i32,
}
```

`strength` is the normalized amount by which sharpness exceeds the threshold.
It gives Unity a stable value for emission rate, particle speed, size, and
opacity without another terrain query. `direction` is the normalized final
river vertex normal, as requested.

Add paired entry points to `ffi.rs`, `include/motu.h`, and `MotuNative.cs`:

```text
CreateRiverEmitters(island, sharpness_degrees, spacing_metres, output)
ReleaseRiverEmitters(output)
```

The create call owns one `Vec<RiverEmitterExport>` behind `handle`. Unity copies
the records while still on its background preparation task and releases the
native allocation in a `finally` block. Rust does not retain a second candidate
array in `Island`.

### Use a packed uniform-grid index in Unity

A quadtree is unnecessary for a fixed 2 km square and fixed points. Use the
same 64x64 world partition already used by LOD1 river tiles:

- `EmitterCandidate[] candidates` stores immutable converted world-space data;
- `int[4097] cellOffsets` identifies each cell's packed range; and
- `int[] candidateOrder` stores candidate indices grouped by cell.

Construction uses one temporary count/cursor array and happens once after
generation. Queries visit only cells intersecting the activation radius and
then test exact 3D squared distance. All query scratch arrays are allocated
once with the fixed pool.

This index deliberately remains Unity-owned. Rust is responsible for static
geometric classification; Unity is responsible for player-relative selection.
No FFI call should occur as the player moves.

### Use a fixed pool with stable reassignment

Create a dedicated `RiverParticlePool` component under the existing river root.
The terrain streamer should delegate player position, first-person focus, and
river visibility to it rather than mixing particle configuration into terrain
mesh construction.

Initial tuning constants:

| Setting | Initial value |
| --- | ---: |
| Pool size | 32 particle systems |
| Full selection radius | 180 m |
| Retirement radius | 220 m |
| Re-query movement | 5 m or one index-cell change |
| Normal threshold | 35 degrees |
| Candidate spacing | 1 m |
| Surface offset | 0.05 m along outflow normal |

The larger retirement radius provides hysteresis. A currently assigned
location should remain assigned while it is inside 220 m unless the pool is
full and an unassigned candidate is meaningfully closer. This prevents pool
members from chattering between similarly distant waterfalls.

At each query:

1. Retain current assignments that remain eligible.
2. Visit candidate cells intersecting 180 m.
3. Maintain the nearest unassigned candidates in preallocated fixed-size
   index/distance arrays; pool size 32 makes a simple insertion routine clearer
   and cheaper than allocating a priority queue.
4. Fill free slots first.
5. Reassign the farthest retained slot only when a new candidate is closer by
   a small hysteresis margin.
6. For a reassigned slot, stop and clear it, move it to the candidate, set its
   rotation and strength properties, then play it.

Pool-slot lookup can scan the 32 fixed assignments instead of maintaining a
dictionary. This keeps the logic allocation-free and the constant factor tiny.

### Particle orientation and appearance

Unity converts positions with the existing mapping:

```text
Rust (x, y, z) -> Unity ((x - 0.5) * 2000, z * 2000, (y - 0.5) * 2000)
```

Directions use the corresponding axis swap without translation:

```text
Rust (x, y, z) -> normalize(Unity(x, z, y))
```

Orient the particle system's known local emission axis to the converted
direction using `Quaternion.FromToRotation`. Add an automated direction test
because Unity's particle shape module axis must be confirmed rather than
assumed.

The first visual should be a lightweight coherent spray/mist effect:

- one shared particle material and texture for the entire pool;
- world-space simulation;
- a narrow cone or directed box aligned to the exported normal;
- translucent off-white/blue particles;
- modest downward gravity after initial outward velocity;
- noise for breakup;
- emission rate, speed, size, and opacity scaled by exported strength; and
- bounded lifetime so recycled emitters do not leave long-lived particles.

Ten centimetres of vertical clearance is applied in Unity, where it has an
explicit world-metre meaning. Unlike a normal-relative offset, it cannot move a
nearly horizontal waterfall emitter sideways behind the water sheet.

## Implementation Phases

### Phase 0: Diagnostics and baseline

1. Add a temporary Rust diagnostic for a few representative seeds reporting:
   qualifying vertex count before suppression, accepted count after
   suppression, sharpness distribution, edge/interior share, and candidate spacing.
2. Capture the current river-mesh vertex count and island generation time so
   extraction overhead is measurable.
3. Inspect at least one waterfall and one noisy constricted river edge in Unity
   to establish the desired visual radius and confirm which way the converted
   normal points.

Exit criteria:

- The 35-degree/1-metre defaults yield a non-empty but bounded set on normal
  generated islands.
- Steep constricted edges remain represented after spacing suppression.
- Candidate extraction is small compared with total generation time.
- Normal direction at waterfall faces is confirmed visually or with gizmos.

### Phase 1: Rust candidate extraction

1. Add an internal `RiverEmitter` value type separate from the FFI record.
2. Implement a borrowed helper over the final `river_mesh` that:
   - validates matching vertex/normal lengths in debug/test builds;
   - converts the angle threshold to a dot threshold once;
   - calculates final face normals and shared-edge dihedral sharpness;
   - computes normalized strength;
   - applies deterministic spacing suppression; and
   - returns only the compact accepted records.
3. Reuse the packed offset/bin pattern already used by `RiverPointIndex` rather
   than doing an all-pairs suppression scan.
4. Keep all coordinates normalized and all directions unit length.

Exit criteria:

- Coplanar horizontal and vertical test patches yield no candidates.
- Waterfall test strips select the top and bottom transitions but no vertices
  halfway down the planar falling sheet.
- Sharper test folds produce monotonic strength.
- Repeated extraction returns identical records in identical order.
- No accepted pair violates the requested minimum spacing.

### Phase 2: Native ABI export

1. Add `RiverEmitterExport` and `ExportRiverEmitters` to Rust.
2. Add create/release functions with the same null-safe, single-owner pattern as
   the current mesh and surface-map exports.
3. Mirror layouts in `include/motu.h` and `MotuNative.cs`.
4. Add ABI layout/length tests and extend the FFI allocation test to create,
   inspect, and release an emitter array.
5. Extend `IslandViewer.BatchValidateNativeInterop` to assert that records are
   finite, positions are inside plausible island bounds, directions are unit
   length, strengths are in `[0, 1]`, and release clears ownership.

Exit criteria:

- Rust, C, and C# layouts agree on Apple Silicon.
- Every successful create has exactly one release path.
- Empty candidate sets are represented safely without requiring a fake record.
- Native interop validation passes with the rebuilt release plugin.

### Phase 3: Background preparation and coordinate conversion

1. Add an immutable managed `PreparedRiverEmitter` record.
2. In `PrepareIsland`, call `CreateRiverEmitters` after native island generation
   and before transferring the island handle to the main thread.
3. Copy and convert candidates in the existing background task.
4. Store the managed candidate array in `PreparedIsland` and pass it into
   `TerrainTileStreamer.InitializeAsync` with the prepared river tiles.
5. Release native records immediately after copying, including cancellation and
   exception paths.

Exit criteria:

- No emitter marshal/copy work is performed on the Unity main thread.
- Cancellation and failed generation leak neither native nor managed owners.
- Converted directions match converted mesh normals for sampled vertices.

### Phase 4: Unity spatial index

1. Add `RiverEmitterIndex` with immutable candidates, 64x64 packed cells,
   offsets, and candidate order.
2. Consume the prepared array once during streamer initialization.
3. Implement a no-allocation radius enumerator or direct visitor over intersecting
   cells.
4. Add deterministic nearest-candidate selection using preallocated arrays.
5. Add focused tests for world edges, empty cells, radius boundaries,
   deterministic distance ties, and a player crossing a cell boundary.

Exit criteria:

- Queries inspect only relevant cells.
- The nearest set matches a brute-force reference in tests.
- Repeated runtime queries allocate zero bytes.

### Phase 5: Fixed particle-system pool

1. Add `RiverParticlePool` and construct exactly 32 child particle systems once.
2. Create one shared material/texture and apply it to every renderer.
3. Implement stable slot assignment, distance hysteresis, and root-level
   enable/disable behavior.
4. Feed player positions from `TerrainTileStreamer.SetPlayerPosition`.
5. Clear/disable the pool from `ClearPlayerFocus`, the river visibility toggle,
   island regeneration, and `Dispose`.
6. Ensure activation radius remains inside the active 3x3 LOD1-group region so
   spray is never shown over a culled LOD2 river surface.

Exit criteria:

- Particle-system count remains exactly 32 while walking and regenerating.
- No emitter appears in overview mode or when rivers are hidden.
- Moving between waterfalls reuses the farthest slots for closer candidates.
- No visible orphan particles remain at recycled locations beyond the chosen
  bounded lifetime policy.

### Phase 6: Visual tuning and controls

1. Add a debug overlay option to show candidate points, normal rays, active
   assignments, and activation/retirement radii.
2. Tune threshold, spacing, pool size, emission strength, lifetime, speed,
   gravity, noise, size, and colour against several seeds.
3. Keep detector constants separate from visual constants.
4. Only add public sliders after useful ranges are established. Threshold and
   spacing can re-export from the retained native island handle; purely visual
   values update the existing pool immediately.
5. If vertex normals prove too averaged at some waterfall lips, evaluate
   face-normal or waterfall-metadata augmentation as a later detector. Do not
   silently change the agreed first-phase vertex-normal rule or remove steep
   edge candidates.

Exit criteria:

- Waterfalls reliably receive spray without continuous lines of emitters along
  ordinary flat rivers.
- Particle direction reads as outflow rather than inward spray.
- Effect density changes smoothly as stronger candidates enter the pool.

### Phase 7: Full validation and documentation

Run:

```text
cargo fmt --all -- --check
cargo test --lib
cargo test --test generation
cargo test --test seam_diagnostic
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
Unity BatchValidateNativeInterop
```

Then perform a Unity first-person profiling pass covering a waterfall, a flat
river, LOD1 group changes, river visibility toggles, overview return, and island
regeneration.

Record:

- raw and suppressed candidate counts;
- fixed and peak-active pool counts;
- candidate extraction time;
- maximum query/update time;
- managed allocations per steady-state update;
- native allocation/release balance; and
- screenshots or video with candidate gizmos and final particles.

Update both READMEs with the detector, C ABI ownership, pool behaviour, tuning
constants, and plugin rebuild/restart instructions.

## Test Matrix

| Area | Required proof |
| --- | --- |
| Rust geometry | Coplanar vertical faces rejected; sharp folds accepted at the threshold boundary |
| Waterfall placement | Top and bottom transition edges selected; middle of planar fall rejected |
| Rust direction | Export direction equals normalized final river vertex normal |
| Rust spacing | No accepted pair is closer than minimum spacing |
| River edges | Steep edge and interior vertices use identical eligibility and priority rules |
| Determinism | Same island and settings produce identical records and ordering |
| ABI ownership | Create/release succeeds for empty and non-empty arrays without leaks |
| Coordinate mapping | Position and direction match Unity's existing mesh conversion |
| Spatial query | Packed-grid nearest set equals brute-force reference |
| Pool cap | Exactly 32 systems exist; active count never exceeds 32 |
| Reassignment | A newly closer candidate replaces the farthest eligible assignment |
| Hysteresis | Small player movement near a distance tie does not chatter assignments |
| Visibility | Disabled in overview, with rivers off, and over LOD2-only terrain |
| Lifecycle | Cancellation, regeneration, and destruction release all owners |
| Performance | Zero steady-state GC allocation and no per-frame native call |

## Performance and Ownership Budget

Rust performs one face-normal pass, sorts one compact shared-edge list, scans
the resulting edge groups, and performs deterministic suppression during
export. It allocates temporary face normals, edge records, per-vertex sharpness,
qualifying records, the packed suppression index, and the final native export
vector. The river mesh remains borrowed and is never cloned.

Unity performs one native-to-managed copy on the existing generation worker and
one packed-index build during upload. Long-lived storage is limited to the
candidate array, two integer index arrays, fixed query scratch, 32 particle
systems, and one shared material/texture. Runtime player updates reuse all
storage.

The initial targets are:

- emitter extraction below 1% of total island generation time;
- pool query/reassignment below 0.25 ms at the 95th percentile on the current
  development Mac;
- zero bytes of managed allocation during steady movement; and
- no increase in particle-system object count after initialization.

## Risks and Mitigations

### Constricted edges may provide many strong candidates

This is desirable: sharp bends around river edges often identify noisy
constrained flow that should produce spray. Preserve those candidates and expose them in
debug views. If they dominate the fixed pool, tune the global spacing,
activation radius, or strength response rather than filtering or down-ranking
edges as a class.

### Rounded waterfall lips may distribute sharpness

The rounded top lip may distribute one large fold across several smaller
dihedrals. The detector uses the maximum incident shared-edge angle and the
35-degree threshold to retain the strongest part of that curve. If a very soft
lip is missed, lower the sharpness threshold or augment with the existing
waterfall-lip mask while retaining deterministic suppression.

### Pool reassignment can visibly pop

Always replace the farthest assignment, use activation/retirement hysteresis,
and bound particle lifetime. If popping remains noticeable, fade emission rate
before clearing a slot; do not grow the pool dynamically.

### Emission-axis mistakes can reverse spray

Rust and Unity swap axes and triangle winding. Use the same explicit direction
conversion as mesh normals and validate the particle system's local emission
axis with a known-normal test and debug ray.

### Too many candidates can increase preparation time

Apply thresholding and deterministic spatial suppression before crossing the
C ABI. Never export every river vertex to let Unity filter it.

## Definition of Done

The feature is complete when a generated island exposes a deterministic set of
shared-edge-sharpness rough-water locations, Unity indexes those locations once, and
a fixed 32-system pool follows the player by reassigning distant systems to
closer candidates. Waterfall and constricted-edge spray must align with exported
normals, remain bounded to visible first-person LOD1 river regions, allocate
nothing during steady movement, survive regeneration and visibility changes,
and pass the Rust, native interop, Unity compile, lifecycle, and profiling
checks above.
