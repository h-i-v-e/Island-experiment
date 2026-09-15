# Whole-island navigation — 2026-09-15

## Use

Navigation is enabled by default. In `OpenSeaWorld`, select the object with
`CoherentIslandFactory` and expand **Navigation (reload islands to apply)**.
The grid request factory exposes the same settings. Defaults:

- Radius: **0.5 m**; height: **2 m**.
- Step height: **0.4 m**; maximum slope: **45 degrees**.
- Voxel size: **0** selects radius/3, or **0.1667 m** at the default radius.
- Tile size: **128 voxels**; height mesh: always enabled.
- Minimum local ground height: **0 m** (sea level).
- Exclude river beds: enabled. This first land-walking implementation excludes
  river-bed faces using the generated material mask; it does not model wading.

Reload islands to apply settings; the native island cache need not be cleared.
`Motu/Navigation/Benchmark Cached Island` accepts a native snapshot and writes a
JSON report, using the scene factory settings if present.

`IslandRuntime.Navigation` exposes `IsReady`, `IsRegistered`, `BuildCompletion`,
`Report`, `Data`, and `AgentTypeId`. Navigation builds after visible terrain
installation; NPC creation must wait for readiness and registration. Configure
an inactive NPC's `NavMeshAgent` with `navigation.ConfigureAgent(agent)`, place
it at a valid sampled point using that agent type, then activate it. This change
does not spawn NPCs or extend player-focused physics streaming to NPCs.

## Implementation

AI Navigation 2.0.14 and the AI engine module are installed. The native full LOD0
render export uses no tile bounds and no LOD morphing, retains cave entrance cuts
and the existing ten-metre underwater cutoff, and is copied on the preparation
worker. Navigation copies retain positions, indices and river material values;
render normals, UVs and environment attributes are omitted. All terrain faces
remain bake input; dry and river-bed faces get separate area assignments.

The build includes prepared cave geometry and non-walkable bounding volumes for
trunks, fallen logs and boulders. These conservative obstacle bounds are
independent of visible LODs and collider GameObject availability. Grass, leaves,
reeds and ferns are excluded.

Builds are serialized across islands to limit peak memory pressure and use
`NavMeshBuilder.UpdateNavMeshDataAsync`. Source Unity meshes are retained until
the native operation completes, including cancellation. Dormant islands remove
their data from the navigation world and retain it for reactivation; unloading
cancels pending work and releases navigation data, sources and shared agent-type
references. Visual LOD changes do not rebuild or unregister navigation. Agent
types are shared by islands with matching radius, height, slope and step size.

## Full-island measurement

See `benchmark.json`. Unity 6000.5.6f1, macOS, isolated editor in batch mode.
The input is a recent large cached island, not a synthetic flat terrain:
908,764 terrain vertices and 1,726,010 terrain triangles, with caves bringing
the bake total to **1,738,314 triangles** and **9,441 sources**.

- Snapshot read and navigation export: **11.01 s**.
- Unity source upload: **0.118 s**.
- Asynchronous bake: **32.05 s**.
- Source position/index buffers: **30.6 MiB** (excludes preparation buffers).
- Sampled process RSS: **1.41 GiB baseline**, **1.68 GiB peak**; rise **276 MiB**.
- Unity allocated-memory sampled rise: **307 MiB**.
- Unity reports **491,064 bytes** for the NavMeshData object. This is an API
  counter, not a demonstrated complete accounting of every native allocation.
- 128 path queries: **0.595 ms mean**, **1.095 ms maximum**; 22 complete,
  106 partial, zero invalid. This run does not distinguish disconnected
  walkable regions from query limitations; these counts do not establish
  whole-island connectivity. River-bed exclusion also prevents ordinary wading.
- 893 NavMesh height-query comparisons: **0.119 m mean**, **0.241 m maximum**.
  72 samples missed walkable space; 64 moved outside the original source face
  and were excluded from height-error measurement. Comparison uses the original
  LOD0 triangle at the returned XZ, avoiding a slope/horizontal-offset error.

RSS and Unity allocation peaks are sampled once per editor update during the
source upload/bake, not exact allocation peaks or isolated navigation costs;
the snapshot-loading peak is outside that sampling interval. This benchmark
measures navigation queries, not the rendered feet of characters. A separate
Play Mode moving-agent test exercises height-mesh placement on uneven ground.

A whole-island bake is viable for this fixture but takes tens of seconds. Several
islands queue their builds, so navigation readiness can lag terrain residency.
No frame-time or large NPC crowd benchmark, live-scene visual QA, or shipping
player benchmark has been performed.

## Verification

Navigation tests cover configured dimensions and request copies, obstacle and
river sources, real async builds, pathfinding around obstacles, dormant/active
registration, cancellation, shared agent-type ownership, disposal during build,
background scheduling and unloading before a scheduled build starts. A Play Mode
agent traversed uneven terrain with **0.0615 m maximum vertical error** in that
synthetic fixture; this is distinct from the cached island's height-query results.

An initial no-graphics run passed the navigation tests but failed six existing
rendering validations that require a graphics device. The final run uses Metal.

Final Metal run: **32 passed, 1 failed**, including **all 8 navigation tests
passed**, plus the boulder, fallen-log and navigation-related runtime lifecycle
checks. `unity-tests.xml` contains the full result. The unrelated `ShoreBreaking`
validation still expects authored breaking depths of 5/4 metres, while the
existing checked-in OceanWaveProfile uses 10/2 metres. Neither that assertion
nor the wave profile was changed for navigation. `git diff --check` passed.

The moving-agent fixture uses Unity Play Mode, but the full cached-island bake
and path/height-query measurement use an isolated no-graphics editor. These
measurements do not establish exact visual foot placement on every terrain LOD.
