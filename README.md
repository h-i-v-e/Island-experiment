# Island experiment

Procedural free-form island generation with interactive Bevy and Unity
renderers.

- [`island-rs`](island-rs/) contains the Rust generator, C ABI, tests, and the
  default-enabled experimental GPU compute implementation.
- [`island-bevy`](island-bevy/) is the interactive Bevy viewer, including
  cinematic rendering, generation controls, repeatable captures, and a runtime
  CPU/GPU comparison switch.
- [`island-unity`](island-unity/) contains the reusable Unity 6
  `IslandGenerator` component, sandbox level, streamed terrain LODs,
  first-person sample controls, the Apple Silicon native plugin, and the
  [Procedural Material Studio](island-unity/PROCEDURAL_MATERIAL_STUDIO.md).

## Experimental GPU generation

The original CPU generator remains the primary runtime implementation. The
default `island-rs` Cargo feature `gpu-generation` includes GPU-native hydraulic
erosion, river generation, and rock settling. It targets a convincing natural
result rather than byte-for-byte parity with CPU output. CPU-only consumers can
omit that code with `--no-default-features`.

`island-bevy` builds both methods and exposes **CPU** and **GPU** buttons in the
header for direct A/B comparison. The same choice is available on the command
line:

```sh
cd island-bevy
cargo run --release -- --generation-method gpu --seed 666
```

CPU and GPU results use separate cache directories. See each project README for
its build, API, cache, and compatibility details.
