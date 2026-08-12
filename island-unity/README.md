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
hydraulic-erosion, or per-stage river source thresholds. Coastal erosion cuts
exposed softer rock into bays and platforms; beach formation conservatively
redistributes that sediment toward sheltered shorelines. Higher river-threshold
values produce fewer source rivers. Slider changes take effect when you press
Generate. Drag to orbit, use the mouse wheel to zoom, and right-drag to pan.

Render-only cliff sharpening and its slider are disabled. LOD 0 displays the
same corrected support surface used by terrain queries, avoiding the inverted
patches previously introduced by the additional normal-retreat remeshing pass.
Hydraulic erosion, coastal erosion, adaptive terrain tessellation, rivers, and
waterfalls remain active.

Enable **Show mesh edges (wireframe)** to render the generated triangle edges
without allocating duplicate line meshes. The setting remains active when you
enter first-person mode and applies automatically to newly streamed LOD tiles.

Click the overview terrain to enter first-person mode. The current LOD 2 tile
and its neighbours are each split into an 8x8 LOD 1 group. The current LOD 1
tile and its neighbours are each split again into 8x8 LOD 0 groups. Only the
current LOD 0 tile has a `MeshCollider`; it moves as the player crosses tile
boundaries. The collider uses the true-3D tile by default and automatically
falls back to a separately exported support tile if Unity cannot cook it. The
overlay can force support collision for diagnostics. Press Escape to discard
the refinement groups and return to the 64-tile LOD 2 overview. River surfaces
are clipped on the same 64x64 LOD 1 boundaries and only the chunks inside the
active LOD 1 neighbourhood are rendered. Chunks are cached after their first
visit, so revisiting an area only changes visibility. First-person controls are
WASD, Shift to run, Space to jump, and the mouse to look.

Every 8x8 group is geometrically clipped at its tile boundaries. LOD 0 uses an
attribute-carrying 3D plane clipper, so vertical faces and multiple heights at
one XY location survive slicing. Only LOD 0 and LOD 1 edges bordering an active
lower-detail neighbour are morphed onto that coarser support surface. Edges
shared by two groups at the same LOD retain their full detail.

Sediment deposition has separate strength and slope controls. At the default
12-degree limit, deposition is strongest below 4 degrees, fades smoothly
across moderate slopes, and reaches zero at 12 degrees. Raising the limit lets
sediment settle on progressively steeper terrain.

All terrain LODs bake the original directional ambient-occlusion texture.
LOD 1 and LOD 2 also bake high-detail normal corrections from LOD 0. The
viewer uses the original resolution progression: 2048x2048 AO for LOD 0,
1024x1024 normal and AO maps for LOD 1, and 512x512 normal and AO maps for
LOD 2. LOD 0 continues to use its full geometric normals directly.

## Rebuild the native plugin

On macOS, after changing `island-rs`, run:

```sh
cargo build --release --manifest-path ../island-rs/Cargo.toml
cp ../island-rs/target/release/libmotu.dylib Assets/Plugins/macOS/
```

The included plugin is built for Apple Silicon. Other platforms need their own
Rust `cdylib` in the corresponding Unity plugin folder.

For an editor compile plus native ABI, streamed tile, UV, support mesh, and
collider-cooking check, run:

```sh
/Applications/Unity/Hub/Editor/6000.5.6f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -nographics -projectPath "$PWD" \
  -executeMethod IslandViewer.BatchValidateNativeInterop -quit
```
