# Cave performance checkpoint — 9 September 2026

This continues phase 6 of `ISLAND_CAVES_PLAN.md` after checkpoint `bbd671c`.
The entrance, traversal and torch implementation is preserved. These changes add
measurement and a repeatable corpus; they do not alter geometry, saved cave data,
settings defaults, cache identity or the native ABI.

## Conditions

Apple M4 Pro, 24 GiB RAM, macOS; Rust release build, CPU generation, profiling
feature enabled. Cave algorithm revision 7, default cave options with caves enabled,
default island/vegetation options. Twenty consecutive seeds (0–19), terrain size
128, plus seed 5 at terrain size 1024. Each island spans 2 km. The corpus is a
repeatable smoke/performance sample, not a classified cliff/ridge/river-mouth
coverage set or a measurement of the coherent world's cave frequency.

Runs were sequential, without the validation Unity process running concurrently.
The user's editor remained open. Timings are local observations, not frame budgets
or target-hardware guarantees. Native timings are nested: do not sum parent and
child stages. Child totals include rejected candidates as well as accepted caves.

## Native results

17 of the 20 small islands accepted one cave. Seeds 1, 6 and 16 accepted none.
All generation and bounded cave validation completed successfully.

| Measurement | Median | Maximum |
| --- | ---: | ---: |
| Whole island generation, size 128 | 2.93 s | 3.50 s |
| Cave generation, all 20 islands | 68.41 ms | 417.41 ms |
| Cave surface sampling | 3.22 ms | 22.44 ms |
| Final rock-cover checks | 10.26 ms | 39.12 ms |
| Entrance collider generation | 4.76 ms | 9.52 ms |
| Passage mesh generation | 0.72 ms | 1.45 ms |
| Rendered passage triangles, accepted caves | 11,334 | 13,509 |
| Collider triangles including entrance, accepted caves | 13,705 | 15,765 |
| Native mesh array payload, accepted caves | 559,672 bytes | 642,540 bytes |
| Serialized cave data, accepted caves | 599,740 bytes | 692,856 bytes |

The size-1024 island took 35.77 s overall; caves accounted for 253.07 ms. It
accepted one cave with 6,694 rendered passage triangles, 9,432 collider triangles,
478,176 bytes of native mesh array payload and 493,668 serialized cave bytes.
Its **whole-process** peak memory footprint was 3.38 GiB; this includes terrain,
vegetation and all other generation work and is not cave-only scratch usage.

`corpus.json` and `full-resolution.json` retain individual measurements and
rejection counts. The selected geometry is checked by the generator's existing
approach, cover, capsule and resource rules. The corpus run does not independently
audit every shared edge or visually inspect every entrance.

## Unity measurements

The `Caves` Inspector now shows native mesh export, managed copy/validation,
Unity mesh creation, total collider creation/cooking and the longest individual
collider creation. It also reports mesh count, rendered/collider triangles and
prepared array payload. Timings exclude frame-budget waits and reset on disposal.
Profiler markers are `Motu.Caves.NativeExport`, `Motu.Caves.CopyMesh`,
`Motu.Caves.CreateMesh` and `Motu.Caves.CookCollider`.

Mesh creation measures CPU-side Unity API work, not GPU submission/completion.
Collider timing includes `AddComponent<MeshCollider>` (which can cook the
existing MeshFilter immediately), configuration and `sharedMesh` assignment. Memory counts
are array element bytes, excluding object headers, spare native vector capacity,
Unity/GPU copies and physics allocations. Peak cave scratch and GPU residency
remain unmeasured.

Unity 6000.5.6f1, Metal, isolated EditMode project; restored seed 5, terrain size
128. The seven cave/torch tests passed. The restored cave retained 2 meshes,
12,492 rendered triangles and 14,865 collider triangles. Its prepared array
payload was 1,024,204 bytes (0.98 MiB).

| Installation measurement | Observed time |
| --- | ---: |
| Native mesh export | 6.040 ms |
| Managed copy and validation | 1.693 ms |
| Unity mesh creation | 0.682 ms |
| Collider creation/cooking, total | 2.421 ms |
| Longest individual collider creation/cook | 1.983 ms |

This sample does not justify splitting colliders: its largest cook is below the
existing 4 ms default installation budget. Export/copy runs in background
preparation. Measurements remain available in the Inspector and Profiler for
larger authored caves; a sample does not guarantee every configuration fits.

## Reproduce

From the repository root:

```sh
cargo build --manifest-path island-rs/Cargo.toml --release \
  --no-default-features --features profiling --bin island-cave-probe
island-rs/target/release/island-cave-probe --seed 0 --count 20 \
  --terrain-size 128 > /tmp/cave-corpus.jsonl 2> /tmp/cave-stages.csv
/usr/bin/time -l island-rs/target/release/island-cave-probe --seed 5 \
  --terrain-size 1024 > /tmp/cave-full.jsonl 2> /tmp/cave-full-stages.log
```

Each `cave-probe,seed,N` line starts the profiling records for one island.
Aggregate repeated `profile,caves.*,milliseconds` records within that seed to
obtain the per-island child totals in the checked-in JSON. The JSON output reports
payload/serialized sizes even without writing snapshots. `--output DIRECTORY`
additionally saves full island snapshots and reports their bytes and save time;
these files include all terrain and vegetation and can be hundreds of megabytes.

For Unity, generate one snapshot as described in `ISLAND_CAVES.md`, then run
`Motu.Editor.CaveTests` with `MOTU_CAVE_FIXTURE_OUTPUT` and `MOTU_CAVE_SNAPSHOT` in
an isolated validation project. `GeneratedCaveSurvivesNativeSnapshotAndUnityCollision`
logs a `CAVE_PERFORMANCE` row and verifies that resource counters clear on unload.
The camera torch render test is in `Motu.Editor.PlayerTorchTests`.

## Validation

- Checkpoint before this stage: 389 native library tests passed (3 existing
  ignored), and 23 Unity cave/torch/ship/runtime-contract tests passed.
- Instrumented stage: all 20 corpus islands plus the size-1024 case generated
  successfully; seven focused Unity cave/torch tests passed with real fixture
  and snapshot inputs, including traversal and disposal checks.
- Strict production Rust Clippy passed with profiling enabled and disabled. The standalone
  macOS development player build passed in the isolated Unity project.
- No live-editor restart or unsaved scene write was performed.

## Acceptance boundary

The next visual acceptance work is a representative daylight/night/reflection
sweep in the running world, including sustained travel and repeated island
unload/return. Existing close-seam and torch pixel tests cover focused cases;
they do not replace that sweep. Branching networks remain a separate extension.
