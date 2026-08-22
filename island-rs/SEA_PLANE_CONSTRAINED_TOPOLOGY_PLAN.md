# Sea-plane constrained topology plan

## Objective

Make the final LOD 0 coastline an explicit mesh feature before sea depth and
land-distance fields are generated:

1. Insert one shared vertex at every mesh edge that crosses `z = 0`.
2. Retriangulate each crossing face so consecutive intersection vertices are
   connected by constrained coastline edges.
3. Validate those edges as one or more closed loops, with open paths allowed
   only where a contour terminates on the outer mesh perimeter.
4. Tessellate only triangles incident to the coastline.
5. Smooth the resulting patch in XY while keeping the coastline at `z = 0`,
   preserving loop edges, and preventing projected triangle inversion.

The pass must be the final LOD 0 topology mutation. Sea depth and accumulated
edge distance from land continue to be calculated afterward and barycentrically
sampled from the resulting final triangles.

## Pipeline placement

Run the pass inside final river geometry generation after all channel and
waterfall carving, relaxation, sea-plane clearance, and final waterfall support
work, but before:

- failed-waterfall validation;
- `finalize_river_geometry` and river mesh duplication/clipping;
- river-rock generation;
- `correct_lods` and construction of the final `TriangleIndex`;
- material export, decorations, and sea-mask distance generation.

Concretely, add a final topology stage between
`RiverGeometryBuilder::finish_waterfalls` and
`RiverGeometryBuilder::assemble`. Pass mutable terrain, `SurfaceMaterial`,
`RiverMeshBuffers`, and `WaterfallTerrainConstraints` through it.

This placement ensures that river mouths and the exported river mesh see the
same explicit sea-plane boundary. Running it later in `Island::generate` would
leave the already-duplicated river mesh and river-bed masks on the old topology.

## Phase 1: reusable arbitrary edge splitting

Add a mesh operation dedicated to splitting selected edges at an arbitrary
interpolation parameter rather than reusing midpoint tessellation.

Introduce an attributed split record such as:

```rust
struct EdgeSplitStencil {
    vertex: u32,
    edge: [u32; 2],
    interpolation: f32,
}
```

The existing `NewVertexStencil` is not sufficient for the initial coastline
insertion: it represents midpoint refinement and neighbourhood averaging,
whereas the sea-plane intersection must use the exact edge interpolation.

For an edge `(a, b)` with heights on opposite sides of zero, compute:

```text
t = za / (za - zb)
position = lerp(a, b, t)
position.z = 0
```

Requirements:

- Canonicalize the edge key as `(min(a, b), max(a, b))`.
- Insert each crossing once in a shared edge map.
- Reuse an endpoint already on the plane instead of creating a duplicate.
- Use one small height epsilon consistently for classification.
- Reject non-finite inputs and zero-length projected edges.
- Interpolate UVs by the same `t`; recalculate normals after retriangulation.
- Return split stencils and the set of constrained coastline edges.

## Phase 2: retriangulate crossing faces

Process every original triangle once using its three classified vertices and
the shared edge-intersection map.

Handle these cases explicitly:

- No crossing: preserve the triangle unchanged.
- Two strict edge crossings: replace the face with one triangle on the
  one-vertex side and two triangles on the two-vertex side. The edge between
  the two inserted vertices is the coastline constraint.
- One existing on-plane vertex plus one opposite-edge crossing: connect the
  existing vertex to the inserted vertex and triangulate both sides.
- Two existing on-plane vertices: retain their existing edge as constrained.
- Entire triangle on the plane: preserve it but do not invent an arbitrary
  coastline direction; report it for validation because a sea-level plateau is
  ambiguous.

Preserve original winding and require every output triangle to have positive
projected area above the existing mesh epsilon. Do not use a generic fan that
can choose different diagonals on adjacent faces.

## Phase 3: form and validate coastline loops

Collect every constrained segment emitted by the crossing-face pass, canonicalize
it, sort it, and remove duplicates. Build constraint-only adjacency.

Expected invariants for an island whose perimeter is underwater:

- every coastline vertex has exactly two constrained neighbours;
- every constrained edge has exactly one land-side and one sea-side face;
- no constrained edge is duplicated;
- no closed loop touches the outer mesh perimeter;
- walking unused constrained edges closes each component without revisiting a
  vertex early.

Return ordered coastline paths, because offshore islands or valid inland
contours may produce multiple closed loops, while synthetic or deliberately
cropped terrain can meet the outer boundary. Open paths are valid only when
both endpoints are mesh-perimeter vertices. Fail generation with a diagnostic
for an interior open chain, branch, non-manifold edge, or ambiguous sea-level
plateau. Do not silently repair topology.

## Phase 4: exact attribute transfer

Extend all vertex-parallel state for every arbitrary edge split before doing
midpoint tessellation:

- `SurfaceMaterial`: interpolate deposited depth, hardness, and cached sea
  proximity by `t`, then perform the existing loose-volume rescale once after
  the complete topology operation.
- `RiverMeshBuffers`: interpolate surfaces, river UV, target widths, and target
  depths by `t`. Preserve a river selection only when both edge endpoints are
  selected; use the existing deterministic owner-selection rules.
- `WaterfallTerrainConstraints`: conservatively inherit boolean protection
  (`patch`, `pinned`, `support`, and `water_unclamped`) and interpolate the
  terrain ceiling. Any split touching a protected waterfall feature remains
  protected from XY smoothing.
- Mesh UV: interpolate by `t` for the initial split and by midpoint for the
  later local tessellation.

Keep a single ordered list of new-vertex records so every attribute vector is
extended in exactly mesh vertex order. Add length assertions after each stage.

## Phase 5: tessellate only incident triangles

After the explicit loops exist, mark every coastline vertex and invoke one
selective refinement pass equivalent to `Mesh::tessellate_incident_to`.

The conforming stitch behavior may split an edge of a neighbouring unselected
face, but no unrelated face should be subdivided. Extend material, river, and
waterfall attributes using the existing midpoint stencil path.

When a constrained coastline edge is split at its midpoint:

- the midpoint must remain exactly at `z = 0`;
- replace the original constraint with its two child constraints;
- splice the midpoint into the corresponding loop order;
- protect both child edges from all later edge flipping.

Record the pre/post face counts and assert that every newly refined face was
incident to the coastline or was required only as a conforming stitch.

## Phase 6: constrained XY smoothing

Perform exactly one Jacobi-style smoothing pass over the ordered coastline
loops. Read every source position from one immutable snapshot and write results
to a separate position buffer, so iteration order cannot affect the result.

Constraints:

- Move XY only; preserve every vertex's Z.
- Keep every coastline vertex exactly at `z = 0`.
- For each coastline vertex, use exactly the vertex and its two neighbours in
  the same ordered loop:

  ```text
  smoothed_xy = (previous.xy + current.xy + next.xy) / 3
  ```

- Do not move non-coast incident vertices during this smoothing pass.
- Pin mesh-perimeter vertices, waterfall-protected vertices, and any explicitly
  protected river feature that cannot safely move laterally.
- Before accepting a move, test every incident triangle for preserved XY
  orientation and minimum projected area. Keep the original position if the
  full averaged move is unsafe; do not apply a partial relaxation.
- Do not change connectivity or run an unconstrained edge-flip pass afterward.

## Phase 7: downstream rebuild

After smoothing:

1. Recalculate LOD 0 normals and UV positions.
2. Rebuild river boundary flags and waterfall support masks if their XY tests
   depend on moved vertices.
3. Run failed-waterfall detection.
4. Derive the river mesh, river-bed mask, and river-rock mesh from the new
   topology.
5. Run `correct_lods`; rebuild the LOD 0 `TriangleIndex` from the constrained
   mesh and pin refined LOD 1/2 vertices to it.
6. Generate final materials and decorations.
7. Compute accumulated LOD 0 edge distance from land as the last generation
   field, then bake both sea-mask channels with barycentric interpolation.

## Tests

### Mesh unit tests

- A single crossing triangle inserts two exact `z = 0` vertices and a
  constrained segment with preserved winding.
- Two triangles sharing a crossing edge reuse one intersection vertex.
- Existing on-plane vertices are reused without duplicates.
- A quad cut by the plane produces one continuous constraint through both
  faces.
- Multiple closed contours are extracted as separate loops.
- Open, branched, non-manifold, and plateau cases fail deterministically.
- Attribute interpolation uses the exact edge parameter rather than `0.5`.

### Refinement and smoothing tests

- Only coastline-incident faces and required conforming neighbours refine.
- Tessellated constraint edges remain a continuous loop at `z = 0`.
- An unconstrained loop vertex moves exactly to the XY average of itself and
  its two ordered loop neighbours after one pass.
- XY smoothing never changes Z, flips projected orientation, collapses a face,
  moves a pinned vertex, or breaks loop degree two.
- Protected coastline edges survive any explicitly invoked local optimization.
- Repeated runs are deterministic.

### Pipeline regressions

- River mouths and the clipped river mesh terminate on the explicit coastline.
- Waterfall constraints and river attribute arrays match final vertex count.
- LOD 1/2 pinning succeeds after LOD 0 gains coastline-only vertices.
- Sea depth and land distance share the explicit final boundary on representative
  external beaches, inlets, and river mouths.
- Existing river, waterfall, material-volume, collider, tile-seam, and native
  export tests remain green.

## Validation sequence

1. Run focused mesh crossing/loop/smoothing tests.
2. Run focused river-mouth, waterfall, material-transfer, and sea-mask tests.
3. Run `cargo fmt --all`.
4. Run `cargo clippy --all-targets -- -D warnings`.
5. Run `cargo test --lib`; leave the slow ignored FFI lifecycle test ignored
   unless the export API changes.
6. Build release, deploy `libmotu.dylib`, compare checksums, and verify signing.
7. Run Unity native/shader batch validation.
8. Restart Unity and visually inspect at least one open beach, inlet, river
   mouth, and small offshore loop with the incoming and reverse wave trains
   moving.

## Acceptance criteria

- Every strict LOD 0 edge crossing of `z = 0` has exactly one shared sea-plane
  vertex.
- Crossing faces contain an explicit constrained edge along the sea plane.
- Constraint components are valid closed loops or perimeter-terminated paths.
- Only the coastline patch receives additional tessellation.
- Constrained smoothing changes XY only and preserves valid height-field
  projection.
- No later stage mutates LOD 0 topology before sea fields are calculated.
- Sea depth, land distance, river mouths, and rendered shoreline all follow the
  same final constrained triangles.
