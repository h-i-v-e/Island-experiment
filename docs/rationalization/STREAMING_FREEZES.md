# Travel streaming stalls

The movement path synchronously built the initial collider, vegetation and
terrain neighborhoods. Even later incremental transitions called the Rust grid
slicer, copied the whole grid and uploaded all its meshes before yielding. Moving
native generation off-thread did not remove these separate main-thread exports.

`SetPlayerPosition` now uses the incremental transition for first focus too.
`TerrainGridWorker` exports and copies terrain on a worker, with one active grid
export across all islands. Unity mesh/object creation stays on the main thread
and yields between uploads when the configured installation budget expires.
Existing/coarse terrain remains visible until a complete neighborhood is ready.
The player's critical collider remains immediate.

The native island owner now supports leases. An export retains its lease through
native reads and managed copies; unload cancels pending work without waiting for
a worker. The final lease releases the island. Cancelling cannot interrupt the
Rust call itself, but prevents its unused results being installed. Export buffers
are released in `finally`. Teleports discard stale replacements. Mesh equations,
LOD seam rules, scene settings and the native plugin are unchanged.

## Evidence and limits

A Play Mode fixture uses seed 17, height 400 m, water ratio 0.85, one forest
prototype, 16-pixel material maps and navigation disabled. The old package's
first-focus call synchronously installed nine LOD1 and nine LOD0 groups and took
284.123 ms. Its retained fixture and output are in
[streaming-freezes](streaming-freezes/).

On the revised full-development test run, first focus returned in 9.431 ms;
terrain groups were still pending and the critical collider was present. All
nine LOD1 and nine LOD0 groups subsequently installed. The test resumed 1,743
times during the transition; its longest observed yield interval was 7.237 ms.
The fresh-consumer run recorded 9.980 ms initial focus. These are single-fixture
observations, not a controlled FPS benchmark: other validation builds/imports
were active, and coroutine resume intervals are not GPU/frame percentiles.
The test compares synchronous and worker grid arrays exactly, including
vertices, triangles, normals, UVs, material and environment channels. It also
checks redirect/unload and reference-counted native release.

The full development suite passes 135 tests with zero failures and eight
existing external cave-fixture skips, plus its player build. The fresh tarball
consumer passes nine tests. Standalone backend results and package hashes are
recorded under `streaming_freezes` in [validation.json](validation.json).
The player fixture now requests detailed terrain, waits for nine active groups
at both LODs and verifies that actual rendered-player frames advanced.

This is a confirmed source of stalls, not proof that all prolonged freezes are
resolved. A three-second sample of the user's editor captured an idle event loop,
not an actual freeze. The existing navigation log includes a 2.93-million-triangle
island, 164.9 ms source upload and 66.0 seconds asynchronous baking. Before that
bake, Unity already reported 24.35 GB allocated; its sampled peak was 24.99 GB.
Process RSS was separately reported as 4.71 GB baseline and 5.43 GB peak. Those
metrics are not interchangeable, and the high allocation cannot all be attributed
to this bake. The retained navigation records are supporting telemetry, not a
measured cause of the user's particular freeze.

Single Unity uploads, mesh combining, collider cooking and feature-group
creation remain atomic. Dense forests, full-world residency, sustained travel,
memory pressure and GPU/frame percentiles still need a live-world profile. The
scene's four-island residency and visual settings have not been reduced. This
change does not rebuild the native library or complete the broader audit plan.
