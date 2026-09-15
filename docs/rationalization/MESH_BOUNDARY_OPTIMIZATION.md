# Mesh boundary optimization

`Mesh::perimeter_mask` now groups undirected edges by their lower vertex index,
then sorts the other endpoints within each group. Detailed terrain has many
vertices with small edge groups; this avoids sorting millions of packed 64-bit
edges together. A three-second sample during final river construction identified
repeated perimeter sorts as a substantial cost. Its original call graph is in
[before-sample.txt](mesh-boundary-optimization/before-sample.txt).

The input mesh is borrowed and unchanged. Exactly one occurrence of an
undirected edge marks both endpoints as perimeter, preserving the original
rules for holes, winding, duplicate/degenerate faces and non-manifold incidence.
The mask remains indexed by the original vertex order. No topology cache, float
calculation, public API, snapshot schema or native ABI changed.

## Ownership and cost

Offsets, scatter cursors and 32-bit endpoint storage are temporary query buffers;
only the resulting mask escapes. The offset copy deliberately gives scattering
independent mutable cursors while preserving the ranges used by local sorting.
This uses the same storage pattern as the existing adjacency builder.

For V vertices and E complete triangle-edge occurrences, temporary array payload
on this 64-bit target is approximately `16V + 4E + 8` bytes, versus the previous
`8E` edge array, excluding the result mask and allocator overhead. Empty meshes
return directly. Sparse meshes with many isolated vertices can use more temporary
storage, and high-valence groups retain a sorting cost. This is a measured change
for generated terrain, not a claim of better memory use for every possible mesh.

## Measurement

Three alternating before/after process pairs used seed 17, 1,024 initial points
and 14 workers on an Apple M4 Pro. Each process warmed up at 128 points before
one measured generation. Builds and tests were stopped during timing; normal
desktop applications remained open. All three pairs improved.

| Median | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Complete native generation | 30.819 s | 27.352 s | 11.25% |
| Final river construction | 6.809 s | 4.106 s | 39.70% |

This compares against the preceding rock-settling optimization. Perimeter queries
also occur elsewhere in generation, so total savings are not attributed only to
rivers. Stage durations are inclusive. Native CPU generation excludes Unity
upload, navigation, rendering and whole-world residency; no FPS/GPU improvement
is inferred. Individual totals vary with normal desktop load.

[comparison.json](mesh-boundary-optimization/comparison.json) retains raw timing
references, source/executable hashes, process peak RSS and complete snapshot
comparisons. RSS is recorded as process telemetry, not proof of a global memory
reduction. Reproduction follows the build/warmup rules in
[GENERATION_PROFILING.md](GENERATION_PROFILING.md), with `/usr/bin/time -l` wrapped
around each executable to record process resources.

## Validation

An independent map of edge-use counts checks all 19,683 three-face combinations
over three vertices, including reversed/duplicate triangles, self-edges and
non-manifold counts, with a fourth isolated vertex. A separate ring/disconnected
fixture explicitly checks inner/outer boundaries, isolated vertices and empty
meshes. All four focused debug perimeter-related tests pass.

All six complete snapshots match the prior implementation byte-for-byte: seeds
17, 666, 2018, 42 and 12345 at 128 initial points, plus seed 17 at 1,024 points.
The complete release CPU suite passes 472 tests with zero failures and three
existing ignored tests. CPU Clippy with warnings denied and formatting pass.
Release tests use `panic=unwind`; the shipped plugin retains `panic=abort`.

The rebuilt plugin passes 134 development Unity tests (eight external cave
fixtures skipped), the development player build and eight fresh-consumer tests.
Both Mono and IL2CPP players pass generation, rendering, full-resolution
navigation, environment disable/re-enable and additive unload. Restart an
already-open Unity Editor to load the rebuilt native library.

Complete snapshot and rebuilt-package validation results are recorded under
`mesh_boundary_optimization` in [validation.json](validation.json). Snapshot
exports are separate from timing. The preceding rock milestone's snapshot
producer was verified byte-identical to this before executable before reusing
its retained hashes. Newly generated scratch snapshots are removed only after
checksum equality succeeds; the checksums remain in the comparison record.

The mesh review is partial: adjacency/perimeter helpers were inspected, while
remaining tessellation, slicing and repair review stays open. Earlier seam and
scene edits are preserved. Full audit closure, active-erosion/Unity preparation
profiling, GPU/frame and world-residency measurements, and distribution gates
remain outstanding.
