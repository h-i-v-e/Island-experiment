# Island experiment

Procedural free-form island generation with interactive Bevy and Unity
renderers.

- [`island-rs`](island-rs/) contains the Rust generator, C ABI, tests, and the
  default-enabled experimental GPU compute implementation.
- [`island-bevy`](island-bevy/) is the interactive Bevy viewer, including
  cinematic rendering, generation controls, repeatable captures, and a runtime
  CPU/GPU comparison switch.
- [`island-material-studio`](island-material-studio/) is the standalone Bevy
  procedural-material authoring application. It edits the same typed JSON
  recipes as the baker and Unity studio, previews them in 2D and with lit
  parallax mapping, and bakes through the shared transactional writer.
- [`island-tree`](island-tree/) owns the deterministic pōhutukawa generator,
  its Bevy material and static-mesh compiler, and the headless `tree-lab`
  visual-review application. Landscape placement policy remains in
  `island-bevy`.
- [`island-unity`](island-unity/) contains the reusable Unity 6
  `IslandGenerator` component, sandbox level, streamed terrain LODs,
  first-person sample controls, a CPU/GPU generation selector, the Apple
  Silicon native plugin, and the
  [Procedural Material Studio](island-unity/PROCEDURAL_MATERIAL_STUDIO.md).

## Experimental GPU generation

The original CPU generator remains the primary runtime implementation. The
default `island-rs` Cargo feature `gpu-generation` includes GPU-native hydraulic
erosion and rock settling. Both methods use the established CPU river and
waterfall builder, preserving its connected channels and geometric waterfall
contracts. GPU-eroded terrain still differs from CPU output. CPU-only consumers
can omit the compute code with `--no-default-features`.

`island-bevy` builds both methods and exposes **CPU** and **GPU** buttons in the
header for direct A/B comparison. Unity exposes the same selector in the
`IslandGenerator` generation settings and its sandbox defaults to GPU. The
choice is also available on the Bevy command line:

```sh
cd island-bevy
cargo run --release -- --generation-method gpu --seed 666
```

CPU and GPU results use separate cache directories. See each project README for
its build, API, cache, and compatibility details.

## Procedural Material Studio

Run the standalone editor without Unity:

```sh
cargo run --release \
  --manifest-path island-material-studio/Cargo.toml -- \
  --recipe island-rs/texture-recipes/rounded-river-stones.json
```

The studio calls `island-rs` directly in process. Preview and final bake use
the same evaluator as `island-texture-baker`; parallax mapping is used only for
the lit height preview and does not alter baked height bytes.
