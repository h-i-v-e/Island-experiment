# Avoid geometry work for hydraulic steps that cannot erode

The reference erosion path now calculates normals and projected-face limits only
when a step can remove material. Existing sea-plane protection makes removal zero
at or below sea level; the existing deposition weighting also makes it zero on
fully depositional slopes. Speed, path order, sediment exchange, deposition and
small-soil cleanup still execute. This adds no cached state, allocations or public
API changes, and retains the sequential terrain mutations.

Sampling the final hydraulic pass showed time in normal/geometry processing and
projected-face safety checks. The optimization avoids unused inputs to those
calculations; it does not reduce erosion strength, quality, simulation steps or
the safety limits themselves. See the captured call graph and raw stage logs in
[hydraulic-optimization](hydraulic-optimization/).

## Measurement

Three before/after process pairs used seed 17, 1,024 initial points and 14 workers
on the Apple M4 Pro. Pair order alternated before/after, after/before, before/after.
Each process performed one 128-point warmup and one measured generation. Compiler
and test workloads had finished; normal desktop applications remained open.
Both binaries used the same profiling build options and benchmark harness.

| Median | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Complete native generation | 32.910 s | 32.097 s | 2.47% |
| Seven hydraulic passes combined | 14.611 s | 13.803 s | 5.53% |

All three pairs improved. This is a modest native CPU improvement for this
fixture, not a frame-rate claim or a guarantee for every seed. Unity upload,
navigation, rendering and world residency were outside the timed interval.
[comparison.json](hydraulic-optimization/comparison.json) retains each timing,
process RSS, source/binary hashes and snapshot checksums.

To reproduce a pair after preserving the old and new profiling executables:

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

The benchmark now accepts `--snapshot-directory PATH`. Snapshots are saved **after**
the timed interval as `SEED-RUN.motusnapshot`; use separate directories for each
build and terrain size. This lets comparisons cover the entire saved island,
including material/environment fields, normals/UVs, coarser LODs, rivers, rocks,
vegetation and spatial indices, rather than only the terrain geometry hash.
Snapshot compression and writing are excluded from reported generation time;
do not use snapshot-export runs as the paired timing experiment.

All five 128-point fixtures (seeds 17, 666, 2018, 42 and 12345), and seed 17 at
1,024 points, produced identical complete snapshot SHA-256 values before/after.
The default-size snapshot is approximately 864 MiB. The checksum manifest is
retained; these large temporary snapshots are not package contents.

A focused test compares all mutated step state as bytes against the old step
calculation across 144 combinations of sea level, slope, sediment, speed and soil
depth, including signed zero sea levels and sub-epsilon soil. All 16 hydraulic
debug tests pass. The complete release CPU suite passes 469 tests, with zero
failures and three existing ignored tests. Release tests use `panic=unwind`;
the shipped library retains its normal `panic=abort` release build.

The broader release run exposed a pre-existing forest assertion differing by two
float values between a constant-folded expected expression and runtime evaluation.
An isolated pre-change checkout reproduced it. The test now supplies opaque
runtime inputs to both calculations; its exact bit comparison, boundary and
monotonicity checks remain intact. No forest generation behavior or tolerance was
changed. Both debug and release forms pass; baseline/fixed logs are retained.

The native plugin was rebuilt for the runtime package. Unity and consumer-player
results, exact package hashes and remaining cave-fixture skips are recorded under
`hydraulic_optimization` in [validation.json](validation.json).

Further optimization still needs measurements within active erosion steps,
river refinement and rock settling. The full codebase audit and Unity GPU,
navigation, upload and long-run residency matrix remain open.
