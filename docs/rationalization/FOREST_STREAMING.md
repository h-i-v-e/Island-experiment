# Forest detail residency

Whole-island forest preparation previously exported both wood and foliage at
LOD2, LOD1 and LOD0 into managed arrays. `ForestTileStreamer` retained all six
grids, including full detail for cells that were never approached. Native forest
geometry remained resident too, and nearby tiles acquired additional Unity mesh
buffers. This was a concrete duplicate allocation separate from navigation.

Preparation now exports only LOD2 overview meshes and the compact trunk/log
collider records. Nearby LOD1 and LOD0 geometry is copied from the same native
forest on demand. One forest worker serves all islands; each request holds a
native lease. An LOD1 region contains 8 by 8 owner tiles and an LOD0 region one
owner tile. Native temporary exports are released per leaf. Managed detail arrays
are cleared after upload and are not kept for revisiting a region. The streamer
also stops retaining the overview's managed arrays after installation.

Forest transitions restore the coarse owner and retire obsolete detail before
allocating replacements. Incoming groups remain hidden until complete. Tile
uploads yield when the existing installation budget expires. Cancellation,
teleports and unload discard pending groups and cancel queued work, while a
running native read keeps its lease until it returns. Trunk capsules and fallen
log boxes still exist only in LOD0; log end-grain channels, foliage clusters,
canopy shadow proxies and the native geometry are unchanged.

The native region API includes both bounds. Leaf upper bounds use the previous
representable float at interior edges, so an anchor exactly on a grid line belongs
to only the next cell. The outside island edge still includes 1.0. Exporting one
leaf at a time avoids perturbing internal subdivision positions and keeps native
scratch buffers small. This uses the existing ABI and does not rebuild Rust.

## Measured payloads

`ForestStreamingBenchmark.RunBatch` loads the same cached large island used in the
navigation report, then measures every LOD0 and LOD1 region sequentially. It sums
mesh channel array payloads instead of recreating the old multi-gigabyte live
allocation. Raw results are in [large-island.json](forest-streaming/large-island.json).
All figures below are decimal MB/GB and exclude array headers, native geometry,
Unity/GPU buffers, allocator slack and other island features.

| Forest detail | Former whole-island managed payload | Largest temporary region | Source triangles |
| --- | ---: | ---: | ---: |
| LOD0 | 2,178.26 MB | 14.16 MB | 45,030,372 |
| LOD1 | 175.45 MB | 20.01 MB | 3,425,676 |
| Combined | **2.354 GB** | At most one completed region per active streamer request | 48,456,048 |

The former persistent detail payload is no longer retained. A forest worker can
prepare one region while another island uploads an already completed region;
the worker limit is not a single global cap on completed upload buffers. Active
Unity detail meshes and native forest geometry still consume memory.

Sequentially exporting every region took 4.34 s at LOD0 and 0.44 s at LOD1. Normal
travel only requests the nearby regions. The densest LOD0 region's two mesh
uploads totalled 12.26 ms; the largest individual upload was 10.23 ms. The densest
LOD1 region's 76 uploads totalled 14.06 ms, with a maximum of 0.95 ms. These are
one warm Editor fixture's mesh creation timings, not rendered frame percentiles,
collider cooking measurements, or a controlled old/new runtime speed comparison.

Snapshot loading alone took 9.67 s in this run, before forest export measurement.
It occurs on a worker but still allocates native data. The reported whole-computer
freeze was not captured. Removing these duplicate arrays is a substantial scoped
memory reduction, not proof that snapshot loading, native residency, material
uploads, navigation, or all four resident islands fit a desired RAM budget.

## Reproduction and validation

Use Unity 6000.5.6f1, Built-in, Metal, Apple Silicon, one validation process at a
time. Run the benchmark in an isolated development project with:

```sh
MOTU_FOREST_SNAPSHOT=/absolute/island.motusnapshot \
MOTU_FOREST_REPORT=/absolute/forest-report.json \
"$UNITY_EDITOR_EXE" -batchmode -force-metal -projectPath /absolute/project \
-executeMethod Motu.Editor.ForestStreamingBenchmark.RunBatch -logFile /absolute/forest.log
```

The benchmark reads the snapshot directly without trimming or rewriting its
cache. It does not require scene edits. Avoid committing complete Unity startup
logs, which can include licensing tokens.

Regression coverage includes exact region vertices, normals, indices, UVs,
material and environment arrays against the existing batch export, bounds at
every grid edge, cancelled worker requests, partial-upload cleanup, wireframe
toggles during upload, repeated redirects/clear, canopy materials/shadows and
fallen-log collider retirement. The native comparison uses bounded reference
regions so tests do not recreate the allocation being removed. Its legacy
inclusive outer-edge cells are excluded where ownership differs; explicit bound
checks cover the new half-open assignment.

The final development suite passes 141 tests, with zero failures and eight existing
external cave-fixture skips. All 12 fresh-package tests pass. Mono and IL2CPP
standalone players pass generation, navigation, detailed streaming while frames
advance, rendering, environment restoration and unload. The Mono player also
passes with navigation disabled. The source ledger has 66 current full reviews.

Final suite, player, artifact and review evidence is recorded under
`forest_streaming` in [validation.json](validation.json). Previous validation
entries remain historical. The full source audit, external cave fixtures,
world travel/RSS recovery and GPU/frame matrix remain open. Navigation's two
previously reported long-route query differences are unchanged by this work.
