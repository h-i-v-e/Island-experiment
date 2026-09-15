# Navigation chunks

Navigation now bakes spatial chunks sequentially instead of uploading and baking
the entire island in one operation. The default chunk width is 128 metres, rounded
to whole NavMesh tiles. `ChunkSize` accepts 32–256 metres; `MaximumBuildWorkers`
accepts 1–4 and defaults to one. The existing global gate still serializes island
bakes. Agent radius remains configurable and defaults to 0.5 metres.

Each chunk uploads compact copies of nearby original LOD0 faces and nearby static
obstacle volumes. Source selection includes a border margin and submerged context;
only cells with potentially walkable ground are baked. Terrain is not simplified
or morphed to a visual LOD. Caves and river exclusions remain included. All chunks
use the original source's vertical bounds so voxel height origins stay consistent.
Source meshes are released after their asynchronous operation has completed, and
a frame is yielded before uploading another batch. Cancellation releases both
completed chunks and the current build after Unity finishes cancelling it.

## Height and joins

The height mesh stays enabled. Unity's `preserveTilesOutsideBounds` partial-update
mode disables it, so the implementation uses separate NavMeshData objects with
short bidirectional links at validated walkable border crossings. These API
constraints are described in [Unity's build settings source](https://github.com/Unity-Technologies/UnityCsReference/blob/master/Modules/AI/Public/NavMeshBuildSettings.bindings.cs)
and [NavMeshLink documentation](https://github.com/Unity-Technologies/NavMeshComponents/blob/master/Documentation/NavMeshLink.md).

Candidate crossings come from original walkable faces, including separate cave
levels. Nearby NavMesh endpoints and border raycasts reject blocked crossings.
Raycast height error is separated from horizontal obstruction because raycasts
follow simplified polygons, not the detailed height mesh. A redundant crossing is
removed only if both sides can walk directly to an existing crossing within eight
metres; this reduces unnecessary path-search nodes while preserving separate
passages. Temporary neighbour registrations are always removed before yielding.
Dormancy/unload removes all final links and data together. Automatic link traversal
must remain enabled on a normal NavMeshAgent, or be handled by the host.

`IsReady` still means the complete island bake and its joins have finished.
`Data` now returns the first chunk for compatibility, not a whole-island asset;
`ChunkCount` and `Report` expose progress and source allocation counters. Report
source counts/bytes are cumulative across uploads, while `peakChunkSourceMeshBytes`
and `peakChunkTriangles` describe the largest individual upload. Neither is a
complete accounting of Unity/native/managed memory.

## Measurement and verification

The retained cached island has 1,738,314 source triangles including caves. With
128-metre chunks it completes 95 bakes. The largest upload contains 58,305 triangles
and 1,501,428 bytes of vertex/index payload, compared with 32,106,048 bytes for the
historical monolithic source upload of the same snapshot. This is roughly a 21-fold
reduction in the largest source mesh payload, not a 21-fold reduction in process
RAM. The native library and user scene settings are unchanged.

The default Play Mode diagnostic records 13.44 seconds for the complete navigation
build, including 12.12 seconds inside asynchronous bakes; final registration took
13.34 ms. It retains 1,573 border links. Process RSS was sampled from a 927.99 MB
baseline to a 929.76 MB peak; Unity's separate allocation counter rose from 2.3824 GB
to 2.3856 GB. These are sampled process observations, not isolated allocation costs
or exact peaks. Snapshot loading/preparation occurs before the baseline. The
historical benchmark used a different run mode and process state, so no controlled
bake-speed or whole-world memory improvement is claimed.

The focused tests cover cross-border movement/height, boulders, river/cliff barriers,
stacked floors, settings validation, dormant registration, cancellation between
chunks and shared agent-type lifetime. Full development and fresh package/player
results are recorded under `navigation_chunks` in [validation.json](validation.json).

## Remaining long-distance query limitation

The retained comparison uses the same 256 source-point pairs for both builds.
Of 39 routes that the monolithic build reported complete, standard chunked path
queries and real NavMeshAgents report 37 complete and two partial. A separate
65,535-node query reaches the exact destination polygon for both remaining routes.
Thus those locations are connected, but standard path calls still have a material
long-distance limitation. The custom query is diagnostic only; it is not silently
installed as an agent controller. Two previously partial queries also become
complete. The diagnostic XML reports successful execution, not an assertion that
all path results are identical. Consumers must handle partial paths and should
validate their long-distance routing strategy on their own islands.

The chunked bake reduces a concrete navigation memory spike; it does not establish
that navigation caused the reported ten-second whole-computer lockups or that all
such lockups are resolved. Existing logs already had high allocation before the
largest navigation bake. Full-world residency, vegetation memory and sustained
travel still require a live profile. Individual source uploads and registration
remain atomic Unity calls, not hard real-time guarantees.
