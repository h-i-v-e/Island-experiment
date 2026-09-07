# Island Unity project

A conventional scene-based Unity 6 project for the Rust generator in
`../island-rs`. The reusable `IslandGenerator` component invokes the Rust C ABI
and displays the irregular terrain, streamed detail, rivers, sea, vegetation
shells, rocks, and hidden terrain colliders beneath its GameObject. River-bed
stones and physically settled dropped rocks share one native-generated,
tile-streamed mesh; rocks no longer use a separate GameObject renderer pool.

The `IslandGenerator` always uses the primary CPU method. Unity does not expose
the experimental GPU generator, and its native plugin is built without the GPU
feature so erosion, rivers, waterfalls, and settled rocks all follow the CPU
generation path.

## Initial soil blanket

Set **Initial Soil Depth Metres** in the standard factory's **Generation** settings,
or set `IslandGenerationSettings.InitialSoilDepthMetres` from a script. Its default
is zero. `CoherentIslandFactory` chooses a depth from 0–5 metres based on its initial
height sample: lower islands receive more soil. Adjust that policy in
`CreateGenerationSettings` to change the open-sea world's distribution.

The blanket occupies the top of the existing land surface before the first erosion
pass; it does not raise the terrain or coat the seabed. Hydraulic erosion removes
loose soil first, then cuts bedrock according to its hardness. Thermal erosion also
removes available soil first. Changing this generation setting applies to newly
generated/reloaded islands, with a separate snapshot cache key. Existing loaded
islands must be regenerated. This change bumps the native snapshot format, so old
cached snapshots are regenerated.

## Open and run

1. Double-click `Open Island Unity.command`. This bypasses a known local
   mismatch between Hub's Licensing Client 1.17.4 and the editor's 1.18.1
   protocol. macOS might ask you to confirm opening it the first time.
2. Open `Assets/Scenes/IslandRuntimeSandbox.unity`.
3. Select **Island World**. Its `GridIslandGenerationRequestFactory` owns the
   configured origin island and all of its generation settings.
4. Press Play. The world manager asks the factory for the origin cell and
   creates its runtime `IslandGenerator` on demand.

The project can also be opened normally from Hub after stale licensing clients
have exited. Use the launcher if Hub reports that it cannot connect to the
licensing service.

To add an island world to another level, create one GameObject with an
`IslandWorldManager` and a component implementing
`IIslandGenerationRequestFactory`, then assign that component as the manager's
request factory. The manager divides the world into 2 km XZ cells and supplies
only the grid position to the factory. The factory is the only authority for
island existence, seed, and settings: it returns a complete
`IslandGenerationRequest` for either a deliberately configured cell or a
generated cell, and returns `null` for open sea. There is no second
authored-island list, generator template, occupancy fallback, or pre-placed
`IslandGenerator` path in the manager.

The 2 km value is a single invariant: it is both the spacing between grid-cell
centres and the width of every generated terrain square. Factories control the
land and water distribution within that square, but cannot create overlapping
or differently scaled terrain footprints.

`GridIslandGenerationRequestFactory` is the built-in implementation. Its fixed
cell entries are suitable for deliberately placed islands, while its own seed
and unlisted-cell policy control deterministic open-ocean population. Both
paths produce the same request type and enter the same loading, generation,
snapshot, and unloading lifecycle. For a custom world distribution, implement
`IIslandGenerationRequestFactory`, normally by deriving from
`IslandGenerationRequestFactoryBase`. The factory component directly owns its
generation, river, vegetation, rendering, material-palette, and debug settings.
Its cell method chooses the seed, clones those settings into a profile, applies
its own per-island customization, selects the dirt, stone, and sand colours,
and constructs the completed request.
`IslandGenerationRequest` snapshots the supplied profile but does not select or
apply policy itself; it only carries the palette selected by the factory.

Runtime `IslandGenerator` objects are implementation details created and
destroyed by the manager. Each clones the settings and material templates in
its request before generated maps are assigned, so project assets are not
mutated. Global sea, sky, clouds, weather, and solar state remain owned by the
world manager rather than any island.

Generation builds the island, all three texture sets, the LOD1-clipped river
tiles, and the 8x8 LOD 2 overview on a background worker. The existing island
remains visible while regeneration runs, and the component reports elapsed time.
Unity texture and mesh objects are then uploaded on the main thread, with the
64 overview tiles spread across frames to avoid a large upload hitch. Use the
Inspector to regenerate another seed and adjust terrain, coastal evolution,
hydraulic-erosion, or river source selection. Coastal erosion cuts
exposed softer rock into bays and platforms; beach formation conservatively
redistributes that sediment toward sheltered shorelines. Source catchment is an
absolute drainage area in hectares. Projected vertex areas are accumulated in
world space, so one slider remains consistent across all mesh densities while
larger islands can support more rivers. A second control suppresses small sources
on steep slopes while retaining sufficiently large catchments. Generation-setting changes take effect when you press
Generate. A third source control continuously lowers the required catchment as
elevation rises. Its default of nine makes the sea-level requirement ten times
the requirement at the configured maximum elevation, discouraging short coastal
rivers without imposing a hard elevation cutoff. Drag to orbit, use the mouse
wheel to zoom, and right-drag to pan.

For a deliberately arranged open-sea test, add fixed cell definitions to the
request factory. They are not persistent scene generators: they simply make
the factory return a request whenever the manager scans those cells. Islands
generate serially, retain independent materials, coast masks, and native
handles, become dormant outside the active radius, and unload beyond the
unload radius. First-person flight and terrain snapping route through the
manager.

`Assets/Scenes/OpenSeaWorld.unity` is the ready-made traversal test. Its factory
always returns islands for three fixed cells, optionally populates other cells,
and starts the main camera in fly mode over open sea facing the central island.
Use WASD to fly, hold Shift for a 2x flight boost, press V to toggle fly mode,
and press Escape for the overview.

The factory uses the world's deterministic seed to sparsely populate 2 km ocean
cells beyond the three fixed test islands. The overlay reports known, loaded,
queued, and generating island
counts; keep flying into open sea to exercise look-ahead generation, unload,
and deterministic return behavior. At most three island runtimes are resident;
the least relevant non-focused island is released before another generation is
allowed to allocate its native handle, meshes, materials, and textures.

The island may be translated and rotated around the Y axis. Generated content,
streaming cells, colliders, materials, rivers, and decoration remain in the
component's local coordinate system. Unit scale is currently required; the
component reports an error rather than silently misaligning physics when a
different or non-uniform scale is used.

LOD 0 displays the same corrected support surface used by terrain queries
without retaining duplicate render geometry. Hydraulic erosion, coastal
erosion, adaptive terrain tessellation, rivers, and waterfalls remain active.

Enable **Show Mesh Edges** to render the generated triangle edges
without allocating duplicate line meshes. The setting remains active when you
enter first-person mode and applies automatically to newly streamed LOD tiles.
Press **M** in either overview or first-person mode to toggle mesh edges without
using the overlay.

Click the overview terrain to enter first-person mode. The current LOD 2 tile
and its neighbours are each split into an 8x8 LOD 1 group. The current LOD 1
tile and its neighbours are each split again into 8x8 LOD 0 groups. Only the
nearby 3x3 LOD 1 neighbourhood has collision. Each logical 31.25-metre square
uses a hidden Unity `TerrainCollider` backed by a 129x129 heightfield sampled from
the final LOD 0 surface. The whole-island 8193x8193 source lattice is prepared
on the generation worker, and adjacent tiles copy the same shared-edge samples.
Incoming colliders are enabled before outgoing colliders are retired, and
crossing the finer LOD 0 render boundaries performs no collider cooking or
replacement. Rust also derives one fitted capsule from each final central
trunk and exports its authoritative forest-tile owner. Unity creates those
capsules only for active LOD 0 forest cells and destroys them with the cell,
so distant LOD 1 and LOD 2 trees do not carry physics objects. Press M to toggle
mesh edges. Press Escape to discard
the refinement groups and return to the 64-tile LOD 2 overview. River surfaces
are clipped on the same 64x64 LOD 1 boundaries and render throughout every
active LOD 1 group, including its LOD 0 refinement cells. Rivers remain hidden
where the terrain is still LOD 2. Chunks are cached after their first
visit, so revisiting an area only changes visibility. First-person controls are
WASD, Shift to run, Space to jump, and the mouse to look. Press V to toggle the
configurable 24 m/s fly mode, which follows terrain or sea level at a 4 m
clearance. Press Tab to release the cursor for Inspector tuning, then Tab again
to resume movement and mouse look. The top-right minimap shows the 16-cell
radius around the player's current 2 km grid cell, using the request factory's
side-effect-free `HasIsland` query without constructing or queuing islands.
Click a minimap square to teleport to its centre in fly mode. Press Tab first
if the cursor is captured. The cursor stays released for repeated map clicks;
press Tab to resume flying. Terrain and sea clearance update as the destination
loads, and Escape returns to an overview of the selected cell. Dragging across
the minimap does not teleport or orbit the camera. Visibility changes apply
without regenerating.

The collision heightfield has one height per horizontal position. It closely
tracks the generated walkable surface but deliberately cannot represent
overhangs, vertical faces, caves, or multiple surfaces stacked at the same XZ
coordinate; those remain visible in the free-form render mesh.

Each accepted waterfall patch now retains its authoritative foot centre, flow
direction, half-width, and drop before the temporary placement data goes out of
scope. Unity copies those records on the generation worker and immediately
releases the native vector. No final-mesh sharpness scan, angle threshold, or
spacing suppression is involved. During upload Unity packs the feet into the
same 64x64 world partition as the LOD 0 terrain cells and creates a fixed pool
of 32 fog volumes. In first-person mode the pool considers only feet inside the
active 3x3 LOD 0 neighborhood, retains them within 220 metres, queries new feet
within 180 metres after five metres of movement, and reuses distant slots for
meaningfully closer waterfalls. Player movement performs no native call, does
not scan the full foot set, and allocates no query collections. Volumes are
hidden outside LOD 0, in overview mode, when river surfaces are hidden, and on
regeneration.

Each active foot receives one animated, depth-clipped proxy fog volume;
there are no particle emitters. Its width, depth, height, density, orientation,
and position derive from the authoritative waterfall width, drop, flow, and
lower water surface. Feet below sea level are lifted to the sea plane. A short
ray march forms a wide, coherent lower blanket that extends beyond both sides
of the impact line. Its density fragments into animated columns and wisps with
increasing height, while a separate coherent ceiling gives each rising section
a different reach. Scene-depth clipping keeps the result against visible
geometry. The compact volume is tucked slightly beneath the falling sheet and
partially veils the impact without retaining an ellipsoidal blob silhouette.
**Show waterfall feet** displays nearby authoritative positions,
their exported flow directions, the activation radius, and active assignments
when Game-view gizmos are enabled.

In first-person mode, grassy terrain gains a sixteen-layer shell-fur treatment
around the player. Grass remains at full density for ten metres, then fades
smoothly to zero over the following ten metres. Only intersecting LOD 0 tiles
receive grass renderers. The
shader uses the same material channels and noisy terrain boundaries as the
ground material, so grass is excluded from cliffs, beaches, river beds, snow,
and submerged terrain. Beneath the fur, grass ground is exposed as brown soil
within half a metre of the player and blends back to green over the following
two metres, making the gaps between nearby blades read as dirt.

Terrain vertex colours carry hardness/forced rock in red, loose cover in green,
river bed in blue, and cached sea proximity in alpha. The ground shader uses
the generated packed height channels for short view-dependent parallax-
occlusion ray marches on authored
rock and rounded-river-stone surfaces; albedo, normals, height and occlusion all
share the same shifted repeating UVs.

The fur shells bend in a coherent world-space wind field sampled from the
world environment's global weather-noise texture. The sea shader uses the same
texture for wave-domain and height variation, while terrain also reuses its
channels for grass coverage and broad colour variation. Each directional ocean
wave samples height noise at a scale proportional to its wavelength, with
separate offsets: broad swell varies in broad patches and finer waves in finer
patches. The ocean wave profile's noise world size sets the texture repeat for
the longest wave; amplitude variation controls the multiplier around each
wave's existing height (zero disables it).
Gusts advect along the global weather direction, bend progressively from fixed
roots to flexible tips, and perturb the lighting normals with the same moving
noise so highlights travel with the geometry. Beyond the fur radius, the
ordinary terrain grass uses that identical advected field to perturb only its
grass-covered lighting normals; non-grass materials remain still, and moving
highlights continue seamlessly into the distance. Direction, speed and gust size
belong to the global world environment, so adjacent islands share the same wind.
Wind speed is the single strength input: all vegetation uses the same response,
with root pinning and shape-dependent bending applied locally.
Runtime systems can update direction and speed immediately with
`IslandWorldManager.SetWind(direction, speedMetresPerSecond)`; no island
regeneration is required.

To drive weather from a scene script, derive a component from
`WorldWeatherDriver`, attach it to a GameObject, and assign that component to
the **Weather Driver** field on `IslandWorldManager` (or
`OceanWaveSandboxController` in the sea-only sandbox). The environment calls
`UpdateWeather` on the main thread before applying the frame's wind, waves and clouds.
The state starts from the scene's environment and cloud settings and ocean wave profile.
Edits affect the runtime state without modifying those authored settings or
profile assets. Disabled or unassigned drivers leave the last weather active.

```csharp
using UnityEngine;
using Motu.World;

public sealed class MyWeather : WorldWeatherDriver
{
    private float elapsedSeconds;

    public override void UpdateWeather(ref WorldWeatherState weather, float deltaTime)
    {
        elapsedSeconds += deltaTime;
        float storm = 0.5f - 0.5f * Mathf.Cos(elapsedSeconds * 0.02f);
        weather.WindDirection = new Vector2(1f, 0.25f);
        weather.WindSpeedMetresPerSecond = Mathf.Lerp(5f, 24f, storm);
        weather.WindGustSizeMetres = Mathf.Lerp(12f, 30f, storm);
        weather.Waves.Wave0.AmplitudeMetres = Mathf.Lerp(0.2f, 0.8f, storm);
        weather.Waves.AmplitudeVariation = Mathf.Lerp(0.25f, 0.7f, storm);
        weather.Waves.WhitecapCoverage = Mathf.Lerp(0.2f, 0.8f, storm);
        weather.Clouds.Coverage = Mathf.Lerp(0.2f, 0.9f, storm);
        weather.Clouds.Density = Mathf.Lerp(1f, 5f, storm);
        weather.Clouds.ShadowStrength = Mathf.Lerp(0.3f, 0.85f, storm);
    }
}
```

`WorldWeatherState.WindSpeedMetresPerSecond` is the single shared wind-strength
control for all vegetation, clouds and waves. Zero stops wind-driven motion.
`WindDirection` is the direction wind blows towards in world X/Z coordinates;
clouds, gusts and vegetation bending agree on this heading. Gusts vary strength,
without rotating the direction. Grass, trees, reeds and ferns share one displacement
response; their root pinning and shape-dependent bending remain intact. The state
also exposes gust size and tree bend heights. Its `Waves` field exposes all four
directional components (direction,
wavelength, amplitude, speed and choppiness), wave noise, whitecap settings,
onshore breaking, wave enable switches and coastal attenuation curves. Wave
amplitudes and speeds are still authored at the reference wind speed of 9 m/s;
the global wind response scales them in the shader. `deltaTime` is in unscaled
seconds, matching the existing environment clock.

`Clouds` exposes enabled state, coverage, density, altitude, vertical thickness,
world size, broad noise scale/strength, detail and edge erosion, day/sunset/night
colours, and direct/ambient shadows, celestial obscuration and low-elevation shadow
fade. Cloud drift uses the shared wind direction and speed. These changes apply
immediately without regenerating the cloud texture or mutating authored settings.
Weather-map seed and resolution stay in the authored cloud settings. The sea-only
sandbox retains cloud state but has no cloud renderer.

For a one-off update from any script, copy `world.Weather`, edit the fields and
call `world.ApplyWeather(weather)` on the main thread. The same API is available
on `WorldEnvironmentController` and the ocean sandbox. Assign or replace a
driver at runtime through `world.WeatherDriver`. An enabled driver runs each
frame and may override fields changed by a one-off update.

The scripting state does not expose mesh spacing, rings, radii, fade distances
or mask resolution/layout. Weather changes reuse the installed mesh and
coastal textures, updating culling bounds when wave heights change. Only edits
to the coastal attenuation curves mark the mask for recomposition.

Per-wave direction and wavelength changes fade between two fixed
wave patterns over four seconds. Requests made during a fade are picked up at
the start of the next fade, so a continuously changing weather driver cannot
rotate an already-visible pattern around the world origin. The same blend
drives displacement and lighting normals. Wave speed changes advance the
existing phase from that point onward instead of recalculating travel from
the scene's total elapsed time. Shore-wave phase and foam travel also accumulate
continuously. This animation advances once per frame using `Time.deltaTime`;
the weather driver's clock continues to use unscaled time as described above.

Runtime integration validation:
`-executeMethod Motu.Editor.WorldWeatherValidation.BatchValidateRuntimeWeather`.
GPU transition and phase validation (requires graphics):
`-executeMethod Motu.Editor.OceanWaveTransitionValidation.BatchValidateWaveTransitions`.

Every 8x8 group is geometrically clipped at its tile boundaries. LOD 0 uses an
attribute-carrying 3D plane clipper, so vertical faces and multiple heights at
one XY location survive slicing. Only LOD 0 and LOD 1 edges bordering an active
lower-detail neighbour are morphed onto that coarser support surface. Edges
shared by two groups at the same LOD retain their full detail. At final island
creation, LOD 1 and LOD 2 are each tessellated once more and the inserted
midpoints are projected onto the final LOD 0 surface. This leaves a smaller
density and silhouette step between adjacent LODs before Unity applies its
edge-only transition morph.
Terrain render and collider exports are additionally clipped five metres below
the sea plane. Crossing faces end on a shared interpolated boundary, and deeper
faces and unused vertices are omitted from Unity without changing the full
terrain retained by Rust for maps and generation.

Sediment deposition has separate strength and slope controls. At the default
12-degree limit, deposition is strongest below 4 degrees, fades smoothly
across moderate slopes, and reaches zero at 12 degrees. Raising the limit lets
sediment settle on progressively steeper terrain.

All terrain LODs share one `Motu/Terrain Unified` material and the same
2048x2048 world-space normal and directional ambient-occlusion maps. The maps
are sampled with global terrain UVs, so their colour and lighting do not jump
at a tile or LOD boundary. LOD 0 disables the sampled normal per renderer and
uses its own geometric normals; LOD 1 and LOD 2 use the world-space normal map
derived from the final LOD 0 terrain.

Rock, riverbed, forest-floor, and fallen-stones textures are baked in memory by
the Rust library for each island and use a single top-down XZ projection. Unity
selects deterministic linear dirt and stone colours first, passes those values
to the background bake, and uses the same values for shader fallbacks. Settled
rocks mark their supporting terrain through UV1.y; that
switch selects the dedicated `FallenStones` recipe, whose coherent gravel
clusters sit on packed dirt and exclude close grass using the same noisy,
height-shaped boundary.

Authored colour, normal, height, and occlusion are fully visible on horizontal
ground. Rock fades into simpler procedural stone through a broad slope band;
macro-scale coherent 3D noise perturbs that blend, while an additional
mid-scale layer breaks up repeated top-down rock textures. The configured fade
slope defaults to 45 degrees. Packed height shapes the middle of material
transitions without changing either endpoint, so raised details retain the
authored surface longer while recesses reveal the underlying ground sooner.
Steeper faces therefore avoid stretched top-down textures. Each authored linear
mask stores height in red and occlusion in green. At startup Unity combines the
rock and river masks as `RG/BA`, then does the same for forest floor and fallen
stones. The runtime terrain shader consequently uses two mask samplers instead
of four while each material retains its own UV scale and parallax sampling.
The generated render textures are linear where appropriate, repeating,
mipmapped, and released whenever runtime materials are rebuilt or destroyed.
Texture upload and dual-mask packing happen on Unity's main thread only; recipe
evaluation happens in the existing generation worker. Runtime islands do not
depend on Unity editor bake windows or files under `Assets/Generated/Textures`.

Grass fur keeps hard rock, beach, and snow exclusions so blades never protrude
through those surfaces; its river edge retains stable whole-blade stippling.
The underlying terrain shader is intentionally softer: the same coherent grass
field blends green ground continuously into bare dirt and the neighbouring
surface materials.

Terrain mesh colours carry material data rather than a visible tint. Red is
normalized bedrock hardness, green is loose/deposited cover, blue marks river
bed, and alpha is cached distance-from-sea strength. Alpha is one on connected-sea vertices and
remains one through two metres of LOD 0 mesh edges, then fades linearly to zero
at twenty metres; it is calculated
before final river tracing and carving, then interpolated onto any vertices the
river refinement adds. Sharp terrain forces red to one and green to zero while
retaining the independent river-bed and coastal values.
Rust samples the authoritative final LOD 0 field after each tile is clipped, so
reordered and newly created boundary vertices receive matching values. The
unified shader uses these channels to expose harder rock on slopes, colour
loose coastal deposits within the twenty-metre sea-proximity field behind the
same coherent noise boundary, retain full sand eligibility through two metres
of elevation, and fade that eligibility to zero at four metres. A separate
two-channel UV field marks forest-floor and fallen-stone support triangles.
Grass ground uses coherent micro-normal
relief at six times the stone detail frequency; beach sand uses eight-times finer
and less strongly perturbed relief. These change nearby lighting without changing
mesh geometry.

The sandbox camera also runs real-time screen-space ambient occlusion in the
Built-in Render Pipeline. It reconstructs nearby opaque geometry from the
camera depth-normal texture, evaluates a fixed sphere-sample kernel at half
resolution, and applies a depth- and normal-aware blur before transparent water
is drawn. This adds live contact shading to terrain folds, cliff joins, and
streamed rocks without baking another island texture. Tune the
`RealTimeAmbientOcclusion` component on **Main Camera**: `Intensity` controls
darkening, `Radius` is the world-space reach in metres, and `Quality` selects
6, 10, or 12 samples. Disable `Half Resolution` for the sharpest result at a
higher GPU cost; setting `Intensity` to zero bypasses the effect.

The same camera drives a half-resolution planar reflection camera mirrored
across the island's sea plane. It renders opaque terrain, vegetation, and rocks
into a reusable HDR texture before the viewer camera draws the water. Sea water
samples that scene reflection with animated ripple distortion and Fresnel
falloff; river water blends it in only through the near-sea estuary band because
the inland river and waterfall surfaces do not share the sea's reflection
plane. The generated sea, river tiles, and waterfall fog volumes use Unity's
`Water` layer, which the reflection camera excludes to prevent water from
reflecting itself. Tune `Resolution Scale`, `Clip Plane Offset`, and
`Reflection Layers` on `PlanarWaterReflection` on **Main Camera**. The existing
sky-colour reflection remains the fallback outside the reflection texture or
when the component is disabled.

The player-relative deep ocean performs reflection, refraction, distortion, and
depth opacity once without depending on any island mask. Each island adds a
bounded, edge-faded coastal overlay just above it; this overlay owns the sea
mask, shallow tint, shore waves, and foam without repeating the ocean GrabPass.
Each of the four ocean waves has its own world-space direction, controlled by
`weather.Waves.Wave0.Direction` through `Wave3.Direction`. Changing
`weather.WindDirection` does not rotate these waves or align them to the first
wave. Global wind speed still scales wave height and travel speed from the
authored 9 m/s reference values. The same global wind
also advects clouds and drives grass, reeds, ferns, foliage, and wood sway.
Generated island content is installed below a self-contained `IslandRuntime`.
It owns the native handle, terrain streamer, per-island materials, generated
textures, colliders, vegetation, rivers, waterfall effects, and coastal
overlay. Installation remains inactive until required resources are ready, and
clearing or a failed partial installation disposes that island without changing
the global sky or deep ocean. `IslandGenerator` remains the inspector-compatible
origin-island wrapper.

The sea mask's green channel stores linear distance from land over **128 metres**.
Geometric onshore waves fade in travelling from 128 to 96 metres offshore,
remain at full strength until 16 metres, then soften towards land. Their direction
comes from the land-distance gradient, scaled in world metres so mask resolution
and coverage do not change wave strength. Their phase also uses actual metres,
keeping authored wavelength and speed independent of the mask range. Scripted
onshore amplitudes have no fixed 4-metre input cap, but the combined wave field
is scaled to the depth map to keep troughs above the seabed. This map covers
0-5 metres, so the same conservative 5-metre displacement limit also applies in
deeper water. Normals use the same depth scaling; breaker foam instead follows shallow-water breaking so it remains visible as displacement shrinks. Carved river
channels still suppress waves. Ordinary
swell attenuation retains its original 16-metre distance weighting.

Incoming coastal overlay waves average red depth proximity with land proximity
rescaled to the original 16-metre band, breaking up coherent flashing across broad
shallow water. The weaker echo travels back offshore across the full 128-metre
range. Echo spacing retains its original 16-metre-to-incoming-range ratio, so the
wider band contains more waves at the same spacing and physical travel speed.
Their individually reduced strengths are added, making crossings brighter than
either wave alone.
Tune `Incoming Shore Wave Strength` and `Reverse Shore Echo Strength` on the sea
material independently. The geometric onshore component compresses its leading,
shore-facing rise over a configurable depth range, while
retaining a rounded rear face. `Leading Edge Sharpness` controls the maximum
asymmetry. `Breaking Start Depth Metres` defaults to 5 m and `Breaking Full Depth
Metres` defaults to 3.5 m, making full breaking occur while the wave still has
substantial height. Raise the full depth to break earlier; lower it for a longer
approach. Both are limited to the depth map's 0-5 m range, with full depth below
start depth. Scripts can set `weather.Waves.OnshoreWaveBreakingStartDepthMetres`
and `weather.Waves.OnshoreWaveBreakingFullDepthMetres` at runtime.
`Sharpening Distance Metres` bounds the offshore breaker region
(default 96 metres, up to 128 metres), fading its outer 20 percent; depth controls
breaking within that region. Breaker foam follows the upper leading face and
crest, grows with shallow-water breaking, and is independent of the depth scale
that shrinks geometry. It fades through the last 15 cm of water, reaches zero at
2 cm, and remains suppressed on land, in carved rivers, and without active waves.
Wave contours remain independent of the camera Z buffer; camera depth is
responsible only for water opacity. The former river-mouth and estuary silt
coloration has been removed. Depth and accumulated-edge land distance are both
barycentrically interpolated over the final planar LOD 0 triangles. The land
distance field is constructed only after every island generation stage has
finished. The same generated RGBA mask stores the union of finalized submerged
river-bed coverage and every earlier river carve that lowered terrain below sea
level in blue. This accumulated field is carried through later tessellations and
includes outlet channels used to escape inland basins. The global ocean compositor
expands the combined suppression by one final-terrain neighbour ring, then uses
that channel to fade both ordinary and onshore geometric waves out of carved
channels while retaining depth for seabed protection. Shallow-water attenuation
and distance fading also shape wave strength.

## Rebuild the native plugin

On macOS, after changing `island-rs`, run the deployment helper. It explicitly
builds without the experimental GPU feature, atomically installs the library,
and checks the installed bytes:

```sh
../island-rs/deploy-unity.sh
```

The included plugin is built for Apple Silicon. Other platforms need their own
Rust `cdylib` in the corresponding Unity plugin folder. Restart Unity after
deployment because the editor does not hot-reload native libraries.

Generated islands are cached as complete native snapshots under
`Application.persistentDataPath/GeneratedIslandCache`. A cache hit restores the
terrain, spatial index, LODs, rivers, waterfall data, vegetation meshes, and
decorations without rerunning terrain generation. Snapshot files are versioned,
Zstandard-compressed, checksummed, written through a temporary file, and keyed
from every geometry-generation input. `Island Generation > Use Snapshot Cache`
can disable the cache; its shared LRU byte budget defaults to 8 GiB. Invalid or
obsolete snapshots are discarded and safely fall back to CPU generation.
The six palette-dependent terrain map sets and the runtime `PlateBark` tree map
set are cached together by Rust's embedded-recipe revision, palette values,
normal convention, and resolution. Tree wood is created directly from its
shader and receives the generated bark albedo, height, normal, and occlusion
maps; it does not depend on an authored Unity material or imported bark maps.
This lets islands with matching palettes share one checked material bundle and
means a returning island does not rebake its procedural material recipes.

Island parameters live on the request-factory component, not on runtime
`IslandGenerator` objects or a shared single-island configuration asset. A
custom factory can therefore expose exactly the controls required by its world
model and derive different settings for every grid cell. Streaming and the
global environment remain responsibilities of `IslandWorldManager`.

For an editor compile plus native ABI, streamed tile, UV, support mesh,
waterfall-foot export, fog-pool, and collider-cooking check, run:

```sh
/Applications/Unity/Hub/Editor/6000.5.6f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -nographics -projectPath "$PWD" \
  -executeMethod Motu.Editor.UnityValidation.Run -quit
```


## Code organization and validation

Runtime code lives under `Assets/Scripts` in `Motu.World`, `Motu.Islands`,
`Motu.Settings`, `Motu.Interop`, `Motu.Streaming`, `Motu.Rendering` and
`Motu.Gameplay`. All runtime scripts compile into `Motu.Runtime`.
Editor tooling and tests compile separately into `Motu.Editor` and
`Motu.Tests.Editor`; tests and GPU probes live in `Assets/Tests/Editor`.
Script GUIDs and serialized setting names are preserved.

Custom weather scripts should import `Motu.World` and derive from
`WorldWeatherDriver`. `WorldWeatherState`, `OceanWaveWeatherSettings` and
`CloudWeatherSettings` are in `Motu.World`; authored profiles/settings are in
`Motu.Settings`. The world owns the shared sky, ocean, clouds and wind resources.
An island receives its environment from `IslandWorldManager`.

Run `./validate.sh` from this directory to import a clean temporary project,
run the editor contract tests (including native generation/cancellation), and
build a macOS development player. The script retains its temporary project,
logs and test XML and excludes `_Recovery`. It uses the version in
`ProjectSettings/ProjectVersion.txt`; set `UNITY_EDITOR_EXE` to override the
editor executable. `./validate.sh --native-exports` also runs the dedicated
river fixture and broader native export/render suite.

For an already imported project, the stable batch entry point is
`-executeMethod Motu.Editor.UnityValidation.Run`; the player build entry point is
`-executeMethod Motu.Editor.UnityProjectBuild.BuildPlayer`. Set
`MOTU_PLAYER_OUTPUT` to override the default `Builds/Motu.app` output.
See [cleanup results](UNITY_CODE_CLEANUP_RESULTS.md) for scope and validation.
