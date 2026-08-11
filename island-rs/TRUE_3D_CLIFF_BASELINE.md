# True 3D Cliff Baseline

Recorded before the render-mesh implementation on 2026-08-11.

## Environment and command

```sh
cargo build --release
/usr/bin/time -l target/release/island \
  --seed 2018 \
  --width 256 \
  --height 256 \
  --terrain-size 256 \
  -o /tmp/island-3d-baseline.png
```

## Result

- Generator-reported time: 1.12 seconds.
- Wall time including PNG output: 1.59 seconds.
- Support LOD0 vertices: 283,750.
- Final rivers: 52.
- Existing Rust tests: 64 passed, including the FFI allocation stress test and
  the streamed seam diagnostic.

The sandbox prevented `/usr/bin/time -l` from reading the kernel clock data, so
peak resident memory was not available from this run. The timing and topology
figures remain suitable for the implementation's relative regression gates.

## Post-implementation comparison

A final same-binary A/B run completed in 1.11 seconds with
`--cliff-render-strength 0` and 1.20 seconds at the default detail strength.
The measured render-stage overhead was therefore 8.1%, inside the plan's 15%
limit. Support LOD0 remained at 283,750 vertices and 52 rivers. The display mesh
is generated separately and therefore does not change support topology, river
routing, maps, or the CLI raster output.
