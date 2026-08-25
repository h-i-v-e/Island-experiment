# GPU Erosion Prototype

## Status

Prototype work began on 2026-08-25 on branch
`codex/gpu-erosion-prototype`. The default generator remains on the accepted
sequential hydraulic path.

The first slice was a CPU fidelity oracle and drainage-decomposition planner
for Confluence-Compacted Causal Erosion. That investigation established that
exact source ordering exposes too little parallel work. The active direction is
now Batch-Synchronous Particle Erosion: a deliberately non-byte-compatible,
fully GPU-shaped model whose acceptance criterion is convincing natural
morphology.

Enable it with:

```sh
MOTU_EXPERIMENTAL_CAUSAL_EROSION=1 cargo run --release --bin generation-bench -- \
  --terrain-size 65 --repetitions 3 --seed 666
```

Optional controls are:

- `MOTU_CAUSAL_EROSION_EPOCHS`, default `64`;
- `MOTU_CAUSAL_EROSION_CORE_SOURCES`, default `64`;
- `MOTU_CAUSAL_EROSION_STATS=1` for per-stage planner statistics; and
- `MOTU_CAUSAL_EROSION_REORDER_COMPONENTS=1` to execute the proposed
  component/core packet schedule on the CPU;
- `MOTU_CAUSAL_EROSION_COHORT_SOURCES` to cap the number of original source
  ranks reordered before packets are replayed;
- `MOTU_CAUSAL_EROSION_FRONTIERS=1` to stop a cohort whenever the next source
  belongs to a component that conflicts with the active frontier;
- `MOTU_CAUSAL_EROSION_PLAN_PNG=/absolute/path.png` to render the final
  partition (`MOTU_CAUSAL_EROSION_PLAN_SIZE` defaults to `1024`); and
- `MOTU_CAUSAL_EROSION_FROZEN_ROUTING=1` for the rejected frozen-route
  experiment.

## Implemented slice

For each rank epoch, the planner:

1. builds the reference model's lowest-downhill-neighbour map in parallel;
2. accumulates exact upstream source counts in descending height order;
3. marks vertices at or above the configured confluence threshold and its
   one-ring halo;
4. assigns every remaining vertex to a stable, bounded drainage component;
5. builds the component conflict graph from shared terrain triangles; and
6. greedily assigns deterministic colors so components of the same color never
   write vertices belonging to the same triangle.

The live-routing control continues to replay every source path through the
accepted exchange law. This produces exact reference geometry and material
while measuring the prospective GPU partition.

The optional reordered emulator schedules sources by `(component color,
component id, source rank)`. Each work item retains live downhill routing and
emits a compact packet `(source rank, vertex, speed, sediment)` when it reaches
the confluence core or another component. Packets are then replayed through the
shared core in original source-rank order.

The stricter causal-frontier mode scans sources in reference order and admits
the next source only when its component has no triangle conflict with the
active frontier. A conflicting component or a source already in the core ends
the frontier and forces ordered packet replay. This preserves much more of the
reference's feedback while still identifying groups whose upland work can
commute exactly. Both modes are CPU emulators of prospective GPU scheduling,
not GPU backends yet.

## First evidence

Seed 666 with 65 input points, 12 Rayon workers, one warmed release run:

| Mode | Time | Final vertices | Rivers | Geometry hash |
| --- | ---: | ---: | ---: | ---: |
| Reference | 915 ms | 111,460 | 16 | 13381516273005513489 |
| Live-routing planner | 1,109 ms | 111,460 | 16 | 13381516273005513489 |
| Frozen routing, 64 epochs | 1,211 ms | 98,455 | 7 | 10378529741592243520 |
| Frozen routing, 256 epochs | 1,715 ms | 136,623 | 19 | 10354459750947435786 |

Frozen routing is therefore rejected: increasing the epoch count did not
converge monotonically, and even the smaller test changed adaptive tessellation
and river topology materially. The GPU design must retain live path routing in
the uplands.

At the final live-routing hydraulic stage of a 128-point seed-666 run, a
64-source core threshold reported:

- 122,611 active source paths;
- 13,311 bounded upland components;
- no component exceeding 63 contributing sources; and
- 38,791 core/halo vertices;
- 28,101 component conflicts; and
- only 7 conflict-free color waves.

Promoting every cross-component triangle to the core was also tested and
rejected: it expanded the final core to roughly 95% of active land. Coloring
the sparse conflict graph preserves the bounded workgroups while requiring
only seven serial dispatch waves for this case.

## Reordered schedule evidence

The first actual schedule emulation retains the island's major ridges and
drainage regions, but is not yet morphology-equivalent to the reference:

| Seed 666, 128 points | Rank bands | Final vertices | Rivers |
| --- | ---: | ---: | ---: |
| Reference / exact live-routing control | exact order | 357,312 | 34 |
| Component/core emulator | 64 | 284,046 | 24 |
| Component/core emulator | 256 | 293,454 | 29 |

Narrowing the routing epochs initially recovered adaptive terrain detail and
rivers, but subsequent cohort sweeps were not monotonic. A one-source cohort
does reproduce the reference exactly, proving that splitting and resuming a
path is lossless; reordering two or more interacting sources is what causes the
drift.

## Causal-frontier evidence

For seed 666, source-conflict frontiers plus a smaller core substantially
improved the result:

| Seed 666, 128 points | Core threshold | Final vertices | Rivers |
| --- | ---: | ---: | ---: |
| Reference | exact order | 357,312 | 34 |
| Causal frontier | 64 | 340,203 | 30 |
| Causal frontier | 128 | 308,283 | 31 |
| Causal frontier | 256 | 351,645 | 34 |
| Causal frontier | 512 | 305,104 | 31 |

The 256-source result retains 98.4% of the reference adaptive vertex count and
all 34 rivers, although the coastline and individual river courses still
differ visibly. Its final hydraulic stage schedules 121,663 paths as 40,384
frontiers: about 3.0 sources per frontier on average, with a maximum of 43.
That is too narrow for a straightforward sequence of GPU dispatches to be an
obvious speedup.

The same configuration is not robust across seeds:

| Seed | Reference vertices/rivers | Frontier vertices/rivers |
| ---: | ---: | ---: |
| 42 | 239,298 / 19 | 188,947 / 16 |
| 1,337 | 281,069 / 19 | 296,439 / 26 |
| 2,026 | 285,090 / 32 | 281,076 / 25 |

The adaptive tessellation and later river extraction amplify small hydraulic
ordering changes. Vertex and river counts therefore cannot be used alone as a
fidelity gate, and the current schedule remains experimental.

At the requested production-scale 1,024-point size, seed 666 completed without
resource failure:

| Mode | Wall time | Final vertices | Rivers |
| --- | ---: | ---: | ---: |
| Reference | 81.41 s | 1,670,192 | 28 |
| Causal frontier, threshold 256 | 55.10 s | 1,600,595 | 25 |

The frontier retained the main mountain system and fine erosion texture while
changing the coastline and losing three rivers. Its 32% shorter wall time is
not an acceleration claim: this was one unwarmed end-to-end run, and the
prototype produced 4.2% fewer adaptive vertices plus less river work. The final
hydraulic stage handled 1,363,043 paths in 435,486 frontiers, averaging only
3.1 paths per synchronization frontier with a maximum of 49.

## GPU-native particle direction

The continuous flux-field experiment is rejected. Across rainfall, capacity,
channel-memory, iteration-count, and shift sweeps, it transitioned from nearly
no visible incision to broad smoothing and then oversized tears. It did not
produce a useful middle regime of converging tributaries.

Batch-Synchronous Particle Erosion retains the useful part of the reference
model—each droplet remembers speed and carried sediment—without retaining its
global mutation order. A batch is one GPU dispatch:

1. recompute normals and expose the current positions and material as a frozen
   snapshot;
2. dispatch one invocation per source droplet;
3. let each invocation follow strictly downhill edges, with a stable edge bias
   and a small particle bias to avoid a perfectly regular drainage tree;
4. perform the existing capacity, erosion, and deposition exchange locally;
5. atomically add quantized XYZ and loose-material changes to four `i32`
   storage buffers; and
6. dispatch one invocation per vertex to clamp and apply the accumulated
   movement before the next batch.

No path in a batch reads another path's writes. Fixed-point integer addition is
independent of invocation order and is supported by WGSL atomics. The current
`2^22` scale resolves about 0.00000024 terrain units while leaving roughly 512
terrain units of signed accumulator range. Scratch storage is flat and reused.

## Native WGPU backend

The algorithm now has an actual WGPU 29 compute backend in addition to the CPU
oracle. It is an opt-in `island-rs` feature so CLI and Unity builds do not gain
a mandatory graphics dependency:

```sh
MOTU_EXPERIMENTAL_GPU_PARTICLE_EROSION=1 \
  cargo run --release --features gpu-erosion --bin island -- \
  --terrain-size 1024 --seed 666 --output gpu-particle-1024.png
```

For each hydraulic stage the backend:

1. packs adjacency, triangles, vertex-face incidence, and source order into one
   read-only `u32` topology buffer;
2. uploads vertex/material state once;
3. encodes all 32 batches, each with clear, normal/limit, particle, and apply
   compute passes, into one command buffer;
4. uses a single `Accumulator { atomic<i32> x, y, z, loose }` per vertex; and
5. reads positions and loose material back once, because later adaptive
   tessellation and river construction still execute on the CPU.

The normal pass computes each movement limit from the frozen position buffer.
The apply pass reads only its own vertex and accumulator. This detail matters:
an early version read neighbouring positions while other apply invocations
wrote them and produced nondeterministic topology. Two independent 128-point
runs now have the same geometry hash.

On the Apple M3 Pro, seed 666 at input size 1,024 produced:

| Backend | Erosion stages | End-to-end | Final vertices | Rivers |
| --- | ---: | ---: | ---: | ---: |
| 12-thread CPU particle oracle | 10.52 s | 33.21 s | 1,579,687 | 29 |
| Metal via WGPU | 1.99 s | 23.68 s | 1,613,583 | 21 |

The erosion stage is therefore about 5.3 times faster including device setup,
topology packing, uploads, all dispatches, and readback. End-to-end generation
is about 29% faster despite the GPU morphology producing 33,896 more adaptive
vertices. The remaining time is dominated by downstream CPU tessellation,
rivers, rocks, and export preparation.

Enable the threaded CPU oracle with:

```sh
MOTU_EXPERIMENTAL_PARTICLE_EROSION=1 cargo run --release --bin island -- \
  --terrain-size 1024 --seed 666 --output particle-1024.png
```

Optional controls are:

- `MOTU_PARTICLE_EROSION_BATCHES`, default `32`;
- `MOTU_PARTICLE_EROSION_ROUTE_JITTER`, default `0.18`; and
- `MOTU_PARTICLE_EROSION_BATCH_SHIFT`, default `0.045` of the local edge
  length, capped by the existing stage shift limit.

Seed 666 evidence from the default 32-batch settings:

| Input size | Wall time | Final vertices | Triangles | Rivers |
| ---: | ---: | ---: | ---: | ---: |
| 128 | 3.92 s | 289,020 | 577,879 | 27 |
| 1,024 | 21.86 s | 1,579,687 | 3,159,205 | 29 |

These are end-to-end CPU-oracle timings on 12 Rayon workers, not GPU speedup
claims. The 1,024 render preserves the main mountain system and produces dense,
converging erosion detail. Its steep basins are more regularly combed and some
rock exposure is harsher than the reference, so visual tuning is not complete.
Increasing to 64–96 smaller batches reduced those extremes but also returned
toward the rejected smooth look.

Focused tests prove fixed-point order independence, stable routing noise,
whole-run determinism, finite outputs, and retained projected face orientation
on a closed synthetic surface.

## Next slice

1. Allow Bevy to inject its existing render device and queue so the viewer does
   not create a second WGPU device.
2. Replace the single-edge particle step with a two-edge barycentric split on
   broad slopes to reduce combing without becoming a diffuse flux field.
3. Add accumulator-overflow telemetry and shader-visible counters before
   testing terrain sizes above 1,024.
4. Judge several seeds using image pairs, drainage density, hypsometry, and
   face-quality distributions; do not tune only to seed 666.
