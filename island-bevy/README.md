# island-bevy

A Bevy viewer for the `island-rs` procedural island generator. It generates an
island in process, converts the generator's meshes into Bevy meshes, and renders
terrain, sea, rivers, river rocks and vegetation in one flyable 3D scene.

The generator is deterministic, so a given seed produces exactly the geometry,
material weights and decoration points the Unity pipeline consumes. This crate
is a second consumer of that data, not a second generator.

## Running

```sh
cargo run --release -- --seed 42
```

The window opens immediately and shows `Generating island...` while the island
builds on a background task pool.

Options are applied in the order given, so a later one wins: `--variant eroded
--max-height 0.3` keeps the variant's erosion and takes the spelled-out height.

| Option | Default | Notes |
| --- | --- | --- |
| `--seed <N>` | `666` | Generation seed. |
| `--terrain-size <N>` | `1024` | Delaunay seed-point count. The first generation takes roughly 30 s, then the cache makes repeat launches fast; `128` or `256` iterate quickly. |
| `--max-height <HEIGHT>` | `0.2` | Normalized maximum elevation. |
| `--water-ratio <RATIO>` | `0.6` | Water coverage. |
| `--variant <NAME>` | `default` | Named generation variant; see below. |
| `--view <NAME>` | `overview` | Named camera pose to open on and to reset to; see below. |
| `--screenshot <PATH>` | — | Render, capture one PNG once the island has settled, then exit. |
| `--no-cache` | off | Generate even when a cached island matches these inputs; the entry is rewritten either way. See below. |
| `-h`, `--help` | — | Print usage. |

### Cache

Generation is deterministic but slow — seconds at terrain size 256, about half
a minute at 1024 — and nothing about it changes between launches, so the
geometry the renderer reads is cached on disk and a repeat launch skips the
generator entirely.

Entries live in `target/island-cache/`, one file per island, named after a hash
of the seed and every generator option. Changing any of them is a different
island and so a different file; nothing is ever invalidated in place, and the
directory only grows. `cargo clean` clears it, and so does deleting the
directory. Each entry is the finished meshes, material weights and decoration
points in a flat binary layout — tens of megabytes at terrain size 256 — and is
read on the same background task generation would have run on, so the first
frame never waits on it.

An entry is only read when the seed and all fifteen options recorded in it
match the run asking for it exactly. Anything else — a missing, truncated,
oversized or otherwise damaged file, or one written by an earlier format
version — is a miss, and the island is generated and the entry rewritten. The
log says which happened on every run:

```
island cache hit: /…/target/island-cache/6a1f….bin
island cache miss: /…/target/island-cache/6a1f….bin
```

`--no-cache` skips the read and generates unconditionally, then writes a fresh
entry. Reach for it when the generator itself has changed — a new island under
an unchanged seed and options would otherwise keep reading the old geometry
back. Bumping `CACHE_FORMAT_VERSION` in `src/cache.rs` retires every existing
entry at once and is the durable answer to the same problem.

### Variants

A variant is a named set of generator option overrides, applied on top of the
defaults and of any option given before it.

| `--variant` | Overrides |
| --- | --- |
| `default` | None. |
| `eroded` | `hydraulic_erosion_strength = 4.0`, `coastal_slope_multiplier = 0.25`: deeply incised channels inland over wide, shallow shores. |

### Views

A view is a named camera pose. It is where the camera opens and where `R`
returns to, so a `--view` plus a `--screenshot` is a repeatable capture. The
five close poses were framed on seed 666 at terrain size 1024; other seeds, and
other terrain sizes, put different ground under them.

A variant moves the generated channels, so every view that frames running water
carries a second pose for `eroded` and `--variant` selects between them. At
1024 the `default` island cuts 28 channels but keeps only twelve above the
waterline, so its four river poses sit on the south-west catchment, where the
largest of them falls through two drops into a bay. The `eroded` island cuts 23
and keeps three, so its poses sit on the one south-east reach that carries both
settled stones and a fall — about 40 m of running water against the `default`
channel's 100, which is why its wider views stand closer in.

| `--view` | Frames | `--variant eroded` |
| --- | --- | --- |
| `overview` | The whole island from 1.6 km out at 700 m. | Same pose. |
| `mountain` | The main massif's relief, with the coast and horizon behind it. | Same pose. |
| `river-region` | The south-west catchment from over its bay: two channels, the inlet and the coves they reach. | The south-east reach from off its cove: channel, fall and cove in one frame. |
| `river-ground` | Gameplay distance up the same catchment's lower channel. | Up the cove to the channel, from about 80 m. |
| `river-level4` | Near-ground from the bay onto the mouth the channel falls out of, close enough to read water, bank, rock and sand materials. | The drop into the cove, where apron, bedrock, grass, fresh water and sea meet. |
| `stream` | Standing height in the catchment's second channel, looking up its stony reach to the fall. Nothing further out resolves 6–22 cm stones. | The stone-strewn reach above the fall. |

## Controls

| Input | Action |
| --- | --- |
| `W` `A` `S` `D` | Move on the view plane at 220 m/s |
| `Space` / `Shift` | Move up / down |
| Left mouse held | Pan: drag the ground along under the cursor, altitude unchanged |
| Right mouse held | Look around; the cursor is grabbed and hidden |
| Scroll wheel | Zoom along the view direction; 60 m per wheel line, less per trackpad pixel |
| `R` | Return to the `--view` pose |
| `Esc` | Release the cursor |

The camera starts on the pose `--view` names, which is `overview`: about 1.6 km
out at 700 m, framing the whole 2 km island.

## Screenshots

```sh
cargo run --release -- --view river-level4 --screenshot island.png
```

The app waits for the island, then holds several seconds while the atmosphere,
occlusion, contact-shadow and bloom pipelines compile and the temporal
anti-aliasing converges, writes the PNG, and exits. A non-zero exit code means
the capture never landed.

## Rendering

The scene is lit by one directional sun carrying raw, unfiltered sunlight and a
physical `Atmosphere` sitting on an ocean-albedo planet whose surface is sea
level. The atmosphere draws the sky, filters the sun on the way down, supplies
image-based ambient through `AtmosphereEnvironmentMapLight`, and lays aerial
perspective over the scene, so there is no clear colour, no distance fog and no
uniform ambient term to keep in step with each other.

The camera renders in high dynamic range at a fixed exposure through ACES tone
mapping, with temporal anti-aliasing (multisampling off), screen-space ambient
occlusion, contact shadows and restrained bloom.

## Coordinates

island-rs is right-handed and Z-up with XY normalized to `[0, 1]` and sea level
at `z == 0`; Bevy is Y-up. Each vertex crosses as
`(x, y, z) -> ((x - 0.5) * S, z * S, (y - 0.5) * S)` with
`S = motu::ISLAND_WORLD_METRES`, normals as `(n.x, n.z, n.y)`, and every
triangle reversed because the Y/Z swap flips handedness. This matches the Unity
importer.

## Surface materials

Terrain, river rocks, the sea and the rivers are each drawn by an
`ExtendedMaterial<StandardMaterial, _>` whose WGSL lives beside the source in
`src/` and is embedded in the binary. Extending rather than replacing
`StandardMaterial` keeps the shadow
cascades, the depth and motion-vector prepasses, screen-space occlusion,
contact shadows and aerial perspective working; only the forward fragment stage
is the crate's own. There are no texture assets: every detail layer is
hash-lattice value noise evaluated in world space, from `src/noise.wgsl`.

The terrain mesh carries the generator's raw material triple from
`Island::material_values_for` in `Mesh::ATTRIBUTE_COLOR` — bedrock hardness,
loose cover, sea proximity — and `src/terrain.wgsl` is the only authority on
what that becomes. It reads elevation and slope from world position and normal,
resolves the bands that follow `island-rs/src/raster.rs`, and layers on top of
them:

| Layer | Wavelength | Carries |
| --- | --- | --- |
| Macro | 430 m | Albedo drift, how dry established cover is, and the phase and frequency every finer layer is offset and jittered by |
| Patch | 34 m | Cover patchiness and hillside brightness |
| Grain | 2.4 m | Per-band mottling and the metre-scale normal relief |
| Micro | 0.42 m | Sand and rock grain, close to the camera only |

Slope, height and cover modifiers refine the generator's weights; they never
replace them. Roughness follows the band — rock 0.90, grass 0.85, sand 0.70 —
and the shoreline darkens and smooths within a couple of metres of the sea
plane. River banks want the same wetness but need a per-vertex river distance
the renderer is not given.

The merged river-rock body is 6–22 cm stones with the occasional 65 cm boulder.
`convert::rock_mesh` hashes world position on a 20 cm lattice into a per-body
albedo tint, which is as close to per-instance as one merged mesh allows, and
`src/rock.wgsl` adds mineral colour, roughness variation and centimetre relief
that fades out past 25 m.

## Water materials

Both waters blend, so neither is written to the depth prepass, and both read
that prepass to recover how far the view ray runs through water before it
reaches the ground under it. That distance drives Beer-Lambert absorption
twice: once for how much of the bottom still comes through, and once, faster,
for what is left of the bottom's colour, because one alpha channel cannot carry
three extinctions. View-angle Fresnel and foam combine with it as independent
chances of the ray not carrying the bottom to the eye, and the sun glitter is
the ordinary specular lobe over a low, noise-varied roughness.

`src/ocean.wgsl` shades the sea plane from world-space XZ, since the quad's own
UVs are a meaningless stretch. Three drifting noise layers at 46 m, 7.4 m and
1.3 m carry the wave slope, each dropped once it is finer than a pixel and
replaced by roughness. The absorption saturates towards the same deep tone the
terrain shader's seabed band converges on, so the generated shelf and the empty
ocean past the terrain square arrive at the same colour and the square's edge
stops reading. Surf needs shallow water *and* little ground left before the
waterline — the bottom's grade comes off the normal prepass — because the
generated shelf is one to three metres deep across the whole square and a band
in depth alone fills every cove.

`src/river.wgsl` runs on the generator's channel parametrisation instead:
`uv.y` is distance travelled downstream and `uv.x` is distance to the nearest
bank, both normalized island units. Screen derivatives of the downstream
coordinate recover the flow direction in the world, which per-vertex tangents
cannot because a bank distance turns over at the centreline. Two layers travel
along it — one at a fixed pace everywhere, one at the surface grade's own pace
and carrying amplitude only where there is a grade — over a world-space chop
near the camera. Fresh water absorbs at well under half the sea's rate, so beds
and stones stay readable; the surface fades out over the last handspan of bank
distance; and it only breaks white where the surface grade is steep, which is
what makes the generator's waterfalls read as falling water rather than as
tilted sheets.

Trees and bushes keep one merged mesh each and one shared white
`StandardMaterial`. Bark, canopy and shrub tones ride in the mesh's vertex
colours, which is what leaves the trunk bark coloured under a material that
knows nothing about either. A shared material handle cannot carry a
per-instance tint, so per-plant variation is baked into four meshes per class,
selected by the same deterministic hash as scale and yaw; each class batches
once per variant rather than once in total.
