# Terrain LOD seam stitching — 2026-09-14

The old boundary clamp projected existing fine vertices onto the coarse edge,
but did not split fine triangles where that edge changed slope. A regression
with two coarse boundary bends reproduced a triangular gap between the meshes.
The former generated-render test also selected a tile below the ocean-floor
cutoff and therefore did not actually check any rendered fine vertices.

Both LOD0-to-LOD1 and LOD1-to-LOD2 now use shared boundary stitching. Incident
fine triangles are split at coarse boundary samples, preserving winding and
interpolating UVs. Edge heights and normals then follow the coarse profile.
Boundary recognition also accounts for the LOD0 clipper's 1e-7 coordinate
rounding, including at small streaming-group boundaries. Interior tiles skip
the edge-splitting pass. LOD0 transition morphing now runs after clipping. Previously, a source
triangle spanning neighbouring groups was morphed differently for each group's
side mask before it was cut. The new regression reproduced a 0.1 normalized
height discrepancy at the same shared edge coordinate. Clipping first and
morphing the resulting vertices gives both groups the same boundary heights.
The coarse mesh is borrowed and generated tile positions/normals are updated
in place; UVs are retained.

Validation:

- `cargo test --manifest-path island-rs/Cargo.toml --no-default-features --test seam_diagnostic`: 4 passed. Covers the reproducer, generated visible land at both LOD transitions with every corner mask, sibling edges, and neighbouring groups with different side masks.
- `cargo test --manifest-path island-rs/Cargo.toml --no-default-features --lib mesh_clipper::tests`: 2 passed. Includes six pairs of neighbouring transition masks. Both clipping paths, all sides/corners, 1 and 8 subdivisions, full and streaming scales; checks actual boundary segments against the coarse surface, winding and attribute counts.
- `cargo test --manifest-path island-rs/Cargo.toml --no-default-features --lib mesh::tests`: 20 passed; 2 existing full-resolution tests remain ignored.
- Library and seam integration-test Clippy with `--no-default-features -- -D warnings`: passed.
- `island-rs/deploy-unity.sh`: release native library built and deployed, with matching build/deployment bytes.

Deployed SHA-256: `378669830fcc7db28216382314e6a6f9d0f31a830591d18435d21de112a4703d`.

No live Unity visual inspection was performed. Restart Unity to load the native
library and reload the islands. Snapshot invalidation is unnecessary: the fix
runs when render tiles are sliced from the cached island meshes.
