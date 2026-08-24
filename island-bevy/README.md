# island-bevy

A Bevy viewer for the `island-rs` procedural island generator. It generates an
island in process, converts the generator's meshes into Bevy meshes, and renders
terrain, sea, rivers, river rocks and vegetation in one 3D scene you can fly
over or walk around.

The generator is deterministic, so a given seed produces exactly the geometry,
material weights and decoration points the Unity pipeline consumes. This crate
is a second consumer of that data, not a second generator.

## Running

```sh
cargo run --release -- --seed 42
```

The window opens immediately and shows `Generating island...` while the island
builds on a background task pool. Press `H` for the parameter panel, which
generates a new island without a restart, and `F` to walk on the one you have.

Options are applied in the order given, so a later one wins: `--variant eroded
--max-height 0.3` keeps the variant's erosion and takes the spelled-out height.

| Option | Default | Notes |
| --- | --- | --- |
| `--seed <N>` | `666` | Generation seed. |
| `--terrain-size <N>` | `1024` | Delaunay seed-point count, 16 to 4096. The first generation takes roughly 30 s, then the cache makes repeat launches fast; `128` or `256` iterate quickly. |
| `--variant <NAME>` | `default` | Named generation variant; see below. |
| `--view <NAME>` | `overview` | Named camera pose to open on and to reset to; see below. |
| `--screenshot <PATH>` | — | Render, capture one PNG once the island has settled, then exit. |
| `--no-cache` | off | Generate even when a cached island matches these inputs; the entry is rewritten either way. Applies to the HUD's rebuilds as well as to the first island. See below. |
| `-h`, `--help` | — | Print usage. |

Every one of the generator's fifteen parameters has a flag of its own, so any
island the HUD finds can be reopened from the command line. `--help` prints
them with the range the HUD offers each over. The flags themselves are not held
to those ranges — only `Island::generate` rejects a value, and only for the
parameters it validates.

| Group | Flags | Default |
| --- | --- | --- |
| Terrain | `--terrain-size` | `1024` |
| | `--max-height` | `0.2` |
| | `--water-ratio` | `0.6` |
| | `--slope-multiplier` | `1.3` |
| | `--coastal-slope-multiplier` | `1.0` |
| Hydraulics | `--hydraulic-erosion-strength` | `1.0` |
| | `--hydraulic-deposition-strength` | `1.5` |
| | `--hydraulic-deposition-slope-degrees` | `12.0` |
| Rivers | `--river-source-catchment-hectares` | `0.05` |
| | `--river-source-steep-multiplier` | `4.0` |
| | `--river-source-elevation-boost` | `9.0` |
| | `--river-source-width-metres` | `2.0` |
| | `--river-maximum-width-metres` | `14.0` |
| | `--river-source-depth-metres` | `0.35` |
| | `--river-maximum-depth-metres` | `2.0` |

`src/options.rs` holds all fifteen in one table — the flag, the HUD's range and
the field — and the parser, the help text, the HUD's sliders, the command line
the HUD reports and the cache key all walk it. A parameter added to
`IslandOptions` is added there once.

### Cache

Generation is deterministic but slow — seconds at terrain size 256, about half
a minute at 1024 — and nothing about it changes between launches, so the
geometry the renderer reads is cached on disk and a repeat launch skips the
generator entirely.

Entries live in `target/island-cache/`, one file per island, named after a hash
of the seed and every generator option. Changing any of them is a different
island and so a different file; nothing is ever invalidated in place, and the
directory only grows. `cargo clean` clears it, and so does deleting the
directory. Each entry is the finished meshes, material weights, per-vertex
river wetness, decoration points and walk mode's height grid in a flat binary
layout — tens of megabytes at terrain size 256, of which the grid is 1 MiB and
the wetness one float per terrain vertex — and is read on the same
background task generation would have run on, so no frame ever waits on it.
The HUD's rebuilds read it too, which is what brings a parameter set you have
already visited back in milliseconds.

An entry is only read when the seed and all fifteen options recorded in it
match the run asking for it exactly. Anything else — a missing, truncated,
oversized or otherwise damaged file, or one written by an earlier format
version — is a miss, and the island is generated and the entry rewritten. The
format version is mixed into the key as well as written into the entry, so
entries from before a bump are never even opened, let alone read as damage. The
current version is 3, which added the per-vertex river wetness; 2 added the
height grid. The log says which happened on every run and on every rebuild:

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

The camera has two movement modes, and `F` switches between them. It starts
flying, on the pose `--view` names, which is `overview`: about 1.6 km out at
700 m, framing the whole 2 km island.

The two take the mouse on opposite terms, because they are used for opposite
things. **Flying** is an editor view of the island: the cursor stays free and
the scene is steered with the buttons. **Walking** is a person on the ground, so
it follows the conventions a person already has for that: the cursor is captured
and the mouse looks with no button held.

| Input | Flying | Walking |
| --- | --- | --- |
| Mouse | Only with a button held, below | Looks, always, no button held |
| `W` `A` `S` `D` | Move on the view plane at 220 m/s | Move along the ground relative to the view, at 1.5 m/s |
| `Shift` | Move down | Sprint, at 3 m/s |
| `Space` | Move up | Jump, about 1 m |
| Left mouse | Held: pan, dragging the ground along under the cursor | Click: take the cursor back after `Esc` |
| Right mouse | Held: look around, cursor captured | — |
| Scroll wheel | Dolly along the view direction; 60 m per wheel line, less per trackpad pixel | — |
| `F` | Start walking | Take off |
| `R` | Return to the `--view` pose | Return to the `--view` pose, flying |
| `H` | Show or hide the parameter panel | Show or hide it, handing the cursor over and taking it back |
| `Esc` | Release the cursor | Release the cursor; looking stops until you click |

`H` and `F` are the only two bindings walk mode and the panel add. While a panel
field has the keyboard, or the pointer is over the panel, neither the camera nor
either toggle sees the input.

### Walking

`F` captures and hides the cursor straight away and seats the eye 1.8 m over the
ground directly under it, however high the camera was. From then on the mouse
looks, `W` `A` `S` `D` moves along the ground relative to where you are facing,
`Shift` sprints and `Space` jumps. The wheel does nothing: a person does not
dolly. `F` again, or `R`, hands the cursor back and returns to flying — the
named poses are all in the air.

The cursor is given up in two other places, and taken back afterwards:

- `Esc` releases it and looking stops. A left click anywhere in the scene takes
  it again.
- Opening the parameter panel releases it for as long as the panel is up, since
  a captured cursor could never reach the panel to click it. Closing the panel
  takes it back without spending a click. Entering walk mode closes an open
  panel for the same reason: on foot the panel behaves like a pause screen.

Jumping is a launch speed that reaches about a metre under gravity, integrated
each frame and floored at the ground under the walker, so a jump onto rising
ground lands on it rather than passing through. Steering stays live in the air.
Walking itself never leaves the surface: sampled bilinearly the ground has no
vertical faces to fall down, so following it is the whole of walking.

The ground comes from a 512 by 512 grid of the generator's own `height_map`.
That is one height every 3.9 m across the two kilometre island and 1 MiB in the
cache entry, a few per cent of an entry that already runs to tens of megabytes.
Cliff faces smooth into ramps at that spacing, which for walking around is what
you want; every valley, ridge and riverbed the eye reads at head height is
there.

Water is not swum. A step that would put the walker in more than a metre of
water is refused — in the air as much as on the ground — and the two axes are
then tried on their own so the shoreline turns the walk along itself rather than
stopping it dead. The generated shelf runs one to three metres deep across the
whole square, so the sea is what stops a walk at the waterline. Ground already
deeper than that, where walk mode was entered over open water, is stood on at
one metre rather than sunk into, which leaves the eye 0.8 m over the sea. Off
the square the grid clamps to its edge, so there is nothing to fall through.

## Parameter panel

`H` shows and hides a panel carrying the seed and all fifteen generator
parameters, so an island can be hunted for without restarting. Sliders span the
range each parameter is useful over — the generator's own limits where it
validates one, working ranges otherwise — and each is labelled with the flag
that reproduces it.

Nothing regenerates while a slider moves. Generation takes seconds to a couple
of minutes, so the panel edits a draft and **Regenerate** hands the whole draft
over at once. The button dims while a build runs, and the status line above it
counts the seconds. The island on screen stays up the whole time; when the new
one lands, every entity the old one spawned is despawned and the new set is
spawned in the same frame.

Under the button is the argument list that reproduces the island on screen, with
a copy button. The same line goes to the log on every build:

```
island arguments: --seed 666 --terrain-size 128 --max-height 0.2 …
```

Rebuilds read the cache like any other generation, so a parameter set you have
already visited comes back in tens of milliseconds rather than seconds. A
rebuild whose parameters the generator rejects leaves the island on screen alone
and reports the error on the panel; only a first island that cannot be generated
is fatal.

A small dimmed frames-per-second readout sits in the bottom-left corner,
smoothed and refreshed twice a second. It is always on and is not part of the
`H` toggle.

The panel is drawn with [`bevy_egui`](https://crates.io/crates/bevy_egui), the
crate's only dependency outside `bevy` and `island-rs`.

## Screenshots

```sh
cargo run --release -- --view river-level4 --screenshot island.png
```

The app waits for the island, then holds several seconds while the atmosphere,
occlusion, contact-shadow and bloom pipelines compile and the temporal
anti-aliasing converges, writes the PNG, and exits. A non-zero exit code means
the capture never landed.

A capture run stays out of the way of whatever else is open. Its window never
takes the keyboard, and neither the panel nor the frame-rate readout is built at
all under `--screenshot`, so no capture can carry either of them whatever the
`H` toggle was last left at.

The window does still appear. A capture reads that window's own surface back,
and macOS only keeps a surface current while the window is composited, so
`Window { visible: false, .. }`, minimizing it and `WindowLevel::AlwaysOnBottom`
each produce a valid PNG of solid black. Each was tried on its own. Opening
unfocused is as far as it can go and still capture.

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
plane. River banks take the same treatment more lightly, from a proximity to
running water measured per vertex; see below.

Every layer is held to what one pixel can actually resolve. The footprint is
taken off the warped detail space itself, with `dpdx`/`dpdy`, so range,
incidence, the frequency jitter and the warp's own local stretch all count
towards it, and each octave of the metre-scale layer fades out as the footprint
reaches its own wavelength. The ratio between the two screen axes is capped at
four to one: filtering to the long axis alone leaves flat paint on ground the
view runs along, and filtering to the short one leaves every bit of the pattern
there for the eye to read as strokes drawn along the view. Detail relief is
also faded by the incidence itself, down to a third of it edge-on, because a
bumped normal answers the sun far out of proportion to the relief it stands for
once the surface is seen edge-on. Albedo keeps its own detail throughout.

The macro layer's domain warp drags the finer ones by 14 m rather than the 55 m
it was first given. What has to stay small is the gradient of that offset, not
its size: the warp field turns over every hundred metres or so, so tens of
metres of drag stretched the domain by as much as the domain itself, and the
finer layers arrived combed into long filaments that a grazing view then drew
out into smears.

### River banks

The generator publishes channels, not a distance to them, so `island_gen`
measures one at build time. Every above-sea segment of every channel — both
ends' water surface over zero — goes onto a uniform lattice one wetness range
across, widened from the centreline by the generator's own cross-section rule
so the distance runs from the water's edge rather than from the middle. Each
terrain vertex then reads one cell and takes the nearest segment's proximity: 1
at the water's edge, 0 at 12 m out from it or 3 m above the water beside it.
That is 1.67 M vertices against 273 segments in 8 ms at terrain size 1024, so
it rides on the generation task without a thread pool of its own.

The result travels in the free alpha channel of the terrain's vertex colours,
and `terrain.wgsl` squares it, breaks its edge with the metre-scale layer and
uses it to darken the ground a little and smooth its roughness — a quarter of
the way, against the tideline's near half. Damp banks, not black stripes.

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

## Vegetation

Trees and bushes keep one merged mesh each and one shared white
`StandardMaterial`. Bark, canopy and shrub tones ride in the mesh's vertex
colours, which is what leaves the trunk bark coloured under a material that
knows nothing about either. A shared material handle cannot carry a
per-instance tint, so per-plant variation is baked into eight meshes per class,
selected by the same deterministic hash as scale and yaw; each class batches
once per variant rather than once in total.

A variant is a shape and a tone together, and both shapes are built rather than
loaded — nothing in this crate reads an asset file. A tree is either three cone
tiers of falling radius or four merged lobes, over a trunk the variant's own
hash leans by up to six degrees and spreads the tiers or lobes by up to a
seventh either way. A bush is two or three flattened lobes. Both tree shapes are
built inside one seventeen-metre envelope, which is the cone that stands in for
them at range.

| Tier | Range | Tree | Bush |
| --- | --- | --- | --- |
| Near | 0 to 220 m | 187 vertices | 135 vertices |
| Far | 220 m to 3.2 km | 29 vertices | 28 vertices |

Every plant is two entities at the same transform, one per tier, each carrying
the `VisibilityRange` that hands over to the other; Bevy dithers across the 30 m
they share, so the swap has no frame it happens on. The far end is a backstop
against a camera taken out to sea rather than a working cull — `overview` stands
1.7 km off the island's centre and 3.1 km from its far shore, and frames every
plant on it.

The near tier casts into the shadow cascades and the far tier does not, because
a canopy 220 m out casts less than a pixel and there are thousands of them.
Contact shadows go on seating both.

At terrain size 1024 the island carries 1904 trees and 1936 bushes, so the class
is 7,680 entities against the 3,840 one tier would be. From `overview`, where
every plant is past the handover, that is 109 k vertices where drawing the full
mesh everywhere would be 520 k. Neither is measurable beside the terrain's own
1.67 M: the frame sits at 25 ms on an M3 Pro at 2560x1440 with the tiers on, off,
and with vegetation shadows on or off. The tiers and the shadow range are
headroom for denser planting, not a saving the current density needs.
