# Island Unity project

A conventional scene-based Unity 6 project for the Rust generator in
`../island-rs`. The reusable `IslandGenerator` component invokes the Rust C ABI
and displays the irregular terrain, streamed detail, rivers, sea, vegetation
shells, rocks, and hidden terrain colliders beneath its GameObject. River-bed
stones and physically settled dropped rocks share one native-generated,
tile-streamed mesh; rocks no longer use a separate GameObject renderer pool.

## Open and run

1. Double-click `Open Island Unity.command`. This bypasses a known local
   mismatch between Hub's Licensing Client 1.17.4 and the editor's 1.18.1
   protocol. macOS might ask you to confirm opening it the first time.
2. Open `Assets/Scenes/IslandSandbox.unity`.
3. Select the **Island** GameObject to inspect the labelled generation,
   rendering, streaming, decoration, and debug settings.
4. Press Play. **Generate On Start** creates the configured island.

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
replacement. Press M to toggle mesh edges. Press Escape to discard
the refinement groups and return to the 64-tile LOD 2 overview. River surfaces
are clipped on the same 64x64 LOD 1 boundaries and render throughout every
active LOD 1 group, including its LOD 0 refinement cells. Rivers remain hidden
where the terrain is still LOD 2. Chunks are cached after their first
visit, so revisiting an area only changes visibility. First-person controls are
WASD, Shift to run, Space to jump, and the mouse to look. Press Tab to release
the cursor for Inspector tuning, then Tab again to resume movement and mouse
look. Visibility and grass-brightness changes apply without regenerating.

The collision heightfield has one height per horizontal position. It closely
tracks the generated walkable surface but deliberately cannot represent
overhangs, vertical faces, caves, or multiple surfaces stacked at the same XZ
coordinate; those remain visible in the free-form render mesh.

Rust also classifies sharp transitions between adjacent final river faces into
deterministic rough-water locations before the river is sliced. This selects
the top and bottom lips of waterfalls instead of their coplanar vertical faces.
Unity copies those records on the
generation worker and immediately releases the native vector. During upload it
packs them into the same 64x64 world partition as the LOD 1 river tiles and
creates a fixed pool of 32 particle systems. In first-person mode the pool
allows generated rough-water locations to be as close as one metre apart and
retains locations within 220 metres, queries new locations within 180 metres
after five metres of movement, and reuses distant slots for meaningfully closer
waterfalls or constricted river edges. Player movement performs no native call,
does not scan the full candidate set, and allocates no query collections.
Particles are cleared in overview mode, when river surfaces are hidden, and on
regeneration. Origins receive a ten-centimetre vertical clearance above the
water mesh; their spray direction still follows the exported river normal.
The spray uses a dedicated alpha-blended shader that renders each billboard as
a feathered circle rather than exposing its square quad. Launch speed is capped
at 1.35 metres per second, particles live for 0.7-1.6 seconds, and their
maximum diameter is 12 centimetres, keeping the effect close to the water.
Each source now uses a broad 80-degree cone, adds modest random-direction
variation, and varies launch speed from 40 to 100 percent per particle. This
breaks up hose-like streams without increasing their maximum travel distance.
**Show rough-water emitter debug** displays nearby candidates,
their exported outflow normals, the activation radius, and active assignments
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

Rock and riverbed asset textures use a single top-down XZ projection. Their
procedural fallback colours are derived from the average colours of their
assigned colour maps when the island's runtime materials are built. Streamed
stones use the same derived rock colour so they continue to match the cliffs.
Their colour, authored normal, and occlusion contributions are fully visible on
horizontal ground and fade together into the simpler procedural stone through
a broad slope band. Macro-scale coherent 3D noise perturbs that blend most near
steep faces, while an additional mid-scale layer forms irregular patches of
authored and simple stone across flat rock to break up repeated top-down
textures. The configured fade slope defaults to 45 degrees, and the broadened
transition reaches farther onto steep terrain. The packed height map
shapes the middle of each transition without changing either endpoint, so
raised details retain the authored surface longer while recesses return to the
procedural surface sooner. Steeper faces therefore retain the original solid
rock colour and procedural noise normal instead of receiving a stretched or
alternate-axis projection. Packed linear mask maps store height
in red and occlusion in green. Open
**Island > Terrain > Pack Height +
Occlusion Mask**, drag the two grayscale source textures into the window, and
save the PNG inside `Assets`. The utility temporarily reads the sources as
uncompressed linear data, restores their import settings, and configures the
packed output as a linear repeating texture. It can also assign the result
directly to either mask property on `IslandTerrain.mat`. A missing height input
uses neutral 50-percent gray; a missing occlusion input uses white.

Grass fur keeps hard rock, beach, and snow exclusions so blades never protrude
through those surfaces; its river edge retains stable whole-blade stippling.
The underlying terrain shader is intentionally softer: the same coherent grass
field blends green ground continuously into bare dirt and the neighbouring
surface materials.

Terrain mesh colours carry material data rather than a visible tint. Red is
normalized bedrock hardness, green is loose/deposited cover, and blue is a
cached distance-from-sea strength. Blue is one on connected-sea vertices and
remains one through two metres of LOD 0 mesh edges, then fades linearly to zero
at twenty metres; it is calculated
before final river tracing and carving, then interpolated onto any vertices the
river refinement adds. Sharp terrain and all river-bed vertices force red to
one and green to zero while retaining the blue coastal value.
Rust samples the authoritative final LOD 0 field after each tile is clipped, so
reordered and newly created boundary vertices receive matching values. The
unified shader uses these channels to expose harder rock on slopes, colour
loose coastal deposits within the twenty-metre sea-proximity field behind the
same coherent noise boundary, retain full sand eligibility through two metres
of elevation, and fade that eligibility to zero at four metres. River beds are
treated as exposed rock consistently across all LODs. Grass ground uses coherent micro-normal
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
plane. The generated sea, river tiles, and rough-water particles use Unity's
`Water` layer, which the reflection camera excludes to prevent water from
reflecting itself. Tune `Resolution Scale`, `Clip Plane Offset`, and
`Reflection Layers` on `PlanarWaterReflection` on **Main Camera**. The existing
sky-colour reflection remains the fallback outside the reflection texture or
when the component is disabled.

Sea-wave phase and shoreline fading use the generated sea mask's red channel,
which encodes seabed depth from sea level through five metres. This keeps wave
contours fixed to the island rather than reconstructing their depth from the
camera Z buffer; camera depth remains responsible only for water opacity.

## Rebuild the native plugin

On macOS, after changing `island-rs`, run:

```sh
cargo build --release --manifest-path ../island-rs/Cargo.toml
cp ../island-rs/target/release/libmotu.dylib Assets/Plugins/macOS/
```

The included plugin is built for Apple Silicon. Other platforms need their own
Rust `cdylib` in the corresponding Unity plugin folder.

For an editor compile plus native ABI, streamed tile, UV, support mesh,
rough-water emitter, and collider-cooking check, run:

```sh
/Applications/Unity/Hub/Editor/6000.5.6f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -nographics -projectPath "$PWD" \
  -executeMethod IslandGeneratorValidation.BatchValidateNativeInterop -quit
```
