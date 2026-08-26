# Island Tree

Deterministic procedural pōhutukawa generation, Bevy render compilation, and a
standalone headless visual laboratory.

The crate owns the complete tree prototype boundary:

- a renderer-neutral botanical recipe and organ graph;
- deterministic trunk, branch, shoot, leaf, scar, and root generation;
- generated bark and leaf texture maps;
- Bevy materials with embedded bark and leaf shaders;
- static near- and middle-distance compilation for instanced placement; and
- the offline `tree-lab` renderer used for repeatable visual review.

`island-bevy` consumes the library API by path. Landscape placement, density,
culling, and far-distance representation remain island-renderer concerns, so
the tree crate can evolve without owning terrain policy.

Render a review image without opening a window:

```sh
cargo run --release \
  --manifest-path island-tree/Cargo.toml --bin tree-lab -- \
  --screenshot /tmp/tree.png --view whole --seed 666
```

Run the crate checks headlessly:

```sh
cargo test --manifest-path island-tree/Cargo.toml
cargo clippy --manifest-path island-tree/Cargo.toml --all-targets -- -D warnings
```

The source tree uses file-named module roots throughout; there are no `mod.rs`
files.
