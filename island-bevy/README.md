# island-bevy

A Bevy viewer for the `island-rs` procedural island generator. It generates an
island in process, converts the generator's meshes into Bevy meshes, and renders
terrain, sea, rivers, river rocks and vegetation in one flyable 3D scene.

The generator is deterministic, so a given seed produces exactly the geometry,
material weights and decoration points the Unity pipeline consumes. This crate
is a second consumer of that data, not a second generator.

## Running

```sh
cargo run --release -- --seed 42 --terrain-size 256
```

The window opens immediately and shows `Generating island...` while the island
builds on a background task pool.

| Option | Default | Notes |
| --- | --- | --- |
| `--seed <N>` | `666` | Generation seed. |
| `--terrain-size <N>` | `256` | Delaunay seed-point count. `1024` matches the generator default but takes roughly 30 s; `128` returns in under a second. |
| `--max-height <HEIGHT>` | `0.2` | Normalized maximum elevation. |
| `--water-ratio <RATIO>` | `0.6` | Water coverage. |
| `--screenshot <PATH>` | — | Render, capture one PNG once the island has settled, then exit. |
| `-h`, `--help` | — | Print usage. |

## Controls

| Input | Action |
| --- | --- |
| `W` `A` `S` `D` | Move on the view plane at 220 m/s |
| `Space` / `Shift` | Move up / down |
| Left mouse held | Pan: drag the ground along under the cursor, altitude unchanged |
| Right mouse held | Look around; the cursor is grabbed and hidden |
| Scroll wheel | Zoom along the view direction; 60 m per wheel line, less per trackpad pixel |
| `R` | Return to the opening view |
| `Esc` | Release the cursor |

The camera starts about 1.6 km out at 700 m, framing the whole 2 km island.

## Screenshots

```sh
cargo run --release -- --terrain-size 128 --screenshot island.png
```

The app waits for the island, holds for a couple of seconds so render pipelines
finish compiling, writes the PNG, and exits. A non-zero exit code means the
capture never landed.

## Coordinates

island-rs is right-handed and Z-up with XY normalized to `[0, 1]` and sea level
at `z == 0`; Bevy is Y-up. Each vertex crosses as
`(x, y, z) -> ((x - 0.5) * S, z * S, (y - 0.5) * S)` with
`S = motu::ISLAND_WORLD_METRES`, normals as `(n.x, n.z, n.y)`, and every
triangle reversed because the Y/Z swap flips handedness. This matches the Unity
importer.

## Terrain colour

Terrain colour is computed per vertex on the CPU and stored in
`Mesh::ATTRIBUTE_COLOR`, so a plain `StandardMaterial` renders the generator's
own palette without a custom shader. The bands follow
`island-rs/src/raster.rs`, weighted by the per-vertex material triple from
`Island::material_values_for`: bedrock hardness selects rock, loose cover
selects grass over bare dirt, and sea proximity selects beach sand near the
shore.
