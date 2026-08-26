# Clustered Foliage Replacement Plan

## Objective

Replace per-tree foliage shells with one deterministic canopy drape per fine
streaming patch. Branch tips are the only projected control points. The coarse
drape is the authoritative control mesh: LOD0 selectively subdivides, smooths,
and coherently displaces only its perimeter band, while LOD1 and LOD2 use the
coarse topology directly.

The canopy change affects foliage generation and foliage batch ownership only.
A subsequent placement revision changes tree spacing independently; terrain
rejection, tree wood generation, material separation, and the existing Unity
forest LOD distances remain unchanged.

## Implementation status (2026-08-24)

Implemented across `src/clustered_foliage.rs`, `src/trees.rs`, and
`src/forest.rs`. The native/Unity batch boundary remains unchanged. During live
log inspection, the surrounding forest streamer scratch-list reentrancy and the
foliage Metal shadow-pass parameter were also corrected because they prevented
runtime LOD movement and shader compilation.

The 2026-08-24 inspection pass additionally:

- smooths the coarse blob's top and bottom Z fields for three bounded
  simultaneous iterations while pinning exact branch-tip underside supports;
- reduces per-sample upper-surface height jitter so LOD1/LOD2 are rounded as
  well as the tessellated LOD0 result;
- adds a lazy tree-only triangle-edge overlay, toggled with `N` in both the
  island sandbox and tree preview;
- applies a deterministic 2-metre exclusion boundary with a one-millimetre
  normalized-f32 comparison tolerance.

The subsequent canopy-budget revision replaces three-metre complete-link trunk
clusters with deterministic 64 x 64 spatial canopy patches and removes the
eight radial envelope samples previously created around every branch tip. Each
tip now contributes one underside control point to a patch-wide alpha surface.
For seed 666 with the IslandSandbox settings this changed the measured output:

- disconnected coarse foliage components: 8,254 to 403;
- coarse foliage triangles: 5,864,902 to 636,690;
- detailed foliage triangles: 23,459,608 to 2,546,760;
- accepted trees and wood topology: unchanged at 17,074 trees.

The displaced-anchor placement revision initially reduced that same seed to
15,126 trees under a 1.5-metre final-anchor exclusion zone. The subsequent
2-metre spacing and shader-beach exclusion revision reduces it to 9,771 trees;
the current canopy contains 400 disconnected coarse components, 371,396 coarse
triangles, and 1,485,584 detailed triangles.

The subsequent tree-surface shader revision gives wood and foliage a shared
island-local three-layer 3D-noise treatment. Broad, detail, and fine samples
perturb the lighting normal without changing geometry, while an independently
weighted coherent signal rotates bark and foliage hue. The island runtime and
standalone tree sandbox both bind the same generated noise field. A later
cutout revision removes the wood shader's position-based sine stripes and uses
only irregular layered noise for bark colour. Foliage uses a vertically
column-aligned noise alpha so upper and lower surfaces open together, and its
shadow pass clips with the identical mask. LOD geometry and ownership remain
unchanged.

Validated state:

- `cargo test --lib`: 190 passed, 1 intentionally ignored slow lifecycle test;
- `cargo clippy --all-targets -- -D warnings`: passed;
- Unity 6000.5.6f1 isolated batch compilation: passed;
- release and installed `libmotu.dylib` SHA-256:
  `36c6eb0fbb1ef7322e38fb23763f30d8e0a732bf7d78a4bb7c2752dfe02ea2ea`;
- required forest/tree native exports are present.

The user's interactive Unity editor was already open with a recovered backup
scene, so it was not terminated. It must be restarted to load the final native
library before claiming live visual LOD quality. Dense-cluster visual inspection
through LOD0, LOD1, and LOD2 is the only remaining manual gate.

## Final visual contract

| Visual LOD | Wood | Foliage |
| --- | --- | --- |
| LOD0 | Existing full tree wood | Subdivided and smoothed canopy drape |
| LOD1 | Existing simplified tree wood | Coarse canopy control mesh |
| LOD2 | Hidden | The same coarse canopy control mesh |

All LOD0 foliage remains in one native combined foliage stream, and all coarse
foliage remains in one native combined foliage stream. Canopy patches are range
and ownership metadata; they must not become individual Unity GameObjects.

## Units and deterministic inputs

- Tree and mesh coordinates remain in normalized island coordinates internally.
- Canopy ownership uses the same 64 x 64 normalized island grid as fine forest
  streaming. On the standard 2,000-metre island each patch is 31.25 metres
  wide.
- All trees whose roots occupy one patch contribute tips to the same canopy
  input. Alpha filtering, rather than root distance, preserves genuine gaps as
  disconnected surfaces inside that one mesh range.
- Stable terrain-vertex order is the primary ordering. Seeded variation must use
  a dedicated foliage seed domain and may not depend on map iteration.
- Branch-tip support points come from the final terminal wood rings after taper
  has been applied. The support position is each terminal ring barycentre.

## Phase 1: make branch tips authoritative

1. Extend the procedural tree result with local-space foliage support points.
2. Preserve the existing wood LOD meshes and wood correspondence data.
3. Remove foliage-cube construction, its RNG, cube-specific options, cube
   counters, cube anchors, and cube-specific tests.
4. Build a single-tree blob through the same cluster-blob builder for the
   procedural-tree preview and native `CreateProceduralTree` path. This ensures
   the preview exercises the production foliage algorithm rather than retaining
   a second foliage implementation.
5. Keep public mesh stream names stable where practical so the FFI and Unity
   preview do not require an ABI change.

## Phase 2: deterministic canopy ownership patches

1. Build canopy patches after forest placements and prototypes are known but
   before combined foliage streams are assembled.
2. Assign each root to its clamped normalized 64 x 64 cell.
3. Group every tree in that cell into one canopy input; do not create a foliage
   shell per tree.
4. Process patches in stable cell order and members in terrain-vertex order.
5. A single tree remains a valid patch input.
6. Store member tree indices in stable order. Compute the owner anchor as the
   arithmetic mean of member trunk XY positions and a deterministic Z value
   from their anchors.
7. Unit-test singleton, same-cell aggregation, exact cell boundaries, and
   deterministic member ordering.

## Phase 3: coarse control-blob construction

For each canopy patch:

1. Transform every member prototype's local branch-tip supports using that
   placement's yaw, scale, and anchor.
2. Retain the transformed tips as underside height constraints.
3. Add exactly one projected underside sample per unique branch tip. Apply only
   a tiny deterministic XY offset when two tips project to the same point at
   different heights so both remain triangulation constraints.
4. Triangulate the projected branch-tip samples with deterministic 2D Delaunay
   triangulation. Reject triangles whose circumradius or edge lengths bridge
   unsupported gaps; the remaining alpha-shape boundary is the concave canopy
   footprint.
5. If filtering yields multiple disconnected footprint components, emit
   multiple closed blobs within the same logical cluster mesh range. Do not join
   them with a long artificial neck.
6. Construct the visible volume from the footprint:
   - exact branch-tip centres remain interior underside support anchors while
     surrounding samples are relaxed in Z to remove spikes;
   - expand separate bottom, middle, and top visual boundary controls beyond
     those supports, then apply two deterministic Chaikin corner-cutting passes
     to each closed loop;
   - join the support cap to the rounded bottom and top outlines with annular
     strips, so no pinned branch tip can remain a visible silhouette corner;
   - the wider irregular middle outline creates crown volume and the contracted
     upper outline breaks up the silhouette;
   - crown peaks are derived from the member trees and major support groups;
   - shallow saddles between tree crowns keep individual crowns legible.
7. Produce consistently wound closed triangles, finite positions, valid u32
   indices, and smooth normals. No triangle may be degenerate within the
   project's mesh epsilon.
8. The resulting mesh is the coarse LOD1/LOD2 control blob.

## Phase 4: derive LOD0 from the control blob

1. Retain the separately rounded bottom, middle, and top visual perimeter-ring
   indices produced by each coarse blob component. Branch-tip support vertices
   are deliberately not classified as perimeter vertices.
2. Apply one conforming subdivision pass only to triangles incident to those
   perimeter vertices. Adjoining untouched cap triangles are stitched to the
   new shared-edge midpoints without subdividing the cap interior.
3. Preserve component boundaries and classify constrained vertices. Only the
   interior branch-tip underside anchors are pinned; the visible outline is
   free to relax.
4. Smooth only retained perimeter vertices and newly created perimeter-band
   vertices with bounded iterations. Interior coarse cap vertices remain fixed.
   Constrained support vertices must stay attached to their source positions,
   and smoothing must not shrink the footprint excessively.
5. Apply low-amplitude coherent displacement after smoothing:
   - strongest on upper and side surfaces;
   - reduced on the underside;
   - zero or near-zero at pinned branch-tip supports;
   - seeded by the island and cluster identity in a dedicated domain.
6. Recalculate smooth normals after displacement.
7. Verify that LOD0 and the control blob retain compatible overall bounds and
   silhouette so LOD transitions do not visibly jump.

## Phase 5: combined streams and owner grids

1. Continue appending wood per tree and retaining per-tree wood ranges.
2. Append foliage per cluster and introduce cluster foliage ranges containing:
   owner anchor, stable member indices, LOD0 range, and coarse range.
3. Foliage grid extraction iterates cluster ranges and assigns each complete
   cluster to the tile containing its owner anchor. It never clips or duplicates
   a blob at a tile boundary.
4. Wood grid extraction continues to iterate tree ranges.
5. `ForestMeshes::mesh` retains the current mapping: foliage LOD0 returns the
   detailed stream and foliage LOD1/LOD2 return the coarse stream.
6. Empty forests and clusters whose filtered footprint is empty must return
   valid empty meshes rather than panic.

## Phase 6: Unity and native boundary

The existing native functions and Unity streamer already request foliage by
visual LOD and consume combined tile meshes. No new per-cluster FFI surface is
required.

1. Keep `CreateForestFoliageMeshGrid` and its release ownership unchanged.
2. Keep Unity's forest tile hierarchy and material split unchanged.
3. Confirm LOD2 requests coarse foliage only, LOD1 requests coarse foliage plus
   simplified wood, and LOD0 requests detailed foliage plus full wood.
4. Rebuild and copy `libmotu.dylib`, compare source/destination checksums, and
   restart Unity before live testing because Unity caches native plugins.
5. Use `N` to toggle the tree-only edge overlay. Edge meshes are created lazily
   for active forest batches and remain children of the corresponding wood or
   foliage renderer so LOD visibility stays synchronized.

## Validation gates

### Rust-focused tests

- Tree generation is deterministic and yields branch-tip supports.
- The old cube topology and cube-count assertions are removed.
- Canopy ownership combines all trees in one fine streaming patch.
- Each unique projected branch tip contributes one control sample; no radial
  per-tip patch topology is retained.
- Coarse blobs are deterministic, finite, closed, consistently wound, and have
  valid indices and normals.
- Multiple alpha-shape components remain separate geometry inside one range.
- LOD0 has more triangles than coarse foliage but fewer than full-blob
  tessellation; the untouched cap interior retains its coarse topology.
- Pinned supports remain within tolerance after smoothing and displacement.
- Cluster ranges exactly cover their combined streams without overlap.
- Foliage owner grids copy whole clusters once; wood grids still copy whole
  trees once.
- LOD2 wood remains empty.

### Commands

Run, in order:

```sh
cargo fmt --all -- --check
cargo test --lib trees
cargo test --lib forest
cargo test --lib ffi
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

Then build the release native library, copy it to the Unity plugin location,
verify identical SHA-256 checksums, run Unity batch compilation, restart Unity,
and inspect at least one dense multi-tree cluster through all three visual LODs.

## Acceptance criteria

- No foliage cube generation remains in the active procedural-tree path.
- Each fine streaming patch has shared coarse foliage rather than per-tree
  shells, with true clearings retained by the alpha-shape filter.
- Each canopy patch has shared coarse foliage, with LOD0 derived by subdivision,
  smoothing, and coherent displacement.
- Individual crowns remain visible through peaks and saddles.
- Combined wood/foliage streams and Unity object counts preserve the current
  batching architecture.
- Automated Rust and Unity compilation gates pass; live visual quality is
  reported separately and is not claimed until observed after plugin reload.
