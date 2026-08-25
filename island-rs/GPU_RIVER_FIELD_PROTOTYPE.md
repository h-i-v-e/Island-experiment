# GPU River Field Prototype

## Status

This branch contains an opt-in, from-scratch river generator designed around
fixed-size GPU passes rather than the legacy mutable river graph and repeated
terrain-topology reconstruction.

Enable it with:

```sh
MOTU_EXPERIMENTAL_GPU_RIVERS=1 \
MOTU_GPU_RIVER_STATS=1 \
cargo run --release --features gpu-rivers --bin generation-bench -- \
  --terrain-size 1024 --repetitions 1 --seed 666
```

The hydrology grid defaults to 512 square. `MOTU_GPU_RIVER_GRID` accepts values
from 128 through 1024. `MOTU_GPU_RIVER_CATCHMENT_MULTIPLIER` controls drainage
density and defaults to 12.

## Algorithm

The design is a **spill-potential rainfall field**:

1. Sample the final irregular terrain onto a regular hydrology grid.
2. Mark the sea and grid perimeter as outlets. All inland cells begin with an
   infinite spill potential.
3. Jacobi-relax the minimum outlet elevation inward. Each cell becomes the
   greater of its terrain elevation and the lowest neighbouring spill
   potential plus a tiny downstream gradient. This is a parallel, fixed-field
   approximation of depression filling: local pits drain without a priority
   queue or terrain mutation.
4. Give every land cell one rainfall packet. Each GPU invocation follows the
   strictly descending spill field and atomically adds its packet to every cell
   it crosses. The accumulated integer count is catchment area.
5. Convert catchment flow to channel width and depth logarithmically. Minor
   drainage is removed with one visual-catchment threshold rather than by
   sequential source insertion and collision pruning.
6. In one vertex-parallel pass, find the nearest channel cell, carve a smooth
   valley apron, mark the bed, and emit the water surface.
7. Read back once. Linear CPU conversion compacts selected original terrain
   triangles into the existing river mesh and traces a small public `River`
   path representation from the completed downstream field.

The CPU never participates in an iteration and there is no GPU/CPU ping-pong.
Rasterization and output conversion are endpoint work only.

## Why this shape

The current river stage spends most of its time rebuilding and querying
changing mesh adjacency, perimeters and compact river topology. Directly
parallelizing that graph mutation would preserve its worst scaling property.
The field model keeps topology immutable and turns the expensive middle into
uniform reads, ping-pong writes and integer atomics.

The spill recurrence is informed by Priority-Flood's requirement that every
cell must obtain an outlet, but replaces its ordered CPU priority queue with a
GPU-friendly relaxation. The rainfall walk is closer to GPU drainage-network
and flow-accumulation work than to a shallow-water simulation: its purpose is
to construct a convincing static drainage hierarchy, not simulate a storm.

Research references:

- Barnes, Lehman and Mulla, [Priority-Flood: An Optimal Depression-Filling and
  Watershed-Labeling Algorithm for Digital Elevation
  Models](https://rbarnes.org/sci/2014_depressions.pdf).
- Ortega and Rueda, [Parallel drainage network computation on
  CUDA](https://doi.org/10.1016/j.cageo.2010.02.002).
- Qin and Zhan, [Parallelizing flow-accumulation calculations on graphics
  processing units](https://www.lreis.ac.cn/xsyj/kycg/scilw/201610/P020250317378478277511.pdf).

## Measured prototype

Apple M3 Pro, seed 666, 512-square hydrology field:

| Terrain | River solver | Whole generation | Output |
| --- | ---: | ---: | --- |
| 1024, all GPU experiments enabled | 228 ms headless | 5.31 s headless | 1,477,473 terrain vertices |
| 1024, Bevy rendering concurrently | 2.25 s | not isolated | 73 paths, 45,265 bed vertices, 83,883 water triangles |

The earlier CPU final-river stage on the same 1024 seed was approximately
6.4 seconds. The headless prototype result is roughly 28 times faster for the
river stage. Bevy currently creates a second WGPU device while its renderer is
active, so its concurrent timing is not representative of generator-only
throughput.

The exact 1024 output counts repeated across two native Metal runs.

## Current limitations

- This is an opt-in prototype and does not replace the default generator.
- The 512 grid gives drainage a subtle D8/raster character in some headwaters.
- Small headwaters can be narrower than a complete triangle strip at coarse
  terrain sizes. The 1024 terrain output is substantially more continuous than
  the 256 diagnostic output.
- It carves sloped channels but does not yet synthesize explicit waterfall
  terraces, plunge pools or river-specific rock geometry.
- The final mesh compaction and public path conversion are still on CPU. They
  are linear endpoint conversions, not simulation stages.
- Terrain-to-grid sampling is currently CPU-parallel. Moving this first pass to
  the existing triangle-index GPU sampler would remove the remaining
  pre-dispatch work.

The next visual improvement should be a direction-smoothing pass over the spill
field followed by sub-cell centreline offsets. That would break D8 alignment
without reintroducing a mutable topology or CPU tracing loop.
