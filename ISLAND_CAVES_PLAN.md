# Island Caves Implementation Plan

Status: initial cave implementation added. See [ISLAND_CAVES.md](ISLAND_CAVES.md)
for the implemented architecture, usage, validation and remaining refinements.
The design below records the original plan; the implementation notes take precedence
where the chosen terrain boundary and collision policy differ.
Written: 2026-09-09. Repository baseline: `3716849` on `main`.

## 1. Intended result

Generate explorable caves as separate meshes after the island's terrain and
erosion have finished. An entrance belongs at the foot of a sufficiently tall,
steep rock face, where that face meets ground flat and wide enough to walk on.
The cave floor meets that ground without a step, and the tunnel travels into
the hillside with enough rock above and beside it.

Use a connected tunnel/chamber layout to define the empty space, a continuous
3D density field to shape it, and marching cubes to generate its surfaces.
Sample only bounded regions containing caves. The existing island generator
continues to generate the surrounding land.

The first playable version has one entrance, one winding passage and one
chamber per accepted cave. Generation is deterministic from the island seed
and cave settings. An unsuitable island may have no caves; the generator must
not manufacture a bad entrance to satisfy a requested count.

Initial scope excludes branching networks, underground rivers, flooded caves,
digging, collapsing rock, caves crossing between islands, and automatic ship
disembarkation. These can be added after the basic entrance and traversal work.
The existing first-person controller and a debug entrance action provide the
initial way to test walking inside.

## 2. Current code and integration constraints

These observations are from the checkout at the baseline above, not assumptions
based on the older architecture plans.

| Existing area | Relevant behaviour and required extension |
| --- | --- |
| `island-rs/src/terrain/generation.rs` | Finishes terrain and rivers, regenerates LODs, constructs `Terrain`, then generates decorations and vegetation. Insert cave planning against the final surface before placing objects that could block entrances. |
| `island-rs/src/terrain/sampling.rs` | Provides indexed terrain sampling. Reuse its spatial lookup for candidate discovery; inspect actual triangles for entrance geometry and clearance. |
| `island-rs/src/mesh_clipper.rs` | Slices terrain into tile meshes and handles LOD edge transitions. It is not a general cave Boolean operation; entrance exclusion and boundary stitching need explicit support. |
| `IslandGenerationProfile` / `IslandGenerationRequest` | Copy settings into an owned generation request. Cave settings must participate in copying, factories and request construction. |
| `IslandPreparationPipeline` | Loads or generates a native island, saves its snapshot, then prepares managed data on the worker. Cave data must exist before snapshot saving and installation. |
| `IslandPreparedData` / `IslandRuntime` | Transfer and own prepared data, native handles and installed Unity resources. Caves belong to this per-island lifetime. |
| `TerrainTileStreamer` | Displays custom terrain meshes at multiple LODs. An opening must survive every representation that can cover it. |
| `TerrainTileStreamer.Colliders.cs` | Uses hidden `TerrainCollider` tiles: 64 tiles per island edge, 129 height samples per tile, normally a 3x3 neighbourhood. Cave collision needs separate meshes and local heightfield holes. |
| `TerrainDetail.shader` | Implements `Motu/Terrain Unified`, the active detailed terrain material. Its top-surface normal and occlusion maps cannot describe a cave ceiling. |
| `IWorldSurfaceQuery` / `FirstPersonController` | Surface entry snaps to terrain; normal movement uses a `CharacterController`. Underground placement needs a height-aware ground query rather than a top-down surface snap. |
| `terrain/snapshot.rs` / `IslandSnapshotCache` | Native snapshot version is currently 4; Unity cache-key schema is 6. Cave options and generated data require deliberate versioning. |

The local `caves.rs` and `caves/voxels.rs` files are an unfinished experiment,
not a functioning cave generator. The fixed `1024 x 1024 x 64` array of `u128`
occupies about 1 GiB before other data; its indexing currently uses only 16 bits
per word. Do not build the production mesher around that allocation. During
implementation, reconcile this experiment explicitly with the new bounded
representation. This planning task leaves it untouched.

## 3. Ownership and generation order

```text
Island seed + copied cave settings
    -> normal terrain / erosion / rivers / final surface
    -> entrance candidates + approach validation
    -> connected passage and chamber layout
    -> clearance validation + reserved entrance region
    -> density sampling + cave meshes + entrance replacement patch
    -> validated render cuts, collider holes and placement exclusions
    -> remaining decoration / vegetation generation
    -> complete versioned island snapshot
    -> worker copies bounded cave buffers through FFI
    -> Unity installs island-owned render meshes and colliders
```

Keep the uncut final terrain available as the authoritative input for surface
height, geology, coast and erosion queries. Cave openings are an additional
surface representation owned by the cave result. Do not append underground
ceilings and floors to a mesh that existing code assumes is single-valued in
horizontal coordinates.

Use an independent seed stream for cave generation, derived from the island
seed, a cave algorithm revision and stable candidate/cave identifiers. Adding
caves must not consume the random sequence used by terrain or trees. Exclusion
tests must preserve placement randomness outside the affected region.

Reject individual invalid candidates without partially changing the island.
Publish an accepted cave only when its mesh, entrance patch, collider data and
exclusion region all validate. Resource exhaustion or cancellation is not the
same as an island legitimately having no suitable entrances: report these
outcomes separately and do not cache a silently incomplete result.

## 4. Entrance selection

### 4.1 Find the foot of a face

Start with a coarse scan of the final terrain for a transition from low slope
to high slope. Refine promising regions using actual triangle geometry and
multiple profiles directed into the hill. The horizontal uphill direction
gives the initial tunnel heading; the face normal refines it.

Evaluate a footprint, not a single triangle or height sample. Reject isolated
steep triangles, narrow ledges, loose decorative boulders and faces whose height
disappears a metre to either side. Discovery resolution must be smaller than
the minimum entrance width, or use triangle-based candidates to avoid missing
small valid faces.

### 4.2 Validate the approach

Define a rectangular landing outside the mouth, at least entrance width plus
side clearance, extending several metres away from the face. Test:

- Maximum longitudinal and cross slope over the whole landing.
- Maximum local step and height variation, including its junction with the mouth.
- Standing headroom, lateral capsule clearance and a clear route to the mouth.
- A connected walkable route from the landing into a larger exterior terrain
  neighbourhood. A flat but isolated ledge does not qualify.
- Separation from rivers, waterfall feet, shore hazards and island boundaries.

Use a local walkability grid or sampled capsule corridor for this first route
test. It establishes local accessibility, not a claim that every entrance is
reachable from every beach on the island. Do not require a whole-island NavMesh
for the initial version.

### 4.3 Validate the face and interior

Measure face height across the full opening width. Require room for the opening,
side walls and roof margin, not merely a high point at its centre. Sample behind
the face along the proposed tunnel corridor and around each cross-section.

Roof thickness is measured from the proposed ceiling to exterior ground.
Vertical clearance is a useful early rejection test, but it is insufficient at
a thin ridge or cliff edge: also test nearest exterior surfaces in lateral and
diagonal directions. Use a local triangle acceleration structure for the final
clearance check. Reject unsupported or ambiguous folded surface regions in the
first version rather than assuming that a height lookup describes solid rock.

The mouth itself intentionally breaks through the face. Apply full minimum
cover only after an explicit entrance transition length, increasing the cover
requirement smoothly from the opening into the hillside.

### 4.4 Rank, select and explain

First apply hard validity rules, then score acceptable candidates for approach
quality, face width/height, interior cover and separation from other caves.
Resolve ties with stable identifiers and seeded ordering. Apply minimum spacing
and bounded retry counts. A target count is a maximum opportunity, not a promise.

Record rejection reasons such as `approach too steep`, `ledge disconnected`,
`face too short`, `insufficient roof`, `river conflict` and `patch too large`.
Expose these in editor diagnostics so tuning does not require guessing why a
seed produced no cave.

## 5. Layout, shape and walkability

Represent the layout as nodes and connecting segments, even though the first
version contains a single passage ending in a chamber. Store a floor elevation,
width and clear height along each segment. This supports later branches without
making branch generation part of the first milestone.

The first segment follows the approach elevation and points inward. Subsequent
segments have bounded heading changes, curvature and floor gradient. Check
rock cover along entire segments and chamber envelopes, including the possible
noise displacement. Shorten, redirect or reject paths that approach the surface,
rivers or island edge. Do not leave abrupt cut ends when a generation bound is
reached: finish with a valid closed end or reject the proposal.

Build the empty volume from rounded passage cross-sections and ellipsoidal
chambers. Blend their junctions. Constrain the lower cross-section to provide
a broad floor rather than the narrow walking strip of a circular pipe.

Use broad coherent noise to vary walls and chamber shape, then smaller noise
for geometric roughness. Reduce roughness near the walking floor, entrance join
and protected boundary vertices. Put sub-voxel detail into material normals;
noise finer than the sampling lattice must not create unstable geometry.

After meshing, validate the navigable corridor again using the intended player
capsule. A connected graph does not guarantee a connected opening after noise,
blending and polygonization. Check floor steps, slopes, headroom, wall clearance
and entrance-to-chamber reachability on the final geometry.

## 6. Density field and marching cubes

Use a documented sign convention: negative means solid rock, positive means
air, and zero is the surface. For a simple local top surface in metre-based
coordinates, `terrainField(p) = p.up - surfaceHeight(p.horizontal)`. A passage
field is positive inside the desired empty volume. Their union of air is
`max(terrainField, passageField)`; the remaining negative region is rock.

The terrain expression is a signed scalar field, not a true Euclidean signed
distance on steep slopes. Do not use its magnitude as the roof-thickness test.
Evaluate the passage field underground; evaluate the combined terrain/air field
only in the entrance replacement region. Preserve actual outer terrain geometry
at the join instead of globally rebuilding the island from a heightfield.

Marching cubes extracts the zero surface from the corner samples of each cell.
Its density-field approach supports the required irregular cave topology.
[NVIDIA's procedural terrain chapter](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
provides the algorithmic reference; its historical GPU implementation is not
a requirement for this project's CPU implementation.

Implementation requirements:

- Heap-allocated, sparse active chunks; no whole-island 3D volume.
- Start with 32 cells per chunk and 0.5 m cell spacing as tunable trial values.
  This is a 16 m chunk with 33 cubed scalar samples before gradient padding.
- An island-local integer lattice gives adjacent chunks identical shared samples.
  Include halo samples for gradients and evaluate noise from absolute lattice
  coordinates, never a chunk-local random sequence.
- Interpolate zero crossings, share edge vertices within chunks, and make boundary
  positions and normals identical across chunks. Use a tested ambiguity-resolving
  marching-cubes variant; consistent shared samples alone do not resolve all
  ambiguous face/interior cases.
- Point solid-surface normals toward air, so cave walls face the cavity and outer
  rock faces outward. Verify winding after the Rust-to-Unity coordinate conversion.
- Reject non-finite vertices, invalid indices, degenerates and unintended open
  boundaries. Avoid T-junctions and protect doorway/floor clearance when simplifying.
- Use fixed resolution initially. Mixed voxel LODs require explicit transition
  meshing and are deferred until profiling justifies that additional work.

Keep all cave computations in island-local metres. Existing Rust terrain uses
horizontal XY with Z elevation; Unity uses XZ with Y elevation. Convert at a
single documented boundary, including scale, handedness and triangle winding.
Never add large world-cell offsets before sampling noise or meshing caves.

## 7. Entrance patch: rendering and collision

This is the highest-risk part and must be proved before procedural networks.

### 7.1 A replacement region with an explicit boundary

Choose a small outer footprint around the entrance. Align its collider boundary
to the existing heightfield cell lattice. Keep the tunnel mouth and disturbed
surface well inside this footprint, with an undisturbed transition band.

Clip original render triangles at the footprint rather than deleting whole
intersecting triangles. Extract an ordered boundary from the final terrain and
preserve its positions and surface attributes. Generate the cave mouth and
surrounding surface inside this boundary. Join the generated inner surface to
the original outer boundary with an explicit triangulated transition band.

Boundary stitching is a meshing operation: do not assume marching cubes will
land exactly on the original triangle edges. Validate matched boundary edges,
consistent winding and a non-self-intersecting transition. If the footprint
produces ambiguous boundary loops or needs excessive replacement area, reject
the candidate in the first version. Decorative rocks may soften the appearance
of the join, but cannot conceal a collision gap or substitute for topology.

The patch contains the exterior cliff, the ground above the mouth, the entrance
roof, walls and floor. The interior tunnel attaches to it with the same shared
edge rules as the other cave chunks. No coincident render faces should remain.

### 7.2 Terrain LOD policy

For the first version, retain a bounded group of affected terrain tiles at a
fixed detail representation while the island is rendered. These tiles use the
clipped terrain and permanent entrance patch. Existing coarser batches must
exclude their covered region; otherwise the coarse terrain would fill the hole.

Keep the patch boundary outside the tile edge-morph band, or pin its boundary
vertices explicitly. The surrounding tile edges can retain the existing LOD
transition behaviour. Count this permanently detailed area against a strict
per-island budget and reject or merge conflicting reservations deterministically.

Do not let switching to an overview or reflection representation restore an
uncut hillside. Test every terrain rendering route. Later, create simplified
patches and matching cut terrain LODs with protected entrance boundaries if the
fixed-detail cost is too high. This is separate from underground chunk LOD.

### 7.3 Heightfield holes and replacement colliders

`TerrainData.SetHoles` uses `true` for retained surface and `false` for a hole.
Unity terrain holes affect terrain collision as well as rendering; our custom
mesh renderer still requires the geometric cut described above.
[SetHoles API](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/TerrainData.SetHoles.html),
[terrain holes behaviour](https://docs.unity3d.com/6000.0/Documentation/Manual/terrain-PaintHoles.html).

A heightfield hole removes the whole surface at those horizontal coordinates.
It cannot preserve a separate roof over an entrance. Therefore:

1. Store a conservative, cell-aligned hole region for each affected collider tile.
2. Supply static, non-convex `MeshCollider` geometry for every surface removed
   there, including the exterior ground on top of the cave.
3. Supply separate interior wall, ceiling and floor collision. Keep the ordinary
   terrain collider above deep tunnel sections where it causes no obstruction.
4. Cover differences between the render boundary and rasterized collision boundary
   with an explicit collision transition. Test against the quantized heightfield,
   not just the unquantized source mesh.
5. Prepare and validate the replacement before exposing the hole. Swap prepared
   objects before the next physics step, with no frame lacking ground collision.
6. On failure, retain the complete previous surface and keep the cave unopened.

Collision simplification may remove small wall detail but must retain doorway,
floor, roof and join constraints. Test both walking into the cave and walking
across the ground above it. A visually correct cave is insufficient.

The first fixture must also test the current Unity/PhysX behaviour underneath an
intact heightfield tile. Do not assume that an underground character is safe
merely because the visible heightfield surface is overhead. If that collider
obstructs or ejects the character, expand the collision replacement to the full
cave projection and restore the exterior ground with a surface `MeshCollider`.
That larger collision footprint need not enlarge the visual entrance cut. Record
the tested policy before enabling procedural caves.

## 8. Materials, lighting, water and vegetation

Use a cave material drawing on the island's existing stone palette and texture
resources, with triplanar mapping in stable island-local metres. Allow a dirt or
sediment floor blend. Keep material ownership in the existing island material
factory/lifetime and include the shader in player builds.

Use actual cave normals and cave-specific ambient visibility. Do not sample the
island's top-down normal/occlusion maps for ceilings or interior floors, and do
not apply exterior grass or snow rules to the interior. Blend the exterior
transition band back to the island material attributes.

Include direct-light shadows and a bounded cave ambient treatment, fading from
the entrance into the interior. Global sky ambient and reflection probes can
otherwise make enclosed rock appear lit by the sky. Ambient darkening is an
artistic approximation, not proof of physical light occlusion. Validate sunrise,
noon, sunset, night, weather changes and an interior test light. Ensure cave roof
surfaces also occlude light from the required direction without duplicate
coplanar geometry. Check depth, shadow and planar reflection passes.

Reserve the mouth and approach before decoration placement. Exclude tree trunks,
rocks, reeds and ferns that block the opening; suppress grass on removed terrain
and interior surfaces. Preserve legitimate vegetation above a sufficiently thick
roof rather than excluding the entire underground cave footprint.

The first version is dry: keep the full cave floor above sea level plus a
configurable clearance, and outside rivers and their carve regions. The ocean is
a global surface and does not understand enclosed spaces. A flooded/sea-cave
version needs explicit water visibility, wave and water-volume decisions; this
plan does not reuse ship deck clamping to solve cave water.

## 9. Settings and data contracts

Add `IslandCaveSettings` under `Motu.Settings` and copy it through generation
profiles, request factories and immutable request inputs. Scripts configure
caves through the same request/factory mechanism as other island features.
Changing generation settings applies to the next generation, not a live mutation
of installed colliders.

The values below are proposed starting points to test, not measured production
defaults. Keep cave generation disabled for existing profiles until the playable
milestones pass; explicitly enable it in the demonstration profile afterwards.

| Setting | Initial trial | Purpose |
| --- | --- | --- |
| Enabled / seed offset | Off / 0 | Explicit activation and deterministic variation |
| Maximum caves per island | 1 | Bound initial scope; zero valid candidates is allowed |
| Minimum entrance spacing | 100 m | Avoid overlapping cave and approach regions |
| Entrance width / clear height | 4 m / 3 m | Usable doorway before roughness |
| Minimum face slope | 60 degrees from horizontal | Select a distinct cliff face |
| Minimum face height | 6 m | Initial threshold, also enforce opening plus cover geometry |
| Approach maximum slope | 15 degrees | Gentle landing in both directions |
| Approach length / side margin | 6 m / 1 m per side | A useful standing and entry area |
| Approach maximum step | 0.2 m | Below the current controller's 0.35 m step offset |
| Exterior route search radius | 20 m | Reject locally isolated ledges |
| Minimum roof / side cover | 3 m / 3 m | Preserve enclosing rock beyond the mouth transition |
| Entrance transition length | 6 m | Gradually establish full rock cover |
| Tunnel length | 30–60 m | Short, inspectable initial passages |
| Tunnel width / clear height | 3–5 m / 3–4 m | Vary passage size while protecting player clearance |
| Maximum floor gradient | 12 degrees | Keep the intended route comfortably walkable |
| Chamber horizontal diameter / height | 8–12 m / 5–7 m | First destination, subject to cover checks |
| Broad shape noise amplitude / wavelength | 0.5 m / 8 m | Large wall variation |
| Fine geometry noise amplitude / wavelength | 0.15 m / 2 m | Surface irregularity resolved by the trial lattice |
| Floor roughness limit | 0.08 m | Keep fine floor changes below step limits |
| Minimum floor above sea | 5 m | Initial dry-cave restriction, not a universal wave guarantee |
| Voxel spacing / chunk cells | 0.5 m / 32 | Starting geometry and memory tradeoff |

Validate all values for finiteness, ordering and safe allocation sizes. Enforce
compatible entrance/tunnel dimensions, player headroom and noise bounds. Keep
performance controls in an advanced group: candidate attempts, passage segments,
active chunks, triangles, scratch bytes and fixed-detail entrance area.

Proposed native types: `CaveOptions`, `CaveEntrance`, `CaveLayout`, `CaveChunkKey`,
`CaveMeshChunk`, `CaveEntrancePatch`, `CaveGenerationStats`, and an island-owned
`CaveSet`. Names can follow local conventions during implementation.

Persist stable cave IDs, settings/revision, accepted layouts, bounds, mesh chunks,
patch boundaries, hole regions and relevant placement exclusions. Temporary
density samples and search acceleration structures need not be serialized.
On a snapshot hit, restore generated results rather than rerunning the search.

Extend both the native snapshot version and Unity request hash. Hash every
geometry-affecting cave option and the cave algorithm revision in a stable field
order. Old cache entries must regenerate through the established version-miss
path. Update both snapshot and legacy island save/load contracts deliberately;
older authored files should have explicit caves-disabled defaults where their
reader supports migration, rather than silently inventing caves on load.

Use an additive cave-capable native entry point or a deliberately versioned
options contract; do not silently change an existing C struct layout. Export
bounded chunk/patch buffers with explicit counts and paired releases. Copy and
validate on the worker, releasing native export allocations in `finally` paths.
Unity objects are created and destroyed on the main thread. Publish matching
native binaries, C declarations and managed declarations together.

## 10. Streaming, queries and lifecycle

Add an island-owned `CaveStreamer` under `Motu.Streaming`. Register it through
the existing runtime installer and dispose it with `IslandRuntime`. The world
manager supplies focus and player position; it should not own cave mesh details
or a second island-generation queue.

Keep entrance patches visible whenever their affected terrain is visible.
For the small first version, install an entire nearby cave interior before
admitting the player. Retain it while the player is inside, even if the entrance
is far behind or out of view. Later stream interior chunks by 3D bounds and
reachable neighbours, never solely by distance to the entrance.

Install incoming colliders before retiring old ones. Use the existing main-thread
installation budget and cancellation/stale-result rules. Keep the occupied island
resident underground; do not infer island occupancy only from the surface height.
Entering an unavailable cave must wait for a complete safe installation rather
than letting the character fall into an empty region.

Keep existing surface queries explicit: `GetTerrainOrSeaHeight` remains an
exterior query. Add a height-aware nearby-ground query or cave spawn operation
for interior placement, with vertical search bounds and clearance checks.
Do not replace it with a top-down ray that snaps an underground player onto the
roof. Normal cave movement continues through `CharacterController` collision.

For testing, add an editor/runtime debug action to visit an accepted entrance
using the existing first-person mode, and a separate validated chamber spawn.
Preserve the normal ship helm startup. User-facing ship disembarkation remains
a separate feature.

Release cave meshes, colliders, Unity materials and native allocations on normal
unload, cancellation, partial installation failure and regeneration. Reset any
cave shader state when the camera exits or switches. Multiple islands must not
share mutable cave masks or accidentally use another island's transform.

## 11. Implementation sequence and acceptance gates

### Phase 1 — Contracts and an entrance-join proof

Define settings, coordinates, density sign, boundary ownership and resource caps.
Create a small deterministic terrain fixture with flat ground meeting a tall
face. Build one manually located opening with an entrance roof, render cut and
heightfield-hole/replacement-collider pair. A simple passage is sufficient.

Gate: walk through the opening and across its roof; no render crack, invisible
wall, unsupported ground or duplicate collision. Show the join in wireframe and
exercise the affected terrain LOD routes. Test the tunnel beneath retained
heightfield tiles and settle the collision replacement policy. Prove the patch
method before spending time on natural-looking cave networks.

### Phase 2 — Bounded cave meshing

Implement the layout primitives, density field, sparse chunks and marching cubes.
Connect a deterministic passage and chamber to the proven entrance patch. Add
geometry validation and floor/headroom checks.

Gate: continuous interior, consistent chunk seams/normals, correct cavity winding,
bounded allocation, and no collision gap between entrance and tunnel. Halving
voxel spacing should improve detail without changing the intended connectivity.

### Phase 3 — Procedural entrance and route selection

Implement face-foot candidates, approach connectivity, face dimensions, rock-cover
checks, ranking, retries and rejection diagnostics. Generate the first layout
from each accepted entrance. Add decoration exclusions at the correct stage.

Gate: all accepted caves in a fixed seed corpus satisfy the geometric rules;
invalid fixtures reject for the expected reason; cave-free islands remain valid.
Show the selected landing, face and tunnel envelope before and after meshing.

### Phase 4 — Native/Unity pipeline and persistence

Wire settings through factories and requests, add native generation/export,
snapshot data/versioning, managed preparation and per-island ownership. Include
the cave result in snapshot saving before any cache entry is published.

Gate: fresh generation and restored snapshots reproduce the same caves; changing
any geometry option invalidates the key; old/corrupt data follows explicit error
or regeneration paths; allocation/release tests pass, including cancellation.

### Phase 5 — Runtime streaming and traversal

Integrate the permanent entrance reservation with terrain batches, collider
neighbourhoods and island residency. Install nearby interiors, add the bounded
interior ground query and debug visits, and test multiple translated islands.

Gate: enter, leave, traverse tile boundaries, teleport, unload, return and regenerate
without restoring a blocked entrance, losing collision or leaking resources.
Walking inside and on the roof must both work after a cache restore.

### Phase 6 — Appearance and performance acceptance

Add the cave material, entrance blending, ambient treatment and debug lighting.
Profile sampling, mesh extraction, native export, Unity upload and collider cooking
separately. Enable caves in the demo configuration after acceptance.

Gate: inspect representative caves from sea level, at the mouth, inside and from
above, in daylight and darkness, including reflections. Record actual timings,
memory and triangle counts. Commit only after relevant Rust/Unity checks and
manual traversal; report automated and visual validation separately.

### Later extensions

Branching, loops and additional exits; larger chamber networks; stalactites and
stalagmites outside protected walking corridors; interior chunk LOD; navigation,
audio and gameplay placement; sea caves and underground water. Each extension
retains the same entrance, topology, persistence and ownership contracts.

## 12. Validation and performance plan

| Area | Required evidence |
| --- | --- |
| Entrance selection | Valid face/landing; too-short face; steep approach; narrow or disconnected ledge; thin ridge; insufficient roof; river conflict; no valid candidate |
| Walkability | Full player capsule fits entrance, bends and chamber; noisy floor respects limits; route remains connected after meshing |
| Meshing | Finite geometry; legal indices; consistent ambiguous cases; equal shared chunk edges/normals; correct winding; no unintended holes or T-junctions |
| Entrance topology | Matched outer/inner joins; no duplicate faces; preserved roof; rejection of unsupported boundary loops |
| Collision | Enter/exit, roof walk, oblique doorway contact, jump against ceiling, tile-edge entrance, hole quantization and failed-cook rollback |
| Terrain rendering | LOD0/1/2 routes, overview, shadows, reflections and grass never refill or expose the opening |
| Determinism | Identical inputs produce identical ordered results on the supported build; parallel scheduling does not change selection or seams |
| Regression isolation | Caves-disabled terrain/vegetation outputs match baseline; enabling caves preserves terrain/erosion and unrelated placement sequences |
| Persistence | Round trip, changed settings/revision, old snapshot, corruption, interrupted save and bounded serialized allocation |
| Runtime | Two translated islands, cave occupancy, surface/interior spawn, rapid direction change, unload/return, regeneration and cancellation |
| Lighting | Sun-angle sweep, night, interior light, camera switches and ambient/reflection leakage |

Add focused Rust unit/integration tests and Unity editor/physics/rendering tests
for these behaviours. Use an isolated Unity validation checkout while the user's
editor is open. Run the repository's applicable Rust checks and Unity player
build when implementation changes native contracts or shaders. A successful build
does not substitute for interactive traversal and seam inspection.

Start profiling on a fixed corpus of 20 seeds spanning tall cliffs, low islands,
thin ridges and river mouths. Record accepted counts and rejection reasons as
well as timings; fast generation that rejects every useful entrance is not a
successful result.

Trial caps for the first version: one cave per island, 128 active meshing chunks,
250,000 render triangles and 128 MiB additional generation scratch. Measure the
cost of fixed-detail terrain reservations separately and set their area limit
from the entrance proof. These are guardrails to validate, not performance claims.
Use checked allocation arithmetic and deterministic bounds, not elapsed-time
cutoffs that could change generated output on a slower machine.

Track peak native/managed/GPU bytes, output/collider triangles, snapshot bytes,
generation time, upload time and collider-cook time. Follow the existing Unity
installation budget; a single collider cook can exceed it, so split collision
meshes if measured stalls require it. Discard density scratch after extraction
and limit parallel chunk jobs by memory as well as core count.

## 13. Expected file changes

This is a responsibility map, not a requirement to create an abstraction for
every row before it is needed.

| Location | Planned responsibility |
| --- | --- |
| `island-rs/src/caves.rs` and `caves/` | Reconcile the stub; options, candidate selection, layout, density, marching cubes, entrance patch, validation and focused tests |
| `island-rs/src/terrain/generation.rs` | Schedule cave work after final terrain; own cave results; apply placement exclusions |
| `island-rs/src/terrain/sampling.rs`, `mesh_clipper.rs` | Focused geometry queries and terrain cut/edge contracts |
| `island-rs/src/terrain/snapshot.rs` and legacy save/load | Persist cave results and validate versions/limits |
| `island-rs/src/ffi.rs` and native declarations | Cave options, bounded export and release contracts |
| `Assets/Scripts/Settings/IslandCaveSettings.cs` | Inspector/script configuration and validation |
| `Assets/Scripts/Islands/IslandGenerationProfile.cs`, request/factories/cache | Copy cave inputs and include them in cache identity |
| `Assets/Scripts/Interop/CavePreparation.cs`, `MotuNative.cs` | Native transfer, coordinate conversion and validation |
| `Assets/Scripts/Islands/IslandPreparedData.cs`, pipeline/installer/runtime | Prepared cave data, installation and ownership |
| `Assets/Scripts/Streaming/CaveStreamer.cs`, terrain streamer partials | Cave residency, terrain reservations and collider holes |
| `Assets/Scripts/World/IWorldSurfaceQuery.cs` and gameplay integration | Explicit underground placement semantics and debug traversal |
| `Assets/Shaders/` and island material factory | Cave rock/floor shading, correct passes and material lifetime |
| `Assets/Editor/` and `Assets/Tests/Editor/` | Candidate/geometry diagnostics, fixtures and runtime validation |

All new Unity types use the existing `Motu` namespaces and assembly boundaries.
Keep native algorithms out of Unity editor code and keep Unity object creation
out of generation workers. Leave unrelated local imports, recovery assets and
lockfile changes outside cave implementation commits.
