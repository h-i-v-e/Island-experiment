# Island Material Studio

Standalone Bevy authoring application for the typed procedural-material recipes
owned by [`island-rs`](../island-rs/). The application is intentionally a
separate crate from the island viewer: `island-rs` remains engine-neutral and
the studio calls its evaluator directly in process.

The document core is ordinary Rust and does not require a window or GPU. It
provides:

- typed recipe open/new/revert/save/save-as lifecycle;
- canonical pretty JSON with a trailing newline and source-byte SHA-256
  tracking;
- external-change conflicts that require Reload, Save As, or explicit
  Overwrite;
- sibling-temporary atomic replacement with cleanup on failure;
- bounded snapshot undo/redo and one-transaction gesture coalescing;
- stable layer IDs with add, duplicate, rename, enable, reorder, and delete
  operations;
- recent-file and small UI-preference persistence.

The Bevy shell owns the UI, preview scheduler, coherent image uploads, lit
render target, and bake panel. The crate uses file-named module roots throughout;
there are no `mod.rs` files.

The editor provides:

- complete typed base-material and layer-stack editing;
- bounded snapshot undo/redo and dirty-document protection;
- debounced 128, 256, or 512-pixel background previews with stale-result
  rejection and an eight-entry CPU cache;
- albedo, height, normal, AO, packed-mask, and selected-layer diagnostics;
- tiled 2D inspection and a lit sphere/plane preview using Bevy parallax;
- explicit background baking through `generate_texture_set` and the existing
  transactional `write_texture_set` boundary.

During development, the intended launch shape is:

```sh
cargo run --release \
  --manifest-path island-material-studio/Cargo.toml -- \
  --recipe island-rs/texture-recipes/rounded-river-stones.json
```

For repeatable visual acceptance, `--window-size 1100x700`,
`--preview-tab lit`, and `--screenshot /tmp/studio.png` are also available.
They are test-only conveniences and do not change recipe state.

On macOS, create an optimized ad-hoc-signed application bundle with:

```sh
./island-material-studio/package-macos.sh
```

The bundle is written to `island-material-studio/dist/Procedural Material
Studio.app`. The packager refuses to replace an existing bundle implicitly.

Document and persistence tests can run headlessly with `cargo test
--manifest-path island-material-studio/Cargo.toml`; packaging validation is
separate from a Cargo run.

The lit preview derives Bevy depth as `1 - normalized_height`, because Bevy
defines white as the bottom and black as the top. Height remains an R16 CPU map
for inspection and baking; parallax does not modify geometry or silhouettes.
The plane is the default lit-preview shape because parallax is a tangent-plane
technique; Bevy documents visible grazing-angle distortion on curved surfaces,
so the sphere remains available primarily for checking lighting and normals.
