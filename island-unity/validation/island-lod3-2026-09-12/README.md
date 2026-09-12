# Island horizon LOD3

LOD3 is derived on the preparation worker by simplifying the complete native LOD2 to a target of one quarter of its triangles. It retains the existing ten-metre render-floor cutoff and exports positions and indices only, through CreateIslandLod3Mesh/ReleaseMesh. It is not sliced into tiles or added to the snapshot format.

IslandRuntime owns one LOD3 mesh and material. World-space distance from the viewer/streaming target to the island centre (including altitude) selects LOD3 above 2,000 m; exactly 2,000 m and below uses the existing tiled terrain. Detail roots, including vegetation, rivers, rocks and caves, are hidden together; pending refinement is cancelled. Dormant resident islands retain the silhouette until the existing unload boundary. The unlit, texture-free Island Horizon shader outputs the current atmospheric horizon colour without shadows or additional fog.

Validation:
- Rust terrain::lod::tests: 3 passed, including deterministic reduction, valid topology, unchanged source vertex positions and retained domain corners/peak.
- Rust ffi::tests::forest_grid_valid_lifecycle_on_a_small_island: passed, including the position-only LOD3 export and paired release.
- Strict no-default-features library Clippy and cargo fmt: passed.
- Isolated Unity 6000.5.6f1 / Metal: 4 tests passed (horizon shader rendering, native generation/cancellation/installation/unload, runtime ownership, supported scenes).
- Runtime checks cover 1,999 m / 2,000 m / 2,000.1 m, altitude, translated island centres, one active renderer, no normals or collider on LOD3, dormant/wake visibility, coastal-mask restoration, and mesh disposal.
- Generated fixture: LOD3 3,283 triangles versus tiled LOD2 18,769 triangles. The tiled count includes boundary clipping; the native simplification target uses the unsliced source.
- Built, deployed and isolated-copy libmotu.dylib SHA-256 all match: f76e912ca28b00843dd67fd372754b06f8b7168b842bd6f6b61f3bad4d146908.
- git diff --check: passed.

Interactive scene appearance has not been accepted in the user's editor. Restart Unity to load the rebuilt native plugin, then regenerate/reload islands; existing snapshots remain usable.
