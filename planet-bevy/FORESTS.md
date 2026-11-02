# Dense radial forests

**Forests → Forest density** controls placement from 0.25–4, with a default of
2. The value is saved with the viewer settings; `--forest-density D` overrides
it. Old settings files automatically receive the new default. Dragging the
slider waits 350 ms after the last edit before rebuilding on the worker.

The equal-area spherical placement keeps the existing terrain/root queries,
water/channel clearance and slope exclusions. Seeded, bounded tangential jitter
breaks up the uniform spacing. A coherent coverage field retains forest patches
and clearings. The 64,000-tree safety limit uses deterministic reservoir sampling
rather than stopping at northern latitudes.

## Broadleaf and conifer biomes

Four existing Island broadleaf prototypes retain their detailed trunks and
foliage. Four lightweight conifer variants add irregular, overlapping evergreen
whorls, tapered crowns, visible trunks, height variation and a cooler green
palette. They share the existing radial wind, weather shadows and day/night
lighting. Conifers retain solid branch whorls at distance and add a short needle
volume up close, with their own cutouts rather than the broadleaf crown mask.

Species selection uses a stable annual-temperature proxy based on the weather
model's latitude/elevation baseline: approximately `20 - 30*y² - 0.09*height`
Celsius, with small coherent regional variation. The probability blends between
broadleaf and conifer across 12–2 C. Trees thin and shrink between -4 and -10 C,
leaving a cold treeline rather than filling the ice caps. This is a placement
climate estimate, not a species switch driven by short-lived weather fronts.

**Inspect conifers**, **View → Conifers**, or `--view conifers` selects a lit,
colder stand. **Inspect forest** and `--view forest` remain available.

## Rendering cost

Planet already shares prototype geometry and materials between tree instances.
This implementation keeps that GPU instancing. Island's spatially merged crown
builder would require separate meshes for planetary patches and different LOD
ownership; it is not imported here. The inexpensive shared silhouettes and
bounded close detail are sufficient for the tested density increase.

- Distant broadleaf trees use three low-poly crown lobes and the existing Island
  LOD2 trunk in a single shared mesh: about 204 triangles per tree.
- Distant conifers use five whorls and a slender trunk: about 144 triangles.
- Conifer base crowns have fewer than 400 foliage triangles. A shared, optional
  near-needle mesh adds 1,152 triangles in four short shells.
- At most 96 closest trees receive detailed geometry. A shared cap of 12 trees
  covers broadleaf shells and conifer needles together. Remaining trees retain
  their shared distant crowns.
- Horizon cell culling, bounded uploads and asset reuse remain in place. Far
  closed crowns cull back faces; their visible silhouettes remain intact.
- Near-tree selection and membership work runs on movement/detail changes or
  pending uploads, rather than rebuilding a set every stationary frame.

## Validation

Release captures on seed 42, radius 2000 m, 1024 starting vertices and two
refinements produced 10,263 trees (3,003 conifers), compared with 4,423 before.
The same fixed orbital camera and graphics settings were used for the orbit
comparison. Separate forest inspection views were also checked visually; those
select a tree from each placement set and therefore do not have identical poses.
Clouds were disabled and solar time paused for both builds. Captures wait for
forest uploads and LOD preparation to settle. These are short stationary render
samples, not a guarantee for every terrain seed, density or GPU.

The automated viewer checks cover climate selection, conifer triangle bounds,
shared distant trunk geometry, radial roots, the near-detail/shell limits,
visibility handover, regeneration, CLI validation and preferences restoration.

Measured medians were approximately 15.35 ms before / 14.9 ms after in the first
fixed-orbit comparison, and 9.94 ms before / 9.89 ms after in the forest inspection
views. The conifer-dominated inspection measured 11.59 ms. These timings include
the complete rendered scene and are recorded under `work/forest-*.png.txt` in
the task workspace.

## Ferns and fallen wood

**Forests → Ferns / Fallen logs** independently toggles these details, including
log collision. The toggles are saved. **Inspect forest floor** or `--view
undergrowth` finds a lit log with nearby fern beds and a view clear of trunks.
CLI `--no-ferns` and `--no-fallen-logs` override the saved visibility.

The renderer reuses Island's frond builder and broken-log geometry through two
small public prototype functions. Island's existing placement and mesh batches
are unchanged. Four fern rosettes share curved fronds and the original analytic
leaflet cutout; eight log prototypes retain broken roots, branch stubs, closed
caps and three native LODs. Caps have end-grain shading; logs remain stationary,
while fern tips follow weather wind. Cutouts, wind and distance fading agree
between colour, depth and shadow passes.

Placement runs on the existing forest worker against the final radial terrain.
Ferns grow in spaced beds around trunks, avoiding freezing climates, river water,
beaches and steep terrain. Actual water clearance is checked as well as river
bed colour, including the thin film on waterfall supports. Logs avoid trunks and
other logs. Nine positions along the shaft and three across its width must have
suitable ground; a fitted shaft cannot bridge a hollow with a support gap above
24 cm before Island's partial burial seats the rounded underside into the soil.
Nearby logs have inexpensive capsule support for walking and jumping.

All instances reuse prototype meshes and materials. A physical 32 m spatial
index finds nearby decorations when the camera moves two metres; it does not
rescan every plant each frame. Uploads are limited to 12 instances per frame.
The nearest 512 ferns and 96 logs are the maximum active population. Ferns fade
out by 50 m; logs change detail around 35/80 m, with hysteresis, and fade out by
160 m. Ground decorations are therefore absent from orbital views. Density
follows the forest setting; global placement limits retain bounded memory.

Seed 42 at 2000 m radius, 1024 starting points and two refinements produced about
26,000 fern clumps and 900 fitted logs. A short same-camera, paused-weather
comparison had frame medians around 9–11 ms with details enabled or disabled;
this is a stationary smoke check, not a frame-rate guarantee across hardware or
all forests. Placement determinism, ground clearance, instance budgets, LOD
reuse, visibility restoration, collision and generation cleanup have automated
coverage. Existing Island fern and log tests also pass.

## Riverbank reeds and rushes

**F1 → Riverbank plants** contains independent **Reeds**, **Rushes** and
**Density** controls (0–3, default 1), plus **Inspect riverbank**. All three
settings persist. CLI overrides are `--no-reeds`, `--no-rushes`,
`--reed-density N` and `--view reeds`. Waterside plants work independently of
forest visibility. Density edits debounce for 300 ms before rebuilding on a
worker against the shared final-terrain query.

Island's four crossed, vertically subdivided cards form each shared clump (24
triangles). Eight seeded variants share two materials. The analytic blade and
cattail-head cutouts agree between colour, depth and shadows; roots stay fixed
while tips bend with the planetary weather wind. Rushes are shorter, denser and
stiffer. There are no texture downloads or fur shells.

Dry roots follow the actual clipped river boundaries and gentle sea shores,
with patchy density, mixed heights and two narrow bank rows. Placement checks
final water clearance, substrate, slope, stable local temperature and waterfall
aprons. Deep, steep shores and frozen regions remain clear. Roots sink slightly
into the surface to avoid floating cards. A deterministic reservoir spreads a
60,000-clump placement limit around the globe rather than favouring one pole.

Only the nearest 768 clumps within 60 m are drawn, through a 24 m spatial hash.
At most 16 instances spawn each frame; selection updates after 1.5 m movement
or a control change. A shared dither fade over the outer 12 m agrees in colour,
depth and shadow passes. Regeneration releases the old instances and meshes.

## Live soil colour

**Terrain appearance → Dirt colour** now rebakes every soil-bearing albedo
recipe, including forest floor, riverbed and fallen stones. Previously only
bare dirt received the shader tint; composite textures retained their original
soil colour. Edits wait 300 ms before a background 512-pixel recipe bake, then
upload a fresh six-layer albedo image and its full mip chain, then rebind
existing terrain/grass materials and the shared source for future chunks.
Bevy 0.19 updates only mip 0 in its in-place image upload path; reusing that
path left older colours in distant mips and misaligned later array layers.
Only dependent materials are refreshed, once per completed bake. Outdated
worker results are discarded. Terrain generation, normals, height/occlusion
maps, rock and beach recipes remain unchanged. Bare dirt responds immediately
while the composite textures finish; no Generate action is needed.

Validation includes deterministic radial bank placement, dry roots, shared mesh
reuse, draw limits, toggles, regeneration cleanup, settings validation, composite
recipe colour changes and rejection of stale colour-bake results. Offscreen
Metal renders check the bank plants and a live soil-colour edit after the scene
has settled, through the same appearance resource used by the colour picker.

## Close pine needles

**Forests → Close pine needles** toggles the saved close detail. **Inspect pine
needles**, **View → Pine needles**, or `--view needles` selects a pine and finds
an approach outside nearby crowns. `--no-pine-needles` provides a comparison.

Each of the four conifer variants shares one 1,152-triangle detail mesh. Four
shells form fine, tapered fibres up to 12 cm long in the prototype, leaning
out and down with the branches. The former regularly placed rim cards are
removed. Physical crown UVs stay attached during radial wind; warped,
integer-hashed roots and coherent density patches avoid uniform rows. Uneven
fibre lengths thin the outer shell and break up the branch edges. The crown also gains close colour and
normal variation; green tips transmit a little sunlight. Colour, depth and
shadow passes use the same needle cutouts and fade.

Detail fades over 14–28 m and uses the existing nearest-12 foliage budget,
including broadleaf shells. Distant meshes and forest density stay unchanged.
The highest added geometry is 13,824 triangles when all twelve slots are pines.
The saved toggle acts immediately; it does not rebuild forest placement.

Tests cover shell attachment/length, the shared triangle budget, first-upload
visibility, the nearest-12 limit, live toggles, cleanup, regeneration and saved
settings. Offscreen Metal captures exercise the colour and shadow shader paths.
