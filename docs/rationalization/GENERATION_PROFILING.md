# Native generation stage profiling

Build outside the measured interval, then run on an otherwise idle machine:

```sh
cargo build --manifest-path island-rs/Cargo.toml --locked --no-default-features --features profiling --profile profiling --bin generation-bench
RAYON_NUM_THREADS=14 island-rs/target/profiling/generation-bench --terrain-size 512 --seed 17 --repetitions 3 > /tmp/generation-profile.csv 2> /tmp/generation-stages.log
python3 scripts/summarize-generation-profile.py /tmp/generation-profile.csv /tmp/generation-stages.log /tmp/generation-profile.json
```

`--terrain-size` is the number of initial triangulation seed points, not a mesh
width or final vertex count. Each process warms up once using at most 128 points.
The summarizer discards that generation, joins the remaining stage groups to the
CSV records, and rejects incomplete logs, mismatched durations and geometry hashes
that vary between repetitions. Keep the raw CSV and stderr with the summary.
Do not concatenate separate benchmark invocations into one input pair.

The `profiling` Cargo profile inherits release optimization, preserves debug
symbols and enables timers only when the separate `profiling` feature is set.
The normal native build has no timer storage or logging. These runs measure CPU
`Island::generate`, including the decoration placement and rock settling it now
performs before vegetation classification. The legacy timer name
`decorations.lazy` does not mean this work is deferred in the current generator.
The runs exclude material baking, decoration mesh export, snapshot loading,
Unity preparation/upload, navigation and rendering. Preserve
the default options and leave `MOTU_EXPERIMENTAL_MESH_FLOW` unset when comparing
with the accepted native baseline.

Stage durations are inclusive. For example, `hydraulic.stage` occurs inside the
LOD-generation stages, so adding both counts the same work twice. Repeated
instances of the same label are summed within each run before taking the median;
an absent stage contributes zero for that run. Ranking identifies where to look,
not an exclusive flame graph or a guarantee that stage percentages add to 100.
Uninstrumented work also remains between stages. Use a sampling profiler inside
the largest stage before deciding which operation to optimize.

Compare geometry hashes with [generation-baseline.json](generation-baseline.json)
and record machine, worker count, build command and source/binary hashes with each
captured result. A timing difference from a different build or load condition is
not evidence of an optimization. Keep the accepted sequential erosion path:
later paths observe earlier mutations, so reordering it can change terrain shape.

The parser's regression checks run with:

```sh
python3 -m unittest discover -s scripts/tests
```

## Captured stage ranking

[The captured report](generation-profile.json) records seed 17, 14 workers, three
measured runs per size, binary/source hashes, raw logs and process RSS. Both
fixtures match the geometry hashes from the earlier baseline. Median total
generation was 12.85 s at 512 points and 35.52 s at 1,024 points. No production
optimization was applied between those baselines; different build/load conditions
mean the lower totals are not a speedup claim. Desktop apps remained open.

Selected inclusive stage medians (seconds):

| Stage | 512 points | 1,024 points | Next investigation |
| --- | ---: | ---: | --- |
| Hydraulic erosion, seven passes | 3.94 | 16.23 | Sample reference-path normal reconstruction, downhill searches and face-safety checks; preserve sequential mutation order. |
| Final river construction | 2.44 | 6.94 | Attribute topology refinement, cross-section constraints and export before changing geometry or copies. |
| Rock settling | 2.21 | 5.77 | Profile contact queries and broad-phase candidates; preserve drop population, RNG order and settling behavior. |
| Projected foldover repair | 1.13 | 1.62 | Measure repair passes and incident-face allocation; preserve inversion protection. |
| LOD simplification and index | 0.52 | 1.15 | Lower priority than erosion/rivers/rocks; measure temporary position/normal copies before replacing them. |

Parent LOD-generation and decoration timers are omitted from this table because
they include some of the listed stages. They remain in the raw report. These
measurements rank native investigation priorities; they do not establish frame
rate, upload, collider, navigation or world-residency budgets.

The first measured follow-up is recorded in
[NATIVE_OPTIMIZATION.md](NATIVE_OPTIMIZATION.md): unused erosion geometry is
skipped, with paired timings and complete snapshot comparisons. Its new result
is separate from the unchanged-generator baseline above.
