# Island Tree

Deterministic procedural New Zealand tree generation, Bevy render compilation,
and a standalone interactive and headless visual laboratory. The species-first
generator currently includes mature pōhutukawa, nīkau palm, harakeke,
mānuka brush, kauri, and rimu architectures.

The crate owns the complete tree prototype boundary:

- a renderer-neutral botanical recipe and organ graph;
- deterministic trunk, branch, shoot, leaf, scar, and root generation;
- generated bark and leaf texture maps;
- Bevy materials with embedded bark and leaf shaders;
- static near- and middle-distance compilation for instanced placement; and
- the interactive `tree-lab` editor and its repeatable headless renderer.

`island-bevy` consumes the library API by path. Landscape placement, density,
culling, and far-distance representation remain island-renderer concerns, so
the tree crate can evolve without owning terrain policy.

Source ownership follows the same file-named module-root layout as the other
island crates:

- `botany/` owns recipes, organ data, deterministic generation, and impostors;
- `render/` owns Bevy compilation, materials, and embedded shaders; and
- `main.rs` plus `studio.rs` own the standalone review application and HUD.

Open the interactive tree editor:

```sh
cargo run --release --manifest-path island-tree/Cargo.toml --bin tree-lab
```

The HUD follows the island generator's studio layout: tree parameters are split
across Form, Branch, Foliage, Light, and Biome tabs; seed randomisation lives
beside the seed; frame and geometry telemetry stays quietly at bottom left; and
camera controls sit at bottom centre. The draft recipe rebuilds only when
its valid controls change, while view, wind, lighting, and review-biome changes
update without rebuilding tree geometry. Form and Branch expose species-aware
labels for crown or fan spread, branch or blade droop, and each plant's stem
character; these values are part of the deterministic botanical recipe rather
than studio-only presentation state. A collapsible showcase rail applies
six deterministic hero configurations in a compact two-column grid from
generated thumbnails, with Whole, Crown, and Detail camera shortcuts beneath
them. Lighting separates direct sun, exposure, and sky fill so shaded crowns can
be reviewed without clipping sunlit bark. The plant-family selector switches
between separate growth programs rather than reskinning one shared silhouette.
The three review LODs cover full leaves, middle-distance foliage pads, and a
32-vertex far impostor. Interactive inspection enables camera-distance LOD
by default: Bevy dithers across species-scaled handover bands as the camera
dollies away, while the HUD reports the current tier or blend. The transparent
eight-angle impostor atlas is baked deterministically from the generated wood
triangles, botanical organs, and bark and leaf colour maps. A depth buffer preserves
branch gaps and front-to-back material variation. Its shader keeps all
candidates on one camera-facing plane and dithers between the two nearest
object-relative views, avoiding both a generic green proxy and alpha-blended
double images.
Interactive runs use Bevy's `AutoNoVsync` present mode so the FPS meter is not
capped by display refresh.

Render a review image without opening a window:

```sh
cargo run --release \
  --manifest-path island-tree/Cargo.toml --bin tree-lab -- \
  --species nikau --screenshot /tmp/tree.png --view whole --seed 666
```

`--species` accepts `pohutukawa` (the default), `nikau`, `harakeke`, `manuka`,
`kauri`, or `rimu`.
For nīkau, `--view frond` generates and frames one mature procedural frond as a
standalone prototype, which is useful for close silhouette and leaflet review.

Add `--capture-ui` to the same command to include the interactive HUD in the
offscreen PNG for layout regression checks, still without opening a window.

Run the crate checks headlessly:

```sh
cargo test --manifest-path island-tree/Cargo.toml
cargo clippy --manifest-path island-tree/Cargo.toml --all-targets -- -D warnings
```

The source tree uses file-named module roots throughout; there are no `mod.rs`
files.
