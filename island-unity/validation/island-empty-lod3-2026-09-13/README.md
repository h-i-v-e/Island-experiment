# Empty LOD3 exports from submerged cells

Inspected the 15 most recently modified local native snapshots with the deployed plugin. Five returned no renderable triangles at LOD0, LOD1, LOD2 or LOD3. The first was a17f0861699fccacfbbf8e735825d2f2d1609717cc9b741ff59ed5ebedc320a4.motusnapshot. This confirms that empty horizon geometry is a valid depth-clipped result for these submerged cells, rather than an LOD3-only simplification loss.

Previously the mandatory generated-mesh copier rejected that valid empty export, aborting preparation. LOD3 now uses an optional copier, the prepared owner accepts an absent silhouette, and the runtime distinguishes completed LOD3 installation from presence of a renderer. Empty cells finish installation and can switch distance/residency without allocating a horizon mesh or material. Native export handles still require validity and are released in finally; mandatory generated meshes and malformed non-empty exports retain validation.

Regression coverage includes optional empty export conversion, missing vertices with non-empty indices, strict required-mesh rejection, empty renderer installation and repeated disposal. An isolated-only reproduction test loads the actual cached submerged snapshot through the complete preparation pipeline, invokes runtime installation, switches far/near/dormant states, and checks native handle release on unload. The ordinary generated island lifecycle and colour/rise tests are also exercised.

No Rust production changes or native plugin rebuild are required. No snapshot files were removed or regenerated.

Validation: Unity 6000.5.6f1 / Metal, isolated EditMode run: all 7 tests passed in 70.10 seconds, including the actual cached snapshot preparation/installation/unload. `git diff --check` passed. The isolated reproduction source is included here for reference; it is outside Assets and is not part of the portable test suite because it needs that local snapshot.
