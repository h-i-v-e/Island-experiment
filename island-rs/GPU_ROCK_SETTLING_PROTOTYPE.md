# GPU rock-settling prototype

This branch contains an opt-in, GPU-native replacement for the decoration rock
rigid-body loop. It is deliberately a new visual model rather than a port of
the ordered CPU impulse simulation. The required output is a convincing,
seeded distribution of resting stones and boulders; matching every CPU body
trajectory is not a goal.

The prototype is enabled with the `gpu-rocks` feature and an explicit runtime
guard:

```sh
MOTU_EXPERIMENTAL_GPU_ROCK_SETTLING=1 \
  cargo run --release --features gpu-rocks --bin island -- \
  --terrain-size 1024 --seed 666 --output gpu-rocks.png
```

Set `MOTU_GPU_ROCK_STATS=1` to print the adapter, accepted support counts,
maximum final grid occupancy, overflow count, state hash, and GPU-stage wall
time. `MOTU_GPU_ROCK_STEPS` changes the default 180 steps and
`MOTU_GPU_ROCK_TIME_STEP` changes the default 1/30-second step.

## Algorithm

The CPU still performs the existing deterministic, geology-weighted spawn.
After that, the complete settling solve runs in one Metal command buffer:

1. Integrate gravity and the seeded horizontal launch velocity for every rock.
2. Clear a 512 by 512 spatial grid on the GPU.
3. Scatter rock IDs into fixed 32-entry cells with integer atomics.
4. Sort each occupied cell by rock ID. This removes atomic insertion order from
   the contact traversal.
5. Give one invocation ownership of one rock. It gathers the 3 by 3 neighbouring
   cells, computes Jacobi position corrections against the previous state
   buffer, samples the irregular terrain mesh, and writes only its own next
   state to the other ping-pong buffer.

The terrain sampler uses the generator's existing triangle-bin offsets, face
IDs, triangle indices, vertex positions, and normals directly on the GPU. It
does not build a regular height map or read terrain samples back to the CPU.

The collision stage is position based rather than pairwise impulse based. A
rock cannot race another invocation because no invocation writes another
rock's state. Terrain projection removes the inward velocity component while
preserving the downhill tangent, which produces slope shedding without the
CPU solver's 360 sequential collision passes.

There is one upload before the solve and one readback after all 900 compute
passes. The readback is required because the existing mesh builder consumes
the final rock anchors on the CPU; there is no per-step CPU/GPU synchronization.

## Why this shape

Position-based dynamics is intended for controllable, stable constraint
projection and maps naturally to parallel particle solvers. The Jacobi form is
less immediately convergent than an ordered in-place solver, but it eliminates
conflicting writes. A uniform grid is effective here because rock diameters
occupy a narrow range and the simulation domain is a bounded height field.

The implementation follows the broad principles from:

- Müller et al., [Position Based Dynamics](https://diglib.eg.org/items/deb0a7a1-2ddf-496f-889a-fe0df1feeb73), 2006;
- Macklin et al., [Unified Particle Physics for Real-Time Applications](https://doi.org/10.1145/2601097.2601152), 2014;
- NVIDIA GPU Gems 3, [Real-Time Rigid Body Simulation on GPUs](https://developer.nvidia.com/gpugems/gpugems3/part-v-physics-simulation/chapter-29-real-time-rigid-body-simulation-gpus); and
- the current [WGSL specification](https://gpuweb.github.io/gpuweb/wgsl/) for
  storage-buffer and atomic semantics.

## 1024 result

Apple M3 Pro, seed 666, GPU particle erosion enabled, release build:

| Rock backend | Rock settling | End-to-end generation |
| --- | ---: | ---: |
| Existing ordered CPU simulation | 5.62 s | 15.45 s |
| GPU Jacobi prototype | 0.33-0.43 s | 10.22-10.75 s |

The isolated rock stage is approximately 14 times faster in the directly paired
profile and removes roughly another third of current end-to-end generation
time. The 1024 solve processed 28,671 candidates and emitted 9,106 accepted
rocks. The final grid maximum was 24 rocks in a 32-entry bucket, with zero
overflows.

Two independent 1024 runs produced the same rock-state hash
`6316620881779037454`, the same accepted-rock count, and the same cleared-soil
count on the test machine. The earlier 16-entry prototype did overflow eight
cells and was measurably nondeterministic; it is not the retained design.

## Remaining work

- Review several seeds from a closer ground-level camera. The 256 Bevy capture
  preserves the broad rock scatter, but the altered paths also change which
  vertices have loose soil cleared.
- Treat any bucket overflow as a rejected run or redispatch with a larger
  capacity before calling this production ready. The current stats expose an
  overflow, but the experimental path does not abort.
- Share the already-created WGPU device and immutable terrain buffers with GPU
  erosion. The prototype intentionally proves the solver first, so its timing
  still includes a second device setup and terrain upload.
- Tune step count, contact damping, and Jacobi relaxation across multiple
  terrain sizes. Byte-for-byte parity with the CPU simulation is not expected.
