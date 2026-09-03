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

## Open and run

1. Double-click `Open Island Unity.command`. This bypasses a known local
   mismatch between Hub's Licensing Client 1.17.4 and the editor's 1.18.1
   protocol. macOS might ask you to confirm opening it the first time.
2. Open `Assets/Scenes/IslandSandbox.unity`.
3. Select the **Island** GameObject to inspect the labelled generation,
   rendering, streaming, decoration, and debug settings.
4. Press Play. **Generate On Start** creates the configured island on the CPU.

The project can also be opened normally from Hub after stale licensing clients
have exited. Use the launcher if Hub reports that it cannot connect to the
licensing service.

To add an island to another level, create a GameObject, attach
`IslandGenerator`, assign a player or camera Transform as **Streaming Target**,
and optionally assign material templates and texture overrides. The component
never creates a camera or light and never modifies the level's global render
settings. An empty level therefore remains empty. Runtime materials are cloned
per island before generated maps are assigned, so project assets are not
mutated. Tree and plant prefab arrays are visible extension points for the
upcoming vegetation phase; they are not spawned yet.

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

For an authored open-sea test, add an `IslandWorldManager` to a parent object
and place two or three `IslandGenerator` objects beneath it at different XZ
positions. If its authored list is empty, the manager discovers those child
generators; otherwise the list controls their stable IDs, world cells, and
start-generation policy. The first entry (or the explicit environment authority)
owns the one global sky, sea, clouds, and solar clock. Islands generate serially,
retain independent materials, coast masks, and native handles, become dormant
outside the active radius, and can optionally unload beyond the unload radius.
First-person flight and terrain snapping automatically route through the manager
when one is present.

`Assets/Scenes/IslandsSandbox.unity` is the ready-made Phase 5 traversal test:
it contains three independently seeded islands and starts the main camera in fly
mode over open sea, facing the central island. Use WASD to fly, hold Shift for a
2x flight boost, press V to toggle fly mode, and press Escape for the overview.

The scene also enables Phase 6 procedural ocean discovery. A deterministic
world seed sparsely populates 5.2 km ocean cells beyond the three authored test
islands. The overlay reports known, loaded, queued, and generating island
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
to resume movement and mouse look. Visibility and grass-brightness changes
apply without regenerating.

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

The fur shells bend in a coherent world-space wind field sampled from the same
generated grass-noise texture used for coverage and broad colour variation.
Gusts advect along the configured direction, bend progressively from fixed
roots to flexible tips, and perturb the lighting normals with the same moving
noise so highlights travel with the geometry. Beyond the fur radius, the
ordinary terrain grass uses that identical advected field to perturb only its
grass-covered lighting normals; non-grass materials remain still, and moving
highlights continue seamlessly into the distance. Wind direction, maximum tip
bend, speed, gust size, and normal strength are live controls in the island's
Rendering settings and do not require regeneration.

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
Generated island content is installed below a self-contained `IslandRuntime`.
It owns the native handle, terrain streamer, per-island materials, generated
textures, colliders, vegetation, rivers, waterfall effects, and coastal
overlay. Installation remains inactive until required resources are ready, and
clearing or a failed partial installation disposes that island without changing
the global sky or deep ocean. `IslandGenerator` remains the inspector-compatible
origin-island wrapper.

Incoming coastal waves average the sea mask's red depth proximity with inverted
green land proximity, breaking up coherent flashing across broad shallow water.
Green stores distance from land over sixteen metres and also drives a separate,
weaker wave echo travelling back offshore across that full range. Echo spacing
is scaled by the ratio between its sixteen-metre range and the incoming range,
so both trains contain approximately the same number of broad waves and retain
the same physical travel speed. Their individually reduced strengths are added,
making crossings brighter than either wave alone. Tune `Incoming Shore Wave
Strength` and `Reverse Shore Echo Strength` on the sea material independently.
Wave contours remain independent of the camera Z buffer; camera depth is
responsible only for water opacity. The former river-mouth and estuary silt
coloration has been removed. Depth and accumulated-edge land distance are both
barycentrically interpolated over the final planar LOD 0 triangles. The land
distance field is constructed only after every island generation stage has
finished.

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

For an editor compile plus native ABI, streamed tile, UV, support mesh,
waterfall-foot export, fog-pool, and collider-cooking check, run:

```sh
/Applications/Unity/Hub/Editor/6000.5.6f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -nographics -projectPath "$PWD" \
  -executeMethod IslandGeneratorValidation.BatchValidateNativeInterop -quit
```
