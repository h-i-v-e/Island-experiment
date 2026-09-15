# Native erosion optimization

Reference erosion now omits normals and face-limit calculations when the existing
sea-plane rule or fully depositional slope already makes erosion zero. Sediment
transport, deposition, sub-epsilon soil cleanup and sequential path updates remain
intact. There is no new cache, allocation, public API or quality setting.

## Measurement

Three alternating before/after process pairs used seed 17, 1,024 initial points
and 14 workers on the Apple M4 Pro. Each process warmed up at 128 points before
one measured run. Both executables used identical profiling build options and
the same benchmark harness. Compiler/test workloads had finished; normal desktop
applications remained open.

| Median | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Complete native generation | 32.910 s | 32.097 s | 2.47% |
| Seven hydraulic passes combined | 14.611 s | 13.803 s | 5.53% |

All three pairs improved. This is a modest native CPU improvement for this
fixture, not a frame-rate claim or a guarantee for every seed. Unity upload,
navigation, rendering and world residency were outside the timed interval.
[comparison.json](hydraulic-optimization/comparison.json) retains each timing,
process RSS, source/binary hashes and snapshot checksums. Raw stage logs and the
sampled call graph are alongside it. Stage timings are inclusive.

To reproduce a pair after preserving old/new profiling executables:

```sh
RAYON_NUM_THREADS=14 /tmp/before-bench --terrain-size 1024 --seed 17 --repetitions 1 > /tmp/before.csv 2> /tmp/before.log
RAYON_NUM_THREADS=14 /tmp/after-bench --terrain-size 1024 --seed 17 --repetitions 1 > /tmp/after.csv 2> /tmp/after.log
python3 scripts/summarize-generation-profile.py /tmp/before.csv /tmp/before.log /tmp/before.json
python3 scripts/summarize-generation-profile.py /tmp/after.csv /tmp/after.log /tmp/after.json
```

Build commands are in [GENERATION_PROFILING.md](GENERATION_PROFILING.md). Leave
`MOTU_EXPERIMENTAL_MESH_FLOW` unset and do not run builds/tests during measurement.
Alternate pair order and repeat three times instead of comparing unrelated runs.

## Output and regression checks

The benchmark accepts `--snapshot-directory PATH`. Complete snapshots are saved
**after** timing, as `SEED-RUN.motusnapshot`; use separate directories for each
build and size. These include material/environment fields, normals/UVs, coarser
LODs, rivers, rocks, vegetation and spatial indices. Snapshot compression and
writing are excluded from reported generation time; do not use snapshot-export
runs as the paired timing experiment.

All five 128-point fixtures (seeds 17, 666, 2018, 42 and 12345), and seed 17 at
1,024 points, produced identical complete snapshot SHA-256 values before/after.
The default-size snapshot is approximately 864 MiB. The checksums are retained;
large temporary snapshots are not package contents.

A focused test compares all mutated step state as bytes against the old step
calculation across 144 combinations of sea level, slope, sediment, speed and soil
depth, including signed-zero sea levels and sub-epsilon soil. All 16 hydraulic
debug tests pass. The complete release CPU suite passes 469 tests, with zero
failures and three existing ignored tests. Tests use `panic=unwind`; the shipped
library retains its normal `panic=abort` release build.

The release run exposed a pre-existing forest assertion differing by two ULPs
between a constant-folded expected expression and runtime evaluation. An isolated
pre-change checkout reproduced it. Opaque runtime inputs now prevent folding of
the reference; its exact bit assertion, boundary and monotonicity checks remain
intact. No forest generation behavior or tolerance changed. Both debug and
release forms pass; baseline/fixed logs are retained.

The native plugin was rebuilt for the runtime package. All 134 development Unity
tests passed (eight external cave-fixture skips), as did eight fresh package
checks. Both Mono and IL2CPP players passed generation, rendering, navigation
and environment teardown. Results and exact package hashes are recorded under
`hydraulic_optimization` in [validation.json](validation.json).

Restart an already-open Unity Editor to load the rebuilt native library.
Further native profiling, the full review ledger, GPU/frame and world-residency
work remain open.
