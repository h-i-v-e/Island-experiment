# Rock settling optimization

CPU rock settling reuses its last terrain height and normal when a body's XY
position is bit-identical. The immutable terrain and fixed body order make those
samples valid for the simulation. Horizontal movement, including movement from a
rock collision, performs a fresh query. Gravity, contact response, pair ordering,
sleep/wake rules, spawn density and random sequences retain their existing rules.

One optional XY/height/normal record per body is allocated with the solver and
released when it returns. The cache is private to the CPU solver: it does not
change the native ABI, snapshot schema, exported geometry, Unity settings or GPU
solver. Contact response still runs on cache hits, including sleeping bodies.
There is no approximate position tolerance or persistent island cache.

## Evidence

A three-second sample during the default-size rock stage showed terrain surface
queries dominating its main-thread stacks; the original call graph is retained
in [before-sample.txt](rock-optimization/before-sample.txt). This motivated reusing
identical queries before changing source density or collision algorithms.

Three alternating process pairs used seed 17, 1,024 initial points and 14 workers
on the Apple M4 Pro. Each process warmed up at 128 points, then generated one
measured island. No builds or tests ran during these measurements; normal desktop
applications remained open. All three pairs improved.

| Median | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Complete native generation | 32.475 s | 30.998 s | 4.55% |
| Rock settling, including spawn/export | 5.071 s | 3.427 s | 32.42% |

This compares against the already optimized erosion implementation. It measures
native CPU generation for one fixture; it does not establish Unity FPS, GPU or
whole-world memory savings. The cache adds bounded temporary memory proportional
to the number of bodies. No process-RSS reduction is claimed.

[comparison.json](rock-optimization/comparison.json) records individual timings,
source and executable hashes, and complete snapshot comparisons. The neighboring
CSV files and stage logs retain raw measurements. Reproduction uses the profiling
build and warmup rules in [GENERATION_PROFILING.md](GENERATION_PROFILING.md).

## Regression and integration checks

The focused decoration suite passes eleven debug tests. Its new regression test
runs both the original solver loop and cached solver on flat, gentle and steep
terrain, including overlaps, a sleeping support hit by a falling rock, horizontal
movement and nearest-vertex fallback. Final positions and velocities are compared
as floating-point bits; the remaining body state is also compared exactly.

All six complete snapshots are byte-identical: five 128-point seeds (17, 666,
2018, 42 and 12345), plus seed 17 at 1,024 points. The complete release CPU suite
passes 470 tests with zero failures and three existing ignored tests; all 11
focused debug decoration tests and CPU Clippy with warnings denied pass. Release
tests use `panic=unwind`; the shipped native library retains `panic=abort`.

The rebuilt plugin passes 134 development Unity tests (eight external cave
fixtures skipped), the development player build and all eight fresh-consumer
checks. Both Mono and IL2CPP players pass native generation, rendering,
full-resolution navigation, environment disable/re-enable and additive unload.
Restart an already-open Unity Editor to load the rebuilt native library.

Complete snapshot and packaged player validation results are recorded in
`rock_optimization` in [validation.json](validation.json). Snapshot exports run
separately from timing. Baseline hashes come from the preceding erosion milestone;
its snapshot-producing executable was verified byte-identical to this milestone's
before executable. No visual-output equivalence is inferred from geometry hashes
alone.

Full-code audit closure, river/active-erosion profiling, GPU/frame measurements,
streamed-world residency and the outstanding distribution gates remain open.
