# Planet experiment

The spherical planet generator and Bevy viewer in the Island-experiment
repository. A closed spherical mesh is built from seeded random surface vertices,
using a translating plane to order an incremental
convex-hull construction of the spherical Delaunay triangulation.

See [the planet conversion plan](../planet-rs/PLANET_PLAN.md) for the geometry contract,
validation requirements, and staged conversion of the inherited code.

`planet-rs` owns spherical topology, 3D continental/detail noise, geology,
hydraulic/thermal erosion, and radial river drainage/carving. `planet-bevy`
displays the terrain, a player-centred spherical ocean, and terrain-fitted river
surface meshes. The Island applications share this repository and retain their
independent Cargo workspaces.

## Run the planet prototype

From the Island-experiment repository root:

```sh
cargo run -p planet-bevy -- --seed 42 --vertices 4096
```

The parameter panel opens on the left. Edit the draft settings and press
**Generate**; the current planet stays visible while a background worker builds
the next one. The controls show generation followed by render preparation while
LOD meshes and surface maps bake. The new planet's rendered chunks and collision
terrain activate together once rendering is ready; the old planet remains
visible and walkable until then. **New seed** advances the seed and generates. **Previous** swaps
the displayed planet and settings with the previous successful result, keeping
the camera position for comparison. Failed generation or render preparation keeps the current result.

**Quick preview** selects 1024 starting points, three refinement stages, and one
river refinement for faster experiments. **Full detail** restores 4096 starting
points, five refinements, and three river refinements. These buttons edit the
settings; press Generate to apply them. **Defaults** resets the draft settings.

The collapsible sections expose terrain/noise, hydraulic and thermal erosion,
tessellation/smoothing, river width/depth/catchment, and the early sea connection
passes. Visibility, shading, edges, and Front/Back/River camera buttons act
immediately. **F1** hides or shows the panel (`--hide-controls` starts it hidden).
Dragging or scrolling inside the panel leaves the camera alone.

Drag outside the panel to orbit, scroll to zoom, **W** toggles triangle edges, **S** switches
flat/smooth shading, **O** toggles sea, **R** toggles rivers, **Home** resets the
camera, **B** toggles before/after channel carving, and **Esc** exits. The default terrain view is smoothly shaded.
`--view back` inspects the opposite hemisphere; `--view river` frames a channel
close-up. `--no-ocean` and `--no-rivers` expose the bed; `--uncarved` shows the
terrain before the final channel carve with the same camera.
Radius is the fixed physical sea level; the viewer uses normalized units so
camera controls work at any radius.

The default sea-level radius is **1000 metres (1 km)**. Click a point on the
planet without dragging to arrive there in first person, with your eyes 1.7 m
above the surface. The controller collides with the displayed terrain triangles;
local up always points away from the planet centre. Visible seas support movement
at the water surface; hiding the sea lets you explore the seabed.

On foot, **WASD** moves, the **mouse** looks, **Shift** runs, and **Space** jumps.
Walking speed is 4.5 m/s, running speed 9 m/s, and gravity 9.81 m/s² toward the
centre. Steep uphill cliffs block movement. **Tab** releases the mouse and pauses
the view for screenshots; repeated presses keep it released. Click the world to
resume capture. **Esc** returns to the exact
previous orbit view, where clicking another point starts a new visit. Leaving
the app releases the mouse. The parameters panel returns in orbit mode.
The radius control and `--radius` use metres and set walking scale and export
units together.

Nearby rock patches refresh after 12 metres of movement. Their terrain query,
local snapshot and rock generation run on a worker sharing immutable terrain and
BVH buffers; refreshing a patch does not copy terrain on the movement thread.
Each job retains its generation until completion, while displayed rock contacts
remain local to the explorer.
The moving rock-mask coordinate frame uses one shared 48-byte GPU buffer. Patch
updates keep terrain and grass material bindings intact instead of rewriting the
coordinate frame on every material.

River finishing now ports the original Island generator's fixed downstream
profiles, five-segment gradient response, terrace spacing, and waterfall lip /
face / landing construction into radial heights and local tangent planes.
Channels are carved against that hydraulic profile before water is assembled.
Flat reaches retain a level cross-section, and confluences use the receiving
lower profile. Waterfall face support follows its own primary upstream path
and stops at the downstream merge. Other feeders and the receiving river retain
their own profiles, even when they run through the same waterfall footprint;
the face raise cannot turn those channels into a mound. Primary paths use flow
catchment, including where several river widths reach the same width cap.
Waterfall placement also reserves the receiving channel's full bank apron around
each junction. If a proposed upstream blend, face or landing overlaps that zone,
the terrace step waits for a safe position farther upstream. The lower channel
stays clear into the merge. Short branches without enough room omit the fall.
Placement also keeps the complete support rectangles of neighbouring waterfalls
separate, including crossing bank aprons and the upstream blend. Conflicting
steps wait farther upstream, leaving a 75 cm gap at the default radius. Delayed
terrace drops retain their planned height and can combine at the next clear
position, so moving a fall does not excavate the upstream river more deeply. This
prevents one waterfall's lower profile from cutting through another's bank
repair. Chunk cache generation 9 retires meshes baked before this separation.
Island's bank lift raises low bank vertices
to the water surface and blends a 4–20 metre apron into the outer terrain at
the default radius. Final boundary water vertices are pinned to the actual
terrain, including after drainage repair; there is no width cut through open
water. Vertical waterfall faces retain a three-centimetre surface clearance
at the default radius, with their dry banks pinned to the terrain.
River finishing keeps the profile and ownership fixed through up to three
width-based tessellation passes, with a two-ring terrain apron. Edge targets
are 1.5–5 metres at the default radius. A separate 1.25-metre-target pass and
one final face subdivision add detail across waterfall banks.
After lifting, four simultaneous height-relaxation passes retain the smoothstep
cross-section target and smooth the outer apron while fixing the bank and
waterfall patch. Two final tangent-plane bank passes use Island's 0.45 smoothing
strength and its width/edge movement limits. Junctions, waterfall anchors and
one neighbouring bank vertex remain fixed; unsafe triangle moves are rejected.
Terrain and water use the same outline. **B** compares radial shaping on this
final topology, using a saved pre-carve height snapshot.
Waterfalls retain the original 2.5-metre shaping run and upstream repair apron
at the default radius. A distance weight fades bank lifting over the 2.5 metres
before the lip, and six radial smoothing passes fill low gaps and clamp high
banks toward the lip. Two groups of six final smoothing passes round the whole
river corridor and bank apron, including both sides of each waterfall.
Smoothing stays at full strength through the two-ring outer apron, then fades
with a smoothstep across two more mesh rings into untouched terrain. This
uses Island's bank-ridge fix without changing the local tessellation footprint.
Ordinary banks retain their pinning floors; hydraulic ceilings and minimal lifts
of connected feeders preserve downhill paths. Submerged roots stay fixed. Sea-level channel interiors have a separate opening
mask: they remain below sea level rather than being classified as dry banks.
The last fall's submerged bed uses the channel-depth cross-section, and bank
lifting, bed relaxation, approach smoothing and final smoothing leave that bed
intact. Final grading keeps these submerged roots open; generation rejects an
outlet that has been refilled above sea level.
At placement, the fall follows the incoming river tangent. Across the drop,
its crest and foot use one straight centreline and the same width, so a bend
or a width change in the sampled reaches cannot shift the banks sideways.
Centreline distance and width blend back to the natural river through the
upstream repair area, landing and outer bank apron. Ownership still follows
the original connected reaches. The original downstream run and smooth face
profile remain intact; there is no later compression of terrain positions.
Final river water uses the same inclination blend everywhere:
`alignment = clamp(dot(terrain_normal, radial_up), 0, 1)`,
`flat = alignment * alignment * (3 - 2 * alignment)`, and
`water = lerp(terrain + 3 cm, regular_water_level, flat)` at the default radius.
Area-weighted normals come from the final smoothed terrain. Vertical surfaces
use the terrain offset, flat reaches use the regular hydraulic level, and
inclined reaches interpolate continuously without a face/foot mask switch.
The smoothstep easing settles gently into both the vertical and flat methods.
Dry boundary vertices remain clamped to terrain so the banks cannot float.
After all smoothing, each bank is walked from downstream to upstream using
distance along the connected river network. Any lower bank vertex is lifted
to the preceding height. The two sides remain independent, submerged outlets
stay open, and nearby disconnected bends do not set each other's bank heights.
A lift-only drainage pass raises any newly trapped feeding terrain; no later
incision can reopen the banks. The terrain pass checks both the original
corridor rim and the final wet outline. If lifting exposes another dry outline,
grading repeats with all previous constraints retained.

After the inclination blend, actual clipped water–terrain contact points are
inserted into the shared terrain and graded upstream as well. These conforming
edge splits preserve the surface before lifting, and both meshes then use the
same graded bank vertices. Water carries its existing bed clearance through that
final lift; the slope blend is not recalculated afterwards. This keeps the thin
waterfall face clearance while preventing a new dip between graded terrain
vertices. Straightened waterfalls use their own flow axis for bank ordering
across bends.
Use **Falls** in the view controls (or `--view waterfall`) to inspect a cascade.
Use **Mouth** (`--view mouth`) to inspect a final waterfall entering the sea.
The graded ocean mesh described below retains fine curvature at waterfall mouths.
River geometry now also uses Island's animated water shading, adapted to radial up.

## Spherical ocean

The ocean's four circular swells, independently scrolling ripple normals, whitecaps,
onshore breakers, shallow-water attenuation and coloured absorption now run on
the planet. Wave phases use fixed planet positions and continuously integrated
time. Moving the player or changing wave speed does not restart or drag them.
The shader receives sun shadows, reflects the sky and sun, and uses the opaque
scene colour and depth for refraction. A bounded screen-space ray march adds
terrain reflections, with sky fallback beyond the viewport or reflection range.
This replaces Unity's flat planar reflection camera on the curved planet.

Four seeded random sources wander over the sphere at **0.35 m/s** by default,
turning gradually and transporting their heading over the curved surface. Their
walk uses fixed simulation steps so frame rate and camera movement do not alter
the paths. Each source radiates outward in true surface distance `d`, using
`h = A * sin²(π d/R) * sin(2π(d - speed*t)/wavelength + phase)` for `0 < d < R`,
and zero outside that range. Here `R` is the physical planet radius: **1 km along
the surface** on the default planet. Each contribution has zero height and slope
at both ends, with maximum amplitude halfway out. The four contributions overlap;
other sources can still contribute at a source's centre. The original 30, 15,
7.5 and 4 m wavelengths remain, and analytic derivatives provide matching normals.
Coastal breakers and shallow-water suppression apply after the swells combine.

One closed spherical mesh rotates its dense pole beneath the camera, including
when walking around the planet. Concentric rings start at **1 m spacing within
256 m**, then double spacing every eight rings up to 8 m on the default 1 km
planet. The far spacing remains bounded by curvature; neither the antipode nor
ring transitions have open seams. Geometric waves fade from 192 to 256 m;
filtered ripple normals and unresolved swell roughness remain visible farther
away. Default geometry is 410,018 vertices and 820,032 triangles. Moving the
camera changes only the entity rotation, not its vertex buffers. This ocean
mesh has its own fixed 500,000 vertex / 1,000,000 triangle limit, separate from
the terrain streaming budget.

Six **1024² cube-map faces** sample the final displayed terrain for radial depth,
geodesic distance to land and river-mouth suppression. Shore-distance propagation
crosses cube edges. Depth covers 0–10 m and land distance 0–128 m; filtering
across cube faces avoids longitude/pole seams. Waves flatten over land, shallow
seabeds and river outlets. The maps have their own checksummed `.ocean-v1` cache
beside the terrain chunks. Regeneration publishes matching terrain, collision
and coastal maps together. The initial bake runs in the background; subsequent
loads reuse the maps without invalidating existing terrain LOD caches.

The **Ocean** section changes wave amplitude, speed, source drift, choppiness, foam, roughness,
refraction, fine radius and spacing immediately. It also pauses animation,
disables geometric displacement and displays depth, land-distance or source maps.
The **Sources** view colours each source's influence, centre and 1 km boundary.
Set source drift to zero to hold the centres still while the waves keep travelling;
Pause freezes both waves and centres. Changing the planet seed chooses four new
sources without rebuilding the ocean mesh.
Grid changes rebuild only the ocean mesh asynchronously; settings exceeding its
budget report an error and retain the last good mesh. **O** still toggles the sea.
Walking uses the mean sea sphere; wave buoyancy is not implemented.

```sh
cargo run -p planet-bevy -- --view ocean
cargo run -p planet-bevy -- --view shore --ocean-paused
cargo run -p planet-bevy -- --ocean-view distance --view back
cargo run -p planet-bevy -- --ocean-view sources --ocean-paused
cargo run -p planet-bevy -- --view ocean --ocean-drift 0
python3 planet-bevy/tools/check_ocean.py # native GPU wave/cube-map regression
```

## Sea and river shader effects

The active environment effects from Unity's `SeaWater`, `RiverWater`, shared
`WaterOptics`/`OceanOptics`/`RiverSurface`, foam-history and underwater shaders
are adapted to the spherical viewer:

| Effect | Planet implementation |
| --- | --- |
| River flow | Generated metre coordinates follow the drainage network, sharing downstream phase through confluences and clipped chunk edges. Cotangent gradients orient broad/fine ripple normals through bends and vertical falls. |
| Rapids and waterfalls | Slope uses the normal's alignment with radial up. Rapids roughness, stretched travelling foam and fine falling-water streaks use Unity's scales, thresholds and 9 m/s fall speed. |
| Estuaries | Flat low river surfaces blend into ocean absorption and reflection settings within 2 m of sea level. |
| Depth optics | RGB Beer–Lambert absorption, dielectric Fresnel, roughness-filtered glints and sun/shadow lighting. Distorted colour lookups reject foreground and above-water banks, bound displacement by water depth, and revalidate the final sample. |
| Independent surface detail | Animated ripple normals bend reflected rays. Refraction has a separate moving distortion bounded by water depth and physical scale. Reflected rays reject false silhouette hits and fall back to sky. |
| Ocean detail | Filtered scalar-noise gradients, two moving ripple bands, unresolved slope variance, backlit crest translucency, coastal tint and whitecap coverage. |
| Persistent foam | Two 512² GPU textures cover a 256 m local patch. Foam is reprojected across the sphere, advected by displacement changes and drift, deposited and exponentially faded with Unity's 6 s lifetime. Dry and suppressed areas clear; regeneration resets the history. |
| Water contacts | Up to 32 expanding foam/normal rings follow the displaced ocean. First-person sea footsteps and landings emit them. |
| Waterfall foot mist | At most 32 nearby LOD0 feet use animated, shadow-lit volumes with 28 depth-clipped ray-march samples, rising wisps and downstream dissipation. Placement samples the final river and terrain surfaces so the dense base clears the finished landing. One proxy mesh and unchanged active instances are reused. |
| Underwater | A per-pixel, displaced waterline applies RGB extinction and suspended-particle haze. The actual scene/surface depth stops absorption at the first exit. The underside uses water-to-air Fresnel and total internal reflection. |

Scene colour, depth and mist lookups convert between camera viewport and full
render-target coordinates, including the offset introduced by the open parameter
panel. The GPU optics probe covers both full and offset viewports.

These shader changes do not move river vertices or alter bank pinning, carving,
waterfall placement, or the four wandering wave-source profiles. River LOD1–3
retain their terrain texture fallback. Flow coordinates are derived when a river
chunk is uploaded; the additional UV bytes count toward the streaming budget.
Existing terrain LOD and coastal-map cache keys remain valid.

The **Sea & river effects** controls adjust distortion, absorption, scene
reflections, river ripples/whitewater, waterfall flow speed, persistent foam,
mist and underwater haze. Ocean Pause freezes these animations together.
Boat-specific hull volumes, deck clamps, ship wakes and hull spray have no
boat data source in Planet and remain outside this environment port. Weather
cloud shadows are likewise supplied only when a weather system exists; current
water uses the viewer's sun shadows and radial daylight.

```sh
cargo run -p planet-bevy -- --view waterfall --ocean-paused
cargo run -p planet-bevy -- --view shore --water-reflections 1
cargo run -p planet-bevy -- --view underwater
cargo run -p planet-bevy -- --view underwater --no-underwater # optical comparison
```

Offscreen captures now wait for GPU pipeline compilation and fail on shader
errors. For local shader iteration only, `PLANET_LIVE_WATER_SHADERS=1` reloads
the repository's shader sources during a capture. Ordinary packaged viewers
continue to use their embedded sources.

## Forests

Forests reuse Unity Island's procedural tree generator through the local `motu`
library. Four shared prototypes supply branched wood, detailed foliage, eight
nearby leaf shells, and a coarse distant canopy. Bark and foliage use procedural
surface detail, radial wind, sun shadows, cloud shadows and the final depth buffer.
Growth steps rotate by at most 45 degrees, with no trunk-wide rotation budget.
Intermediate rings are omitted only while net twist and endpoint-frame rotation
stay within 45 degrees; otherwise a support ring is retained before
tessellation and smoothing. The final rounding pass supports those short spans.
Trunk-side branch eligibility uses the resulting upward-biased growth direction,
so a slight trunk lean does not exclude branches on half the trunk. Branch count
and tessellation levels stay unchanged.

Seeded equal-area candidates keep density independent of terrain tessellation.
Roots are fitted to the displayed terrain, with radial up and clearance around
river beds, beaches, steep ground and the snowline. A seamless globe mask from
the accepted roots supplies forest-floor shading and removes grass below trees.
Placement and prototype generation run on a worker retaining the current terrain
generation and sharing its immutable query index when available. Meshes are
shared by instances; coarse uploads and nearby spawns have per-frame limits.

Trees select their LOD by camera distance in both orbit and first-person views.
Fine wood is used within 30 m, leaf shells nearby, and detailed foliage out to
240 m by default. Beyond the detailed range, trees use the simplified canopy.
Each coarse canopy stays visible until its detailed replacement is ready, then
hands back when the tree leaves the range. Meshes remain shared between trees,
and coarse forest cells beyond the planet's curvature are hidden conservatively.

Use **Forest** or **Inspect forest** to visit a lit grove. The **Forests** section
controls visibility and the detailed-tree range. Forest assets and roots are
replaced when the planet is regenerated. For command-line comparisons:

```sh
cargo run -p planet-bevy -- --view forest
cargo run -p planet-bevy -- --no-forests
```

## Atmosphere and volumetric clouds

The atmosphere is a true planet-centred scattering shell, visible from both
surface and orbit. Bevy's scattering lookup textures use metre-based optical
coefficients adjusted to the small planet, with a persistent 64-pixel sky-light
probe. The existing directional sun drives atmospheric and cloud lighting.

Clouds occupy a separate spherical layer. A seeded, seamless 3D density field
rotates around the globe with the wind; it is independent of the camera and has
no longitude or polar seams. Its elevated shell remains visible beyond the solid
planet silhouette from orbit; only rays that actually hit the ground are clipped.
Coverage, density, formation size, cloud base and
thickness are live controls under **Atmosphere & clouds**. The default base is
100 m, thickness 55 m and formation scale 90 m. **Keep cloud base above mountains**
raises the base when necessary; disable it to let clouds surround mountain tops. The atmosphere
always extends at least 20 m above the effective cloud top.

Low quality uses quarter-width/height cloud buffers and 16 view samples;
balanced uses half-width/height and 24; high retains half resolution with 40.
Opaque and transmissive final depth stop the cloud march. Depth-aware upsampling
keeps cloud fog off foreground terrain, grass and water. Temporal reconstruction
accumulates only cloud radiance/transmittance, follows camera and wind motion,
and rejects changed depth, disocclusions and vanished clouds. Histories reset
for regeneration, changed weather settings, teleporting and viewport changes.
The checkbox disables reconstruction for comparisons.

Cloud shadows share a low-resolution global cloud-column map. Rivers and ocean
sample a persistent 64²×6 sky/cloud reflection cube, refreshing one face per
frame and all six after a reset. The reflection's cloud field and illumination
match the volume; its clear-sky colour and foreground cloud haze approximate
the atmosphere instead of sampling Bevy's exact scattering lookup tables.
Cached shadows approximate attenuation of aggregate surface lighting, with an
indirect-light floor. These approximations keep weather updates bounded without
rewriting terrain materials or adding a cloud march to every water fragment.

The 3D camera renders in HDR; clouds composite after SSAO and before underwater
extinction and tone mapping. The UI remains a separate overlay. **Sky** frames
the atmosphere from near the ground; **Clouds** places the camera within the
current cloud layer. These views also work from the command line:

```sh
cargo run -p planet-bevy -- --view sky
cargo run -p planet-bevy -- --view clouds --cloud-coverage 0.65
cargo run -p planet-bevy -- --cloud-quality low --no-cloud-temporal
cargo run -p planet-bevy -- --environment-paused --sun-elevation -15
cargo run -p planet-bevy -- --no-atmosphere --no-clouds
python3 planet-bevy/tools/check_clouds.py
python3 planet-bevy/tools/check_environment.py
```

Captures include environment settings and settled offscreen frame-interval
statistics in their `.png.txt` metadata. Those intervals are end-to-end timings,
not GPU timestamp measurements.

## Planet chunks, LOD and streaming

The finished terrain and rivers are clipped into an icosahedral hierarchy.
Depth 2 gives 320 spatial cells; depth 0–3 gives 20, 80, 320 or 1280 similar-area
cells. Terrain and river patches are counted separately from the ocean. Clipping
preserves the existing planar geometry and interpolates shared positions,
normals and colours identically across every cell edge.

Each terrain patch has four LODs generated from the final surface. Every
boundary vertex is fixed at every LOD so neighbouring cells meet exactly.
LOD edge collapses reject new triangles narrower than a 10:1 longest-edge to
minimum-altitude ratio and stop vertices growing beyond 16 incident faces.
Existing narrow clipping triangles may remain, but cannot become thinner.
This can retain extra geometry beside dense chunk seams rather than stretching
those seams into large fans. Collapse priorities include both endpoints' error
bounds so simplification spreads across the patch instead of growing one hub.
Terrain LOD1–3 allow interior river beds, banks and shoreline features to collapse
normally. River water uses
its full authored mesh only where terrain is actually displaying LOD0; coarser
terrain receives a baked river texture instead. Screen-space error and hysteresis select detail; a resident
finer mesh remains visible while a replacement is loading.

The **Chunks & streaming** controls toggle horizon culling and chunk colours,
force an LOD, adjust the pixel-error target and detail-memory budget, and rebuild
at another chunk depth. Counts show rendered patches, horizon/frustum rejection,
LOD distribution and estimated GPU mesh/texture bytes. Coarse fallback geometry is
always resident in addition to the adjustable detail budget. Nearby detail is
prefetched; unused fine geometry is released after a grace period or under
memory pressure. A budget reduction falls back to coarse meshes without holes.

Terrain LOD1–3 recover LOD0 lighting detail through baked normal and ambient
occlusion maps, using Island's chart-atlas approach. Each simplified chunk has
its own UV charts, while one finest-terrain sampler supplies normals and AO
across chunk borders. RGB stores the correction from each LOD's vertex normal
to the finest surface normal, so retained bank detail stays sharp even when
an atlas cannot resolve every narrow bank triangle. AO probes use metre-scale distances in the local tangent
plane, with spherical curvature removed. LOD0 uses its original vertex normals;
river and sea geometry retain their existing shading. Flat-shading inspection
bypasses the maps so face normals remain visible.

**LOD normal + occlusion maps** toggles the effect immediately. **Surface**
selects shaded, normals or occlusion inspection. The command-line equivalents
are `--lod-surface-maps on|off` and `--surface-view shaded|normals|occlusion`.
Base atlas targets are 256, 128 and 64 pixels for LOD1, 2 and 3; extra chart rows
preserve thin cut-boundary triangles. Eight-pixel gutters and a maximum of
two additional mip levels prevent filtering across unrelated charts. Atlas UVs,
linear normal/AO pixels and a one-byte river-coverage mask participate in caching
and the streaming budget, including their mips. Four coverage samples per texel
soften narrow river edges. Texture assets are released with their meshes. Baking runs in the
background on the first preparation of a world, then reuses disk results.

The **Rivers** toggle applies to both water meshes and river textures. The texture
is independent of the normal/AO toggle. LOD0 water is requested only for cells
whose terrain is displaying LOD0; after a budget downgrade it is hidden in the
same update, preventing floating water over simplified ground. Distant river
fallbacks have no mesh payload. Source hydrology and first-person collision
still use the finest terrain.

Horizon tests use height-aware patch bounds and an inscribed sphere computed
from triangle interiors across all terrain LODs. This stays conservative below
sea level and allows mountains to remain visible beyond the sea-level horizon.
Disabling horizon culling keeps normal camera-frustum culling enabled.

Versioned, checksummed mesh/atlas files and a world index live in
`target/planet-chunks/`. Their key includes complete source geometry, attributes,
chunk depth, physical radius and the algorithm version. Background tasks preprocess/cache new
worlds and read requested fine meshes; only GPU installation runs on the main
thread. Missing or corrupt fine files trigger rebuilding while coarse geometry
remains visible. Captures wait for streaming to settle and save chunk statistics
beside the PNG as `.png.txt`.

This streams render geometry. The procedural simulation, source terrain and
exact collision index remain global CPU data, so drainage stays continuous and
click-to-land has authoritative ground available immediately. The first run
prepares the chunk cache; subsequent identical worlds reuse it. **Edges** / **W**
renders wireframes directly on the active terrain chunk meshes, including
streaming fallbacks and mixed LODs. Edges share terrain visibility and update
with LOD switches; no separate full-resolution overlay mesh is retained.

```sh
# Inspect partition boundaries and disable horizon rejection for comparison.
cargo run -p planet-bevy -- --chunk-colours --no-horizon

# Force full detail, or use a tighter/looser automatic pixel-error target.
cargo run -p planet-bevy -- --lod 0 --chunk-budget-mb 192
cargo run -p planet-bevy -- --lod auto --lod-error 2 --chunk-level 2
```

```sh
# Validate/export without creating a GPU or a window.
cargo run -p planet-bevy -- --validate-only --export planet.obj

# Capture the terrain offscreen to a new PNG.
cargo run -p planet-bevy -- --hide-controls --screenshot planet.png

# Inspect the original undeformed spherical triangulation.
cargo run -p planet-bevy -- --sphere-only --wireframe

cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 planet-bevy/tools/check_water_optics.py # native GPU reflection/refraction regression
```

Use `--help` for seed, count, radius, and sweep-axis options. The initial
all-face scan and exhaustive validation have quadratic cost; point counts are
limited to 4–8192 for the initial Delaunay sphere. Subsequent conforming
refinement supports up to two million terrain vertices. Optional adaptive stages
stop before exceeding their budget, reserving room for local river refinement. Very small random sets can fail whole-sphere coverage and
are rejected with a seed/count diagnostic. Trees, caves, atmosphere, animated water and generation animation are later
adaptations.

## Sun shadows and depth-buffer ambient occlusion

**Lighting & ambient occlusion** in the parameter panel controls sun shadows,
physical shadow distance/bias, AO strength/radius, 6/10/12 samples, half resolution
and an AO-only diagnostic. The same controls appear when releasing the mouse with
Tab in first person. These controls take effect without regenerating the planet.

The sun uses four 2048-square shadow cascades, Gaussian PCF filtering and a
300 metre surface range, following the Unity project's lighting configuration.
Distances and the 2 cm depth bias are converted to the viewer's normalized units.
Orbital views expand the cascades to include the planet. Terrain outside the
camera frustum can still cast shadows from its resident LOD; the camera's own
frustum culling and planet horizon culling remain active. Terrain and stones cast
shadows, while water and fur shells receive them. Modest ambient skylight keeps
shadowed rock detail visible.

AO uses an independent WGSL hemisphere-probe implementation of the Unity effect.
Its defaults match OpenSeaWorld's settings: strength 0.3, radius 0.05 metres,
12 samples at full resolution. It reads the completed opaque depth buffer after
the clipped grass shells and reconstructs view-space positions and normals.
Two bilateral blur passes and geometry-aware upsampling preserve silhouettes;
empty sky depth stays unchanged. AO composites before tone mapping and UI, and
works alongside the existing baked LOD material occlusion. FXAA handles screen
antialiasing with the single-sample, readable final depth buffer.

The native GPU regression probe `python3 planet-bevy/tools/check_depth_ao.py`
executes the actual AO shader against steep planar depth, empty sky and an
occluding surface at both resolutions, including texel-boundary rounding.

For a broader cavity effect, try radius 1.5 metres and strength 0.6. Command-line
comparisons include `--no-shadows`, `--no-ssao`, `--shadow-distance 300`,
`--shadow-bias 0.02`, `--ao-radius 1.5`, `--ao-intensity 0.6`, `--ao-samples 12`,
`--ao-half` and `--ao-only`.

## Unity Island ground shading

Planet uses the `Motu/Terrain Unified` ground shader ported to WGSL. It keeps
Island's recipe/procedural transitions, height blending, 37–45 degree rock
shoulder, grass ground colour, noisy snow line, detail normals, local occlusion,
wet banks and shared eight-step parallax ray. Nearby LOD0 terrain also uses the
sixteen-layer `Motu/Terrain Grass` fur port described below.

Elevation is distance from the original sea sphere; slope uses the normal's dot
product with radial up. Physical material sizes remain 2/2/4/2/3/2 metres as in
Unity. Terrain recipes and fur grass share radial Lambert equal-area hemisphere
UVs measured on the sea sphere. North and south meet as a mirror at the equator;
the poles map to the centres of the two discs, without a longitude cut or pinching.
Vertex elevation does not move the UVs. Equal area keeps the grass density
consistent, with bounded directional stretch towards the equator. Normal-map
surface gradients use the mapping Jacobian, including the reflected direction;
a neutral normal map adds no tilt. Mip selection uses the maximum local UV
stretch rather than derivatives across a wrap seam. Parallax fades beyond
8–24 metres, and mipmapped 3D noise
filters subpixel patches in orbit. Existing streamed LOD normal/occlusion maps
and coarse river masks remain independent of the ground texture projection.

The generator supplies hardness, loose soil cover, carved river bed and coastal
coverage. The fallen-stone recipe uses the local rock placement mask. Forest-floor
coverage stays empty until Planet has a tree placement field.
The six recipe layers are baked from the same native library used by Unity,
using its default palette, and embedded into the viewer. Three shared texture
arrays plus the cliff-noise volume use about 25 MiB outside the per-chunk budget;
their full mip chains are filtered independently per material. No Island
checkout, Unity installation or native plugin is needed to run Planet.

### Grass, stones and boulders

**Surface detail** is a collapsible parameter section with immediate grass,
stone and large-boulder toggles, grass height/density and rock density controls.
`--view grass --lod 0` starts a close inspection of a suitable grassy slope.

Grass reuses the active LOD0 terrain buffers for sixteen clipped shells, with
Island's warped tuft pattern, random blade heights, colour field, tangent wind
and 10–20 metre stochastic distance fade. Growth follows the terrain normal;
slope and elevation use radial up. A single radial tuft field mirrors between
hemispheres at the equator, with no blend between separate planar patterns.
Coverage excludes rivers, bare rock, beaches, snow and rock footprints.
Shells are removed when their source chunk leaves LOD0 or the camera is more
than 30 metres above the ground.

Stones retain Island's irregular, partly buried icosahedral cages and material
noise. The small population has 10–30 cm diameters, with occasional 30–60 cm
stones; river stones are 6–22 cm. Large boulders are 2–6 metres across, with two
rounds of subdivision and smoothing. Their bounding spheres support first-person
collision. Steep land faces seed downhill drops; river beds seed river stones.
Planet uses bounded quasi-static downhill settling instead of Island's full
sphere-drop simulation. It rejects overlapping rocks and support deeper than
10 metres below sea level, without Island's perimeter constraints.

A camera-local background worker owns a small terrain snapshot. Placement is
seeded by source face and planet seed, and geometry is grouped by the same
spherical cells as terrain. Rock meshes draw within 80 metres only while their
owning terrain is visible LOD0. A shared 512-square local mask blends fallen-stone
ground coverage and keeps grass out of rock footprints. Repartitioning,
regeneration and bed-view changes discard obsolete detail jobs and meshes.
Rock buffers and the 1 MiB mask are separate from the terrain streaming budget;
grass shares existing mesh buffers.

`planet-bevy/assets/terrain/source.json` records the recipes' source hashes,
palette, tile sizes and normal convention. To refresh the recipe pixels after
building Island's native plugin, run:

```sh
python3 planet-bevy/tools/bake_island_materials.py .
```

## Planet terrain contract

Sea level is the original sphere radius R. A terrain vertex is
`direction * R * (1 + relative_elevation)`; local down is `-direction`.
Continental and detail noise are evaluated in Cartesian 3D and can displace
every site above or below sea level. No island falloff, perimeter mask,
water-ratio percentile or coastal height remapping is used.

The pipeline adapts the inherited stage order to a sphere: noise/geology and initial soil,
hydraulic erosion with sediment transport/deposition, thermal relaxation,
two broad shared-edge subdivisions followed by up to three curvature-selected
refinements, tangent-plane position relaxation and conservative radial smoothing,
then renewed sequential hydraulic erosion at every stage,
river-corridor refinement, final flow/catchment calculation, cross-section
incision, and final normals/material colours.
The default uses five broad/adaptive refinement stages (`--refinements 0..6`)
and three smoothing passes per stage. Like the island CPU reference, hydraulic
paths run sequentially from high terrain, retain downhill momentum and sediment,
and observe earlier paths' edits. Transport uses area-weighted volumes, hardness,
loose soil, slope-dependent deposition, and terminal sediment fans. One sweep per
stage is the default; `--erosion-passes` repeats it and `--erosion-strength`
controls its strength. Hydraulic height changes share a cumulative per-stage
budget limited by local vertex spacing, so converging paths cannot repeatedly
excavate or pile sediment onto a tiny coastal cell. Terminal sediment spreads
over an angular footprint of at least 0.015 radians, preferring submerged cells;
when that footprint fills, excess travels farther outward. Sediment is conserved
without clamping terrain to sea level. Final fine erosion runs after finishing smoothing and
before river cross sections and water construction. `--erosion-strength 0 --thermal-passes 0` disables erosion for comparisons; channel carving remains a
separate stage. Smooth material weights avoid discrete colour thresholds
outlining triangles.

This is a spherical adaptation rather than identical island output: smoothing
reprojects positions to the sphere and prevents triangle inversions, the sphere's
own curvature does not trigger adaptive detail, and hydraulic excavation is
radial rather than retreating vertices along a planar surface normal. No island
perimeter, ocean-ratio, or sea-plane erosion protection is introduced.

Slopes use radial elevation differences and great-circle distance; rainfall
and transport use solid-angle control areas. Eroded sediment can deposit on
the seabed. The transport budget is an area-weighted height approximation,
not a volumetric fluid or sediment simulation.

By default, disconnected seas are joined at the end of LOD2 (first refinement)
and LOD1 (third refinement). Starting from the first sea in stable vertex order,
a multi-source shortest-path search finds the nearest unconnected sea by angular
mesh-edge distance, cuts a channel below the original sea radius, then continues
from the sea just reached. The cut has a submerged core and blended bank apron;
any other seas incidentally reached are joined as well. Excavation is recorded
as exported sediment. Sea level and the initial land bias stay fixed; no target water ratio is
imposed. Dry worlds stay dry, and already connected oceans need no cuts.

Later smoothing, erosion, or river incision can separate shallow water again.
Those later disconnections are allowed: no final ocean repair carves narrow
channels into the finished terrain. The LOD2/LOD1 cuts remain actual terrain
excavations that evolve during subsequent generation. `--separate-seas`
disables the early connection passes for comparison. Console output reports sea counts and channels for each
stage; `PlanetTerrain::ocean_passes` retains those diagnostics.

Every vertex below R remains a valid river drainage root. A sea-seeded minimax flood finds spill routes, prefers downhill
neighbours where possible, and incises intervening saddles so the final land
routes decrease strictly in radial height. Rivers stop at the first submerged
vertex and merge through shared receivers. Material removed in final channel
incision is recorded as sediment exported into the water system; deposition of
that final exported material is not simulated. Net material added by bank and
waterfall support is recorded separately in `bank_added_material`; the per-vertex
`bank_lifts` retain the before/after radial height comparison. All-dry worlds have no sea-bound
rivers; neither all-dry nor all-wet worlds are forced into an island shape.

`--relief`, `--land-bias`, noise frequencies, erosion pass counts, and river
catchment/depth are available through `--help`. Elevation amplitudes and channel
depth are relative to R, so terrain generation scales consistently with radius.
The sea is a separate render mesh at R; toggling it does not change terrain.

The default uses two broad tessellations, up to three curvature-selected
refinements, and up to three local corridor refinements. Shared midpoint IDs and conforming transition triangles prevent
cracks. The broad stages add diminishing 3D displacement; later adaptive stages
interpolate the existing relief. Each broad/adaptive stage relaxes positions, smooths radial
heights conservatively, and runs hydraulic/thermal erosion. River detail is
added afterward, without re-routing or erosion between its subdivisions. Sea and land use
the same smoothing rules. Final channels have flow-scaled widths, broad-bottomed
beds, and blended bank aprons with smoothed outlines. Terrain is cut before water is constructed.
Water triangles share the terrain topology, clip at actual bank intersections,
and share cached edge vertices through confluences; they are not free-floating
ribbons. Every water triangle is checked against its supporting bed plane, and exposed
water edges above sea level must meet that plane.

`--refinements`, `--smoothings`, `--river-refinements`, `--river-width`, and
`--river-depth` control these stages. To inspect carving directly:

```sh
cargo run -p planet-bevy -- --view river --no-rivers --no-ocean
# Press B to switch between the carved and pre-carve terrain.
```
