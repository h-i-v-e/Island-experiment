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
builds on a background task pool. Press `H`, or the `☰` button in the corner,
for the menu panel: ten showcase presets and every generator parameter, which
generate a new island without a restart, and `F` to walk on the one you have.

Options are applied in the order given, so a later one wins: `--variant eroded
--max-height 0.3` keeps the variant's erosion and takes the spelled-out height.

| Option | Default | Notes |
| --- | --- | --- |
| `--seed <N>` | `666` | Generation seed. |
| `--terrain-size <N>` | `1024` | Delaunay seed-point count, 16 to 4096. The first generation takes roughly 30 s, then the cache makes repeat launches fast; `128` or `256` iterate quickly. |
| `--variant <NAME>` | `default` | Named generation variant; see below. |
| `--view <NAME>` | `overview` | Named camera pose to open on and to reset to; see below. |
| `--weather <NAME>` | `clear` | Named weather look: sun, haze, cloud and its ground shadow, mist and grade as one set; see below. |
| `--debug-view <NAME>` | `off` | Switch the terrain and water surfaces to one diagnostic channel; see below. |
| `--screenshot <PATH>` | — | Render one 2560×1440 PNG offscreen once the island has settled, write `<PATH>.txt` beside it, then exit. No window opens. |
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
background task generation would have run on, so no frame ever waits on it. The
terrain in it is the chunk grid: 64 squares, each at three levels of detail,
each level carrying its own mesh, material weights and wetness. That is what
took the entry for seed 666 at terrain size 1024 from 120.5 MB to 152.4 MB.
The HUD's rebuilds read it too, which is what brings a parameter set you have
already visited back in milliseconds.

An entry is only read when the seed and all fifteen options recorded in it
match the run asking for it exactly. The chunk grid's own divisions and skirt
depth are mixed into the key as well: neither is a generator option and neither
would otherwise retire an entry that holds ground cut into other squares.
Anything else — a missing, truncated,
oversized or otherwise damaged file, or one written by an earlier format
version — is a miss, and the island is generated and the entry rewritten. The
format version is mixed into the key as well as written into the entry, so
entries from before a bump are never even opened, let alone read as damage. The
current version is 6, which took the skirt off the outside of the chunk grid,
where there is no neighbour to close a seam with; 5 replaced the one
island-wide terrain mesh with the grid at three levels of detail, 4 added the
river drops and widened the wetness around a plunge pool, 3 added the
per-vertex river wetness and 2 the height grid. The log says which happened on every run and on every rebuild:

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
other terrain sizes, put different ground under them. `chunk-seam` is a
diagnostic pose rather than a subject and is described with the terrain grid
below.

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
| `chunk-seam` | A diagnostic pose rather than a subject: 2.7 km out, where the terrain grid's LOD 0 to LOD 1 handover falls across the middle of the island instead of past its far corner. | Same pose. |

### Weather

A weather look is a place rather than a slider. Each one names where the sun
stands and how bright it is, how much aerosol the air carries, what cloud is
over the island and how hard it shades the ground, where mist collects and how
thick it is, and the restrained grade the frame is finished with. `--weather`
selects one, the menu panel offers the same list, and every capture records
which was in force. An unrecognised name is rejected at parse time with the
valid ones listed.

`clear` is the renderer with none of it: no cloud layer, no fog volume, an
unmodified earth atmosphere and a neutral grade. It is the default, so every
capture taken before weather existed is still the baseline the rest are read
against, and `docs/captures/phase-i` at `clear` A/Bs directly against
`docs/captures/phase-h`, which A/Bs against `docs/captures/phase-e`.

| `--weather` | Sun | Air | Cloud | Mist | Grade |
| --- | --- | --- | --- | --- | --- |
| `clear` | Mid-morning, 38° up, raw sunlight | Standard earth | None | None | Neutral |
| `maritime` | 33° up, round to the north-west | Mie ×1.9 | 36% cover at 1600 m, 62% shadow | At falls only | Warm ground, cool shadows, highlights held down |
| `valley-mist` | 15° up, raking across the island | Mie ×2.2 | 16% cover at 2200 m, 34% shadow | Pooled in the drainage, and at falls | +0.45 EV, warm, shadows lifted |
| `overcast` | 64° up, undimmed above the deck | Mie ×3.2 | 96% cover at 1400 m, 90% shadow | Light in the hollows, and at falls | +0.40 EV, desaturated, shadows lifted |

`src/weather.rs` holds all four in one table — sun, aerosol, cloud, mist and
`ColorGrading` together — and `lighting`, `clouds`, `mist`, the camera and the
capture sidecar all read it. A look added there is added once.

The looks each move the sun, so shadows fall differently and the four are not
pixel comparisons of one another; they are four places. What they do share is
the island, which no look touches.

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
| `W` `A` `S` `D` | Move on the view plane, ramping to 140 m/s over a quarter second | Move along the ground relative to the view, at 4.5 m/s |
| `Shift` | Move down | Sprint, at 9 m/s |
| `Space` | Move up | Jump, about 1 m |
| Left mouse | Held: pan, dragging the ground along under the cursor | Click: take the cursor back after `Esc` |
| Right mouse | Held: turn the island on a turntable around the ground it was pointing at, cursor captured | — |
| Scroll wheel | Dolly along the view direction; 60 m per wheel line, less per trackpad pixel | — |
| `F` | Start walking | Take off |
| `R` | Return to the `--view` pose | Return to the `--view` pose, flying |
| `H` | Show or hide the menu panel | Show or hide it, handing the cursor over and taking it back |
| `Esc` | Release the cursor | Release the cursor; looking stops until you click |

`H` and `F` are the only two bindings walk mode and the panel add. The `☰` button
in the top-left corner does what `H` does, so the panel can also be opened and
closed with nothing but the mouse. While a panel field has the keyboard, or the
pointer is over the panel or that button, neither the camera nor either toggle
sees the input.

### Flying

The keys ask for a velocity rather than for a step, and the velocity the camera
is carrying moves towards it at 560 m/s² — a quarter second from rest to the
full 140 m/s, and the same quarter second back to rest when the key comes up. A
single frame's press therefore reaches about 9 m/s and coasts to a stop inside
0.16 m, where an unramped 220 m/s moved 3.7 m on that frame. The wheel is
unchanged. `R` and taking to foot both clear the velocity: a pose is a place,
not a heading with speed behind it.

The right button turns the island rather than the head. On the press, the view
direction is marched against the surface — the generated ground, or the sea
plane wherever that ground runs below it — and the first place it crosses
becomes the pivot the drag turns around. Pointed over the horizon, or grazing
flat enough to run out of range, it takes a point 400 m ahead dropped onto the
surface under it instead. That pivot is then held for the whole drag: one
re-read every frame would slide along the surface as the view moved and the turn
would wander off whatever it was started on.

From there the eye swings around the pivot, facing it throughout and keeping the
distance it had, so the drag rotates the scene and does nothing else. It reads
the way a hand on the landscape would: dragging right carries the island right,
dragging down tips it down and lifts the eye over it, and half a window width is
a quarter turn. The elevation is held between 5° and 85° over the pivot's
horizontal plane, so the view neither flips over the top nor dives under the
ground it is turning around; a drag begun from outside that band — from a beach
looking up at a summit — is led back towards it rather than snapped into it,
which would throw the eye hundreds of metres on the press.

The wheel still dollies mid-drag, and shortening the arm that way is the one
thing that changes the distance. Releasing the button reads the yaw and pitch
back off the pose the orbit left and clears the velocity, so the keys resume
from where the island was put down rather than from where the drag started.

Flying will not go through the ground. After everything that moves the eye —
keys, pan, wheel, `R` — the camera is floored 2 m over the ground beneath it,
read off the same 512-square height grid walking stands on, and never under 2 m
over sea level, because the sea is a surface to clear as well and the shelf
under it is not ground to fly along. Downward speed is spent at the floor rather
than carried. A capture run is left alone: its pose is what `--view` names and
what the sidecar records, and lifting the eye off it would make the image
disagree with its own metadata.

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
- Opening the menu panel releases it for as long as the panel is up, since
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

## Menu panel

The interface is four areas anchored to the screen rather than one tall column,
so the middle of the frame — which is the island — is never behind a control.

- **Top left**, always: a `☰` button. It is the whole of the mouse-only path in
  and out of the panel, and it does not move when the panel opens, so it is
  always in the same place.
- **Left, under it**: the menu panel, which `H` and that button show and hide.
- **Top centre**, only while there is something to say: the generation strip.
- **Bottom left**, always: the frame rate and the draw census.

The panel is three collapsing sections over a pinned footer. The sections scroll
together inside whatever height the window leaves between the toggle above them
and the footer below, so the two buttons that act on a draft are never the part
that runs off the bottom of a short window.

**Showcase** is ten curated islands, two buttons to a row, each with a line on
what it is under the pointer. A click fills the draft in and asks for the
rebuild in the same frame — the point of a preset is that it is one press — and
the button stays lit while the island on screen is that preset. A preset does
not carry `terrain_size`: it opens at whatever size the viewer is already
running, so the same ten can be flipped through in seconds at 256 and revisited
at 1024 later. `src/presets.rs` is the table, and `docs/captures/showcase/` is
an overview of each at 1024 with its parameters written out.

**Parameters** is the seed and all fifteen generator parameters, so an island
can also be hunted for by hand. Sliders span the range each parameter is useful
over — the generator's own limits where it validates one, working ranges
otherwise — and each is labelled with the flag that reproduces it.

**Look** carries the **weather** and **debug view** dropdowns, each with the
same list its flag takes, so a look or a channel found by dragging can be
captured by name. Neither regenerates anything: a debug view is a uniform the
surfaces read, and a weather look moves the sun, the air, the cloud and the mist
over an island that does not change. Switching looks rebuilds the cloud field
and replaces the fog volumes in the same frame.

Nothing regenerates while a slider moves. Generation takes seconds to a couple
of minutes, so the panel edits a draft and **Regenerate** hands the whole draft
over at once. The island on screen stays up the whole time; when the new one
lands, every entity the old one spawned is despawned and the new set is spawned
in the same frame. **Copy command** puts the full argument list that reproduces
the island on screen onto the system clipboard — the same line the log carries
on every build:

```
island arguments: --seed 666 --terrain-size 128 --max-height 0.2 …
```

Under the two buttons is what the island on screen was built from and how long
it took. While a build is running the strip at the top centre counts the seconds
instead, and it is not part of the panel, so a rebuild asked for and then hidden
still reports itself. A rebuild whose parameters the generator rejects leaves the
island on screen alone and reports the error in the same strip and on the panel;
only a first island that cannot be generated is fatal.

Rebuilds read the cache like any other generation, so a parameter set you have
already visited — a preset second time round, for instance — comes back in tens
of milliseconds rather than seconds.

The bottom-left readout is frames per second, smoothed and refreshed twice a
second, and under it the census of what the culling stages left for that frame —
terrain chunks drawn against chunks that exist, the vertices behind them, and
scatter instances drawn against instances that exist. It is always on and is not
part of the `H` toggle. `--screenshot` logs the same census, and the mean frame
time over its settle, on every run.

Everything is drawn on one dark translucent palette rather than on egui's own
grey-on-grey: a deep-water ground at 226 of 255 alpha, a shoal-blue accent on
every hovered and active control, hairline outlines in the same blue, and text a
stop under white, which is what keeps a panel held against a bright horizon from
glaring. The theme is written into both of egui's themes and the context is
pinned dark, so what the host reports as its preference cannot put a light panel
over the sea.

The panel is drawn with [`bevy_egui`](https://crates.io/crates/bevy_egui), the
crate's only dependency outside `bevy` and `island-rs`.

## Capture harness

```sh
cargo run --release -- --view river-level4 --screenshot island.png
```

The app waits for the island, holds a fixed count of frames while the
atmosphere, occlusion, contact-shadow and bloom pipelines compile and the
temporal resolve converges, writes the PNG and the metadata beside it, and
exits. A non-zero exit code means the capture never landed.

A capture run puts nothing on screen at all. No primary window is asked for, so
winit's event loop is left out of the run with it, `ScheduleRunnerPlugin` drives
the frames instead, and the camera renders into a 2560×1440 offscreen image that
`Screenshot::image` reads back. Neither the panel nor the frame-rate readout is
built either, so no capture can carry them whatever the `H` toggle was last left
at. A capture cannot interrupt what is in front of it, and nothing in the run
registers with the window server.

That also settles a failure the old path could not avoid. A capture used to read
the window's own surface back, and macOS only keeps a surface current while the
window is composited, so `Window { visible: false, .. }`, minimizing it and
`WindowLevel::AlwaysOnBottom` each produced a valid PNG of solid black — each
tried on its own — and opening the window unfocused was as far as it could go.
An offscreen image has no compositor behind it to keep anything current, so
there is no longer a state the capture can be taken in that returns black.

The whole image stack runs offscreen unchanged: HDR, ACES, atmosphere and its
environment map, temporal anti-aliasing, screen-space ambient occlusion, contact
shadows and bloom, and the debug channels with them. A headless capture of
`--view overview` differs from the windowed capture of the same pose in
`docs/captures/phase-e` on 0.67 per cent of pixels, none of them by more than
ten steps in 255, and the two sidecars are byte-identical.

Offscreen frames cost more than presented ones. At 2560×1440 the settle runs at
a little over 40 frames a second on an M3 Pro where the windowed viewer reports
64, so `--screenshot` spends around 7 s on its 320 frames rather than the 5 a
window would take.

### Repeating a capture

The point of the harness is that one command run twice produces the same image
twice, so a diff between two captures is a diff between two renderers.

Two things used to stop it. The first was the water: both water shaders animated
from `globals.time`, so every crest, streak and foam edge stood wherever the
wall clock had left it. They now read a water clock the app owns — a resource one
system advances by the frame delta on an ordinary run, and `--screenshot` pins
at 27.5 seconds. That value puts both time-only noise axes mid-cell rather than
on a lattice plane, and every drifting layer several wavelengths along, so a
capture catches waves and surf mid-phase rather than the still field the shaders
start from. Nothing about how the water looks in the viewer changed.

The cloud layer drifts on the same clock and is frozen by the same freeze, so a
capture under a named look finds the cloud — and the shadow it lays on the
ground — exactly where the last capture of that command left them.

The second was the settle, which held for 120 frames *and* 5 wall-clock seconds
and so depended on how fast those frames ran. It is now 320 frames after the
island appears and nothing else — except that the capture also waits for the
frame counter to reach a multiple of 64. Four sequences in the render stack are
indexed by frame number: the temporal jitter walks 8 Halton offsets, its history
ping-pongs between 2 textures, the occlusion pass steps through 64 noise offsets
and the contact shadows read 32 layers of a blue-noise volume. Sixty-four frames
return all four to where they started, and generation takes as long as it takes,
so waiting for a multiple of it is what stands two captures at the same point in
every one of them. The dithered vegetation LOD needs no help: its stipple is a
fixed 4×4 threshold map over the fragment coordinate and the camera distance,
both of which a named pose fixes.

What that does not close is the temporal resolve, whose history is the whole run
of frames rather than this one. Through a window, a frame the window system
declined to present left a mark on it that decayed without quite going: two
captures of `--view stream --terrain-size 128` on an M3 Pro agreed on the sky
and the water and differed on about five per cent of the lit ground and foliage
edges, by a single step in 255 for almost all of them and by no more than 17 for
any. Rendering offscreen took that with it, because there is no longer anything
that can decline a frame. Three captures of the same command, compared each
against each, differ on between 0.04 and 0.07 per cent of pixels and by no more
than 8 steps. What is left is not a transient a longer warm-up outlasts, so 320
frames is still set for the pipeline compiles rather than against it.

### Capture metadata

Every `--screenshot` writes `<PATH>.txt` beside the PNG: one `key: value` per
line, always the same keys in the same order, so two sidecars diff to what
actually differs about the two captures.

```
crate: island-bevy 0.1.0
seed: 666
terrain-size: 128
variant: default
non-default-options: none
view: stream
eye: -531.5, 6.3, 339
target: -524, 5.8, 324
weather: clear
debug-view: off
sun-direction: -0.48, -0.62, -0.62
exposure-ev100: 13.5
water-clock-seconds: 27.5
warm-up-frames: 320
capture-frame-period: 64
renderer: hdr, aces-fitted, atmosphere, taa, ssao, contact-shadows, bloom
adapter: Apple M3 Pro (metal)
resolution: 2560x1440
```

`weather` names the look, and `sun-direction` and `renderer` both follow it: a
look decides where the frame was lit from and what was in the image stack.
Under `clear` the stack is exactly the camera's own seven entries, which is what
keeps a sidecar written before weather existed comparable with one written
after; a named look appends `clouds`, `cloud-shadows`, `volumetric-fog` and
`colour-grading` as it uses them.

`non-default-options` is the `--flag value` pairs this island's generator
options differ from the defaults by, or `none`; the full reproducing line is
what the HUD and the log already print. `adapter` comes from
`RenderAdapterInfo`, and `resolution` is the offscreen target's size, stated in
`screenshot.rs` rather than read off a window. It is the 2560×1440 the sidecars
already recorded — the physical size a 1280×720 window came back as on a Retina
display — and keeping the aspect ratio the viewer opens at is what keeps a
`--view` framing the same thing on screen as in its capture.

### Debug views

`--debug-view <NAME>` switches the terrain and water fragment stages to one
diagnostic channel; the menu panel carries the same list, so a channel
found by dragging can be captured by name. An unrecognised name is rejected at
parse time with the valid ones listed.

| `--debug-view` | Surface | Shows |
| --- | --- | --- |
| `off` | — | Ordinary shading. |
| `weights` | Terrain | The generator's material triple as red, green and blue: bedrock hardness, loose cover, sea proximity. |
| `wetness` | Terrain | The per-vertex proximity to running water, as the scalar ramp. |
| `slope` | Terrain | How far off level the surface stands, as the scalar ramp. |
| `flow` | River | The downstream heading in red and blue, the speed it is travelled at in green. |
| `grade` | River | The surface grade in red and the whitening a running reach takes from it in green, after the bank-rim clearance. |
| `depth` | Sea and river | The optical depth the absorption is taken over, as the scalar ramp. |
| `state` | River | Which of the four water states the surface is in: calm blue, running green, plunge orange, falling white. |
| `foamless` | River | Ordinary shading with every foam contribution removed, which is where a fall has to still read as a body of water. |
| `chunks` | Terrain | Which square of the terrain grid the ground belongs to and which level of detail is drawing it: green for LOD 0, amber for LOD 1, red for LOD 2, each chunk's square in two tones so the seams between them can be read. |

Each surface answers only the views that name a channel it carries and shades
normally under the rest, so a terrain channel is still seen through the water
standing over it and a water channel still stands on shaded ground. Water
channels are written opaque, or a diagnostic of the water would arrive blended
with the bottom it is measuring. The one exception is the spray: it is a mist
rather than a surface and carries no channel of its own, so any channel at all
takes the whole cloud out of the frame instead of leaving a diagnostic of the
water at the foot of a fall to be read through it.

The channel is a `u32` on each material extension, written every frame by
`src/capture.rs` alongside the water clock; zero is ordinary shading, which is
what a material carries before anything writes to it. The scalar ramp runs
black, blue, green, white rather than through grey, because a diagnostic goes
through the same exposure and ACES curve the scene does and a grey ramp loses
most of its bottom half to that. For the same reason these read as a tone curve
over the channel rather than as raw float values; what they are for is
comparing, locating and diffing, not measuring.

Debug views work under `--screenshot`, which is their main use, and the sidecar
records which one was active.

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

Where the sun stands, what it carries and what the air between is made of all
come from the named weather look. A look scales the earth medium's Mie term and
leaves Rayleigh and ozone alone: what separates a maritime haze from a clear
day is aerosol, and Mie is the term that carries it. It also shortens the
aerial-perspective lookup from Bevy's 32 km to 16 km, which is what resolves the
gradient across a two-kilometre island instead of spreading two of the lookup's
32 slices over the whole of it.

### Clouds and their shadows

The cloud layer and the shadow it lays on the ground are one field read two
ways, which is the only arrangement in which they cannot drift apart.

The field is a tiling four-octave value-noise sum, built on the CPU from the
same hash the rest of the crate uses into a 512×512 single-channel image, once
per look. Its threshold is read off a histogram of its own samples rather than
assumed, so a look asking for a third of the sky gets a third of the sky: a sum
of four octaves is banked hard around the middle of its range and a naive
threshold of `1 - coverage` would deliver a fraction of what was asked for.

That image goes to the sun as a `DirectionalLightTexture`, which Bevy projects
along the light and multiplies into the *direct* term of every lit fragment in
the scene. Skylight is untouched, so a shadowed slope goes soft rather than
black — which is the physically right behaviour and the reason this route was
chosen over the alternatives. Nothing in `terrain.wgsl`, `rock.wgsl`,
`ocean.wgsl`, `river.wgsl` or the vegetation's plain `StandardMaterial` changed
at all: foliage, rock, sand, snow and both waters take the shadow through
Bevy's own lighting path. Modulating the sun inside our own four shaders would
have meant editing four fragment stages, giving vegetation a material extension
it does not otherwise need, and approximating a direct-only term by darkening
albedo, which also darkens ambient.

The same image is sampled again by the layer in the sky, through the same
projection. A directional light texture is mapped by the inverse of the light's
own transform, so its local XY plane is square to the sunlight and every point
on one sun ray lands on the same texel whatever height it stands at. Registering
the layer with the ground shadow therefore needs no altitude arithmetic: the
layer reads the field at its own world position through the same basis, and the
ground below it along the ray reads the identical texel. Drift is the light's
translation, which a directional light takes nothing else from — the cascades
are built from its rotation alone — so moving the sun sideways moves the cloud
and its shadow together, with no second copy of the offset to keep in step. That
translation is written from the water clock, so `--screenshot` freezes the
layer exactly as it freezes a crest.

What the image cannot carry is fine detail: a cloud a kilometre and a half up
has lost its edges to the sun's own angular size long before its shadow reaches
the ground. So the layer adds a finer noise of its own in the fragment stage
and the ground shadow stays the smooth field. The two disagreeing at that scale
is the physical answer rather than a compromise.

Two things this needed that are worth writing down. Both light-texture Cargo
features have to be on — `pbr_clustered_decals` builds the binding array light
textures ride in, and `pbr_light_textures` is what makes the lighting stage
sample it; with only the first the component is extracted and then silently
never read. And the field image is four channels for a value that needs one,
with the other three zero: 0.19.1 takes its GPU decal count from the length of
the whole buffer that light textures share with clustered decals, so the sun's
own texture is also rasterised into the clusters and composited as a base-colour
decal by every fragment going through the stock standard material. A
single-channel image arrives there as opaque red and turns every tree on the
island with it. Alpha is what that pass blends by and what the light path never
reads, so an alpha of zero makes it a no-op.

### Mist

Local mist is `FogVolume`s under a camera-side `VolumetricFog`, and neither kind
is hand-placed.

Valley mist is found in the height grid the generator already hands over for
walking on. The island is divided into 24 coarse cells a side, and a cell whose
lowest ground stands above the sea, stands under the elevation the island's
valleys stop at, and stands well below the ground in the ring of cells around
it, is a hollow — which is where the drainage runs and where mist collects on a
still morning. The dozen best-scoring hollows get a volume each. Waterfall mist
comes from the drops the river pass already found, one volume at each foot,
scaled by the same strength the spray cloud and the wet rock around it are built
from.

A fog volume is a box of uniform density otherwise, and a box is exactly what it
looks like. All of them therefore share one 32³ density volume built the same
way the cloud field is: noise inside an envelope that reaches zero at all six
faces, so what stands in the valley is a soft pool and never the shape of the
box it was cut from.

Two calibrations were needed and both are worth stating. Density is read against
optical depth — the depth across a volume is density times absorption plus
scattering times the metres the ray crosses — so a hundred-metre valley pool is
already thick at a density of 0.02 and opaque anywhere near one. And the
volumetric pass multiplies its in-scattering by the camera's exposure and then
composites into the frame the main pass wrote, which has not been exposed yet;
in a scene lit at 130 000 lux the mist therefore arrives a whole exposure short
and can only ever darken what it stands in front of. `FogVolume::light_intensity`
divides that back out, and the look's own `glow` is then the share of physical
in-scattering it wants. The camera's `ambient_intensity` is left at zero: that
term is added over the whole of a volume's box without consulting the density
texture, and it is the one thing that can draw a volume's own silhouette across
the terrain.

### Colour direction

Each look carries a `ColorGrading` on the camera, applied just before tone
mapping. The values are deliberately small and are the review's three asks, in
the only terms `ColorGrading` offers: shadow saturation up and shadow lift up a
few thousandths, so skylit facets read cool and keep detail rather than going
featureless; midtone saturation and gain up slightly, so vegetation separates
from beige rock; highlight saturation, gain and gamma pulled down, so snow and
pale rock keep structure instead of clipping. Global temperature and tint are
the only per-channel controls the component has, and they carry the warm or cool
cast of the look as a whole.

There is no vignette and no chromatic aberration, and bloom is unchanged from
what it was. `clear` is ungraded by definition, so the colour direction is what
separates the three named looks from it; `maritime` against `clear` on
`overview` is the direct A/B for the grade alone.

### Screen-space reflections

Evaluated and not adopted. Bevy's `ScreenSpaceReflections` requires deferred
rendering — it inserts `DeferredPrepass` as a required component, and its render
node runs between deferred lighting and the main opaque pass. The sea and the
rivers are `AlphaMode::Blend` and are drawn in the transparent pass, which runs
after both, so a reflection pass can never reach them; and all five surface
materials are `ExtendedMaterial`s that replace only the *forward* fragment
stage, so putting the camera into deferred draws them through the stock deferred
path instead. Tried on `river-level4`, that is exactly what happens: the terrain
loses every band, detail layer and wet bank and comes back as flat paint, the
river rock comes back black, the water loses its depth absorption — and there is
no reflection on the water anywhere, because the water was never in the pass
that could have produced one. Making this work would mean giving all five
materials a deferred fragment stage and moving the sea onto an opaque path,
which is a redesign of the water composition rather than a prototype, so it is
out of scope here and recorded as evaluated.

## Coordinates

island-rs is right-handed and Z-up with XY normalized to `[0, 1]` and sea level
at `z == 0`; Bevy is Y-up. Each vertex crosses as
`(x, y, z) -> ((x - 0.5) * S, z * S, (y - 0.5) * S)` with
`S = motu::ISLAND_WORLD_METRES`, normals as `(n.x, n.z, n.y)`, and every
triangle reversed because the Y/Z swap flips handedness. This matches the Unity
importer.

## Terrain grid

The terrain is a fixed grid of chunks rather than one island-wide mesh: 64
squares of 250 m on an 8x8 grid, each carrying the same ground at all three
levels of detail the generator publishes, spawned as 192 entities. One entity is
drawn whole or not at all, so an island-wide mesh could not leave out the half
of the island behind the camera; a grid can, and at `river-level4` 161 of the
192 entities are culled.

The chunks are cut once, on the generation task, by the generator's own
`Mesh::sliced_grid`, and go into the cache with everything else. Nothing is
sliced at run time and nothing streams.

| Level | Vertices | Drawn from | Crossfade |
| --- | --- | --- | --- |
| LOD 0 | 1 868 246 | 0 | 2400 to 2480 m |
| LOD 1 | 255 070 | 2400 m | 5000 to 5160 m |
| LOD 2 | 36 115 | 5000 m | 60 to 80 km |

The three levels of one chunk hand over through `VisibilityRange`, which Bevy
dithers across the margin two of them share, so a level change has no frame it
happens on.

**Where they hand over was measured, not chosen.** LOD 1 has 9.7 times fewer
vertices than LOD 0 — 3.1 times coarser along an edge — and a LOD 1 chunk
boundary stands 0.46 m from the LOD 0 surface on average. Held against phase H
frame for frame at 2560x1440, a 900 m handover leaves 1.45 per cent of the
`overview` pixels differing by more than 16 steps in 255, a 1500 m one 0.37 per
cent, and a 2400 m one 0.002 per cent. On a two-kilometre island at this
resolution the generator's LOD 1 therefore does not become invisible until
further away than the island's own diagonal: at 2400 m it engages only at
`overview`'s far corner, and LOD 2 not at all. The levels are headroom for a
larger terrain or a camera taken out to sea, which is the same role the
vegetation impostor's 3.2 km backstop already has. `--view chunk-seam` is a pose
that stands far enough out to put the frontier across the middle of the island,
and `--debug-view chunks` colours it.

**Every level of one chunk stands at the same point**, `chunk::origin`, with its
vertices relative to it — the one place in the crate where geometry is not left
in world space. Bevy reads the crossfade distance off the entity's translation
in both the culling stage and the shader, so geometry left at the origin makes
every chunk answer that test as if it were the island's centre, and the shader
then discards chunks the culling stage kept.

### Seams

Chunks are sliced without any boundary clamping. One source-triangle pass is
clipped into every tile at once, so two chunks at the same level contain
identical boundary points and meet exactly — the near view, where every chunk is
LOD 0, has no seam treatment at all and no ground pulled onto a coarser profile.

Two chunks at different levels do not meet, and a skirt closes them: a vertical
apron hung 24 m below every interior chunk edge, carrying the normal, the UV,
the material weights and the wetness of the vertex it hangs from, drawn on both
faces. Whichever surface stands higher fills the gap under it with ground shaded
exactly as the ground above it is. Twenty-four metres is the worst step any two
adjacent levels can leave — the worst LOD 1 boundary vertex on this island
stands 19.0 m off LOD 2 — with a margin, and the margin costs nothing.

Where the two surfaces do meet the apron is not visible, and that is a property
of the geometry rather than a hope: the terrain is a height field, so any ray
from a camera above it that reaches a point below a shared edge has already
crossed the surface on the near side. The four sides on the outside of the grid
get no apron, because there is no neighbour there and an apron would only stand
proud of the terrain square's own edge.

The two handovers are far enough apart that a LOD 0 chunk can never touch a
LOD 2 one — 2520 m between the bands against a 354 m chunk diagonal — so only
two adjacent levels ever meet at a seam, which is what the 24 m is sized
against.

The grid costs vertices twice over: a boundary vertex belongs to both chunks
that share it, and every interior edge grows its apron. At LOD 0 that is 5.9 per
cent for the duplication and 6.0 for the skirt; all three levels together are
2.16 M vertices resident against the single mesh's 1.67 M.

## Surface materials

Terrain, river rocks, the sea, the rivers and the spray a fall throws are each
drawn by an `ExtendedMaterial<StandardMaterial, _>` whose WGSL lives beside the
source in `src/` and is embedded in the binary. Extending rather than replacing
`StandardMaterial` keeps the shadow
cascades, the depth and motion-vector prepasses, screen-space occlusion,
contact shadows and aerial perspective working; only the forward stage is the
crate's own, and only the spray replaces the vertex half of it. There are no
texture assets: every detail layer is hash-lattice value noise evaluated in
world space, from `src/noise.wgsl`.

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
at the water's edge, 0 at 12 m out from it or 3 m above the water beside it —
and never less than what the spray of a nearby fall leaves, which is measured
on a lattice of its own beside it. That measurement is per vertex and keys off
nothing but the position, so it is taken on the chunks rather than on the whole
island and answers the same either way; slicing all three levels, hanging the
skirts and measuring both this and the material triple on all 2.16 M resulting
vertices takes 1.4 s at terrain size 1024, once, on the generation task and
never again because the result is what goes in the cache.

The result travels in the free alpha channel of the terrain's vertex colours,
and `terrain.wgsl` squares it, breaks its edge with the metre-scale layer and
uses it to darken the ground a little and smooth its roughness — a quarter of
the way, against the tideline's near half. Damp banks, not black stripes.

The merged river-rock body is 6–22 cm stones with the occasional 65 cm boulder.
`convert::rock_mesh` hashes world position on a 20 cm lattice into a per-body
albedo tint, which is as close to per-instance as one merged mesh allows, and
puts the same spray measurement in the one channel that tint leaves free.
`src/rock.wgsl` adds mineral colour, roughness variation and centimetre relief
that fades out past 25 m, and darkens and smooths whatever a fall is wetting.

## Water materials

Both waters take their motion from a water clock the app owns rather than from
the renderer's own `globals.time`, so a capture can freeze it; see the capture
harness above.

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

Surf needs two more things. A bottom that shoals, because shallow water
standing against a wall is not a wave running out of water. And a swell
actually arriving here: surf comes in stretches as long as the swell itself,
with gaps of the same length between them and a shorter layer breaking each
stretch up, so a beach several hundred metres long carries several of them and
a cove twenty metres across carries part of one. Between them that is what
stopped every cove and plunge pool on the island from being outlined in white.

`src/river.wgsl` runs on the generator's channel parametrisation instead:
`uv.y` is distance travelled downstream and `uv.x` is distance to the nearest
bank, both normalized island units. Screen derivatives of the downstream
coordinate recover the flow direction in the world, which per-vertex tangents
cannot because a bank distance turns over at the centreline. That same turn is
why nothing but the bank behaviour itself is sampled on it: a layer read on a
bank distance is mirrored about the channel's own centreline, and on anything
wider than its lateral wavelength the mirror reads as a chevron. The layers use
signed lateral world position, wrapped every 256 m to keep the noise lattice
inside the range its hash still separates.

Fresh water absorbs at well under half the sea's rate, so beds and stones stay
readable, and the surface fades out over the last handspan of bank distance.

### Water states

A river is not one material with more foam on it. `river.wgsl` resolves four
states per fragment and blends between them:

| State | What puts it there | What it does |
| --- | --- | --- |
| Calm | whatever the other three leave | clear water, no foam at all |
| Running | surface grade, or the draw towards a lip | flow layers, foam only on a real rapid |
| Falling | the drop field's sheet channel | streaks, thickness, torn edges, aeration |
| Plunge | the drop field's foot channel | churning normals and foam that leaves downstream |

Grade alone never promotes water to falling. A pool's own rim tilts up to meet
its bank and has the grade of a waterfall with none of the water; phase D read
exactly that as one and drew an unbroken white contour around every pool.

Foam has three sources and no fourth — a sheet aerating as it falls, the water
that receives it, and a reach steep enough to break on its own — and each is
gated by the state that produces it, so calm water has no foam rather than a
little. The grade-driven one is refused within 85 cm of a bank for the same
reason the rim is not a fall.

`--debug-view state` colours the four; `--debug-view foamless` removes every
foam contribution, which is where a fall has to still read as a body of water.
It does, because what carries it is the sheet's own opacity, its thickness
variation and the pale tone aerated water takes — not the white.

### Drops

The generator publishes channels, not falls, so `island_gen` derives them from
the river node profile at build time: a run of consecutive segments whose water
surface falls at least 75 cm, and at least half a metre for every metre it
travels, is one drop, with the first node its lip and the last its foot.
`Island::river_emitters` was the other candidate and was not used — it finds
every sharp crease on the river mesh, which on this generator is as often
angular channel topology as it is a lip, and a crease cannot say which side of
a fall it is on. A node profile can, and it hands over the fall's height and
the channel's own width there as well. Seed 666 at terrain size 1024 has 19
drops, the tallest 8.1 m; the eroded variant of the same seed has 3, because it
keeps a fifth as much channel above the sea.

The drops themselves are cached; everything derived from them is not. At spawn
`convert::river_mesh` writes four numbers per water vertex into the mesh's
colour attribute, which the generator leaves free on that mesh: the approach to
a lip, how much falling sheet is at the vertex and how far down the face it
stands, the plunge below a foot, and the fall's own height. `convert::rock_mesh`
writes how much spray stands on each boulder into the one channel its tint
leaves free, and `rock.wgsl` darkens the stone and smooths its roughness from
it, with the rock grain breaking the edge so what ends is wet stone and not a
disc laid over it. Ground beside a fall takes the wetter of the bank measurement
and the same spray, so a plunge pool reads as one damp hollow.

### Spray

`src/spray.rs` builds one merged mesh of camera-facing quads, four vertices per
droplet, and `src/spray.wgsl` throws each one on the GPU: launch point in the
position attribute, launch velocity in the normal, quad corner in the UV, and
phase, size, life and brightness in the colour. The vertex stage evaluates a
ballistic arc from the same water clock both water surfaces animate on, so
there is no per-frame CPU work, no buffer rewritten between frames, and a
frozen clock freezes the cloud exactly as it freezes a crest. Everything a
droplet is comes from hashing its own index against the drop it belongs to.

The cloud blends, so like both waters it never reaches the prepass — which is
also what lets its vertex stage move a droplet with no prepass to keep in step.
It casts no shadow, its bounds are given rather than derived from the launch
points it leaves, and it is restrained on purpose: a hundred and ninety faint
droplets on the biggest fall on the island, none of them opaque enough to be
seen on its own.

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
1.87 M. The tiers and the shadow range are headroom for denser planting, not a
saving the current density needs.

### Groups

The plants are not loose in the world. Each tier of each square of the terrain
grid has a parent entity, and every plant hangs off the parent for its square
and tier — 94 parents over the 47 squares that have anything planted on them.
A parent carries the sphere around its plants' origins and the range its tier is
drawn over, and a system hides it when the two cannot reach each other. Bevy's
visibility propagation then skips the whole subtree instead of testing thousands
of instances one at a time.

The test is exact rather than approximate, so no plant that would have been
drawn is ever hidden: Bevy measures a plant's range against its own translation,
which is exactly what the sphere bounds, and the shadow passes take the same
range from the same camera. The instance counts are identical to what they were
before the grouping, to the plant.

| | Groups kept | Instances drawn |
| --- | --- | --- |
| `overview` | 47 of 94 | 3839 of 7680 |
| `mountain` | 49 of 94 | 2486 of 7680 |
| `stream` | 52 of 94 | 1963 of 7680 |
| `river-level4` | 53 of 94 | 1073 of 7680 |

Nearly every hidden group is a near-tier one, because the full meshes stop at
250 m and only a handful of squares can hold one from any pose. The impostor
groups mostly stay up, which is the same fact as their 3.2 km backstop being a
backstop rather than a cull. The spray is one merged entity per island and needs
no grouping.

## Budgets

The quality tier this crate has is one: an M3 Pro at 2560x1440, offscreen, with
the whole render stack on. Frame times below are the mean over a capture's
settle once the pipelines have compiled, which `--screenshot` logs on every run
beside the census of what the culling stages left standing. The viewer's
bottom-left readout carries the same census live.

The structural half does not depend on the machine, and is the half the terrain
grid was built for:

| View | Chunks drawn | Chunks culled | Terrain vertices drawn | Resident |
| --- | --- | --- | --- | --- |
| `overview` | 65 | 127 | 1 801 273 | 2 159 431 |
| `mountain` | 43 | 149 | 1 595 315 | 2 159 431 |
| `river-region` | 43 | 149 | 1 514 149 | 2 159 431 |
| `river-ground` | 35 | 157 | 1 431 623 | 2 159 431 |
| `river-level4` | 31 | 161 | 1 260 148 | 2 159 431 |
| `stream` | 36 | 156 | 1 309 863 | 2 159 431 |
| `chunk-seam` | 66 | 126 | 725 463 | 2 159 431 |

Before the grid every one of those rows read one entity and 1 670 192 vertices,
in every pose. The close views now draw between 58 and 68 per cent of what is
resident and a sixth of the entities.

The timing half is a wash, and that is worth saying plainly. Best of nine
interleaved passes with the two binaries run back to back:

| View | Before the grid | After |
| --- | --- | --- |
| `overview` | 32.1 ms | 33.5 ms |
| `mountain` | 32.2 ms | 31.6 ms |
| `river-region` | 31.9 ms | 34.1 ms |
| `river-ground` | 30.0 ms | 31.0 ms |
| `river-level4` | 30.0 ms | 29.7 ms |
| `stream` | 33.7 ms | 32.8 ms |

Between −0.9 ms and +2.2 ms, which is inside the machine's own spread: passes of
the *unchanged* renderer at `overview` ranged from 32.1 ms to 71.4 ms over one
afternoon depending on what else was running. Taking a third of the terrain
vertices out of the frame recovers about a millisecond, because the frame is not
terrain-bound. Toggling one thing at a time on a quiet machine says where it
does go:

| | `overview` | `stream` |
| --- | --- | --- |
| As shipped | 29.0 ms | 28.4 ms |
| Shadow cascades off | 24.8 ms | 24.3 ms |
| Contact shadows off | 27.9 ms | 26.6 ms |
| Cascade range 5 km → 1.2 km | 27.7 ms | 29.6 ms |
| Abrupt level handover, no dither | 25.6 ms | 27.6 ms |
| Occlusion culling on | 28.9 ms | 28.9 ms |

Shadows are the largest single item — 4 to 6 ms for the four cascades, and 3 ms
more for contact shadows at ground level. Both stay.

**Bounding the cascades by distance does not pay.** Cutting the cascade range to
1.2 km buys 1.3 ms at `overview`, nothing at `stream`, and costs every shadow
the massif casts across the plain. The scatter's shadow range is already bounded
— the impostor tier has been out of the shadow passes since it existed.

**The level crossfade costs 1 to 3 ms**, because an entity with a dithered
`VisibilityRange` compiles a `discard` into its fragment shader and a tile-based
GPU gives up early depth rejection for it. It stays: without it a whole 250 m
chunk changes level in one frame and the temporal resolve sees that. The
measurement is here because it is the one number in this crate that could
reasonably be spent the other way.

**Occlusion culling is not adopted.** Bevy 0.19's `OcclusionCulling` on the
camera, with the depth prepass TAA already requires, measured 0.5 to 1.3 ms
*slower* at every pose tried. Two-phase occlusion culling splits the depth
prepass in two and builds a depth pyramid between them, and an island has few
large occluders and a prepass that already rejects what is behind them. It is a
one-component change if the scene later grows geometry that repays it.

**Texture compression, mip generation and anisotropy have nothing to validate.**
There are no texture assets in this crate: no file is loaded from disk, every
surface is procedural in WGSL, and the only images are the cloud field and the
mist density volume, both generated at startup and both single-mip by
construction. There is nothing to compress and no sampler whose anisotropy could
be wrong. That holds until an authored asset library exists.
