# Island Unity viewer

A minimal Unity 6 viewer for the Rust project in `../island-rs`. It invokes the
Rust generator through its C ABI and displays the irregular, detail-tessellated
terrain and carved river strip directly as Unity meshes.

## Open and run

1. Double-click `Open Island Unity.command`. This bypasses a known local
   mismatch between Hub's Licensing Client 1.17.4 and the editor's 1.18.1
   protocol. macOS might ask you to confirm opening it the first time.
2. Open any empty scene, or create one if Unity prompts you.
3. Press Play. The viewer bootstraps itself; no scene setup is required.

The project can also be opened normally from Hub after stale licensing clients
have exited. Use the launcher if Hub reports that it cannot connect to the
licensing service.

Generation builds the island, all three texture sets, the LOD1-clipped river
tiles, and the 8x8 LOD 2 overview on a background worker. The existing island
remains visible while regeneration runs, and the overlay reports elapsed time.
Unity texture and mesh objects are then uploaded on the main thread, with the
64 overview tiles spread across frames to avoid a large upload hitch. Use the
overlay to regenerate another seed and adjust terrain, coastal evolution,
hydraulic-erosion, or river source selection. Coastal erosion cuts
exposed softer rock into bays and platforms; beach formation conservatively
redistributes that sediment toward sheltered shorelines. Source catchment is a
percentage of the current pass's land vertices, so one slider remains consistent
across all mesh densities. A second control suppresses small sources on steep
slopes while retaining sufficiently large catchments. Slider changes take effect when you press
Generate. Drag to orbit, use the mouse wheel to zoom, and right-drag to pan.
In first-person mode, terrain, grass, rivers, and sea fade into sky-coloured
linear distance haze from the camera position to full strength at one kilometre.
The overview remains haze-free.

LOD 0 displays the same corrected support surface used by terrain queries
without retaining duplicate render geometry. Hydraulic erosion, coastal
erosion, adaptive terrain tessellation, rivers, and waterfalls remain active.

Enable **Show mesh edges (wireframe)** to render the generated triangle edges
without allocating duplicate line meshes. The setting remains active when you
enter first-person mode and applies automatically to newly streamed LOD tiles.
Press **M** in either overview or first-person mode to toggle mesh edges without
using the overlay.

Click the overview terrain to enter first-person mode. The current LOD 2 tile
and its neighbours are each split into an 8x8 LOD 1 group. The current LOD 1
tile and its neighbours are each split again into 8x8 LOD 0 groups. Only the
current LOD 0 tile has a `MeshCollider`; it moves as the player crosses tile
boundaries. The collider uses the true-3D tile by default and automatically
falls back to a separately exported support tile if Unity cannot cook it. The
overlay can force support collision for diagnostics. Press M to toggle mesh
edges. Press Escape to discard
the refinement groups and return to the 64-tile LOD 2 overview. River surfaces
are clipped on the same 64x64 LOD 1 boundaries and render throughout every
active LOD 1 group, including its LOD 0 refinement cells. Rivers remain hidden
where the terrain is still LOD 2. Chunks are cached after their first
visit, so revisiting an area only changes visibility. First-person controls are
WASD, Shift to run, Space to jump, and the mouse to look. Press Tab to release
the cursor for the live grass-brightness slider, then Tab again to resume
movement and mouse look. Brightness changes apply without regenerating.

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

Every 8x8 group is geometrically clipped at its tile boundaries. LOD 0 uses an
attribute-carrying 3D plane clipper, so vertical faces and multiple heights at
one XY location survive slicing. Only LOD 0 and LOD 1 edges bordering an active
lower-detail neighbour are morphed onto that coarser support surface. Edges
shared by two groups at the same LOD retain their full detail. At final island
creation, LOD 1 and LOD 2 are each tessellated once more and the inserted
midpoints are projected onto the final LOD 0 surface. This leaves a smaller
density and silhouette step between adjacent LODs before Unity applies its
edge-only transition morph.

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

Terrain mesh colours carry material data rather than a visible tint. Red is
bedrock hardness in the `0..1` range, with `2` reserved for sharp vertices that
the final Rust geometry pass marks as exposed rock; green is loose/deposited
cover, and blue is final river-bed coverage. Rust samples the authoritative
final LOD 0 field after each tile is clipped, so reordered and newly created
boundary vertices receive matching values. The unified shader uses these channels to expose harder rock on slopes,
colour sheltered coastal deposits below a noisy one-metre beach line, and treat
river beds as exposed rock consistently across all LODs. Grass ground uses coherent micro-normal
relief at six times the stone detail frequency; beach sand uses eight-times finer
and less strongly perturbed relief. These change nearby lighting without changing
mesh geometry.

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
  -executeMethod IslandViewer.BatchValidateNativeInterop -quit
```
