# True 3D Cliff Render-Mesh Plan

## Status

Phases 0 through 8 were implemented on 2026-08-11, but the render-only cliff
sharpening result continued to invert some steep patches. It was therefore
disabled later that day. LOD0 now renders the authoritative XY-safe support
surface directly. The separate render-mesh type, 3D clipping implementation,
native ABI fields, and saved option remain in place for compatibility, but
selective refinement, normal retreat, relaxation, and edge flipping are skipped
by passing zero strength at the terrain ownership boundary. Hydraulic and
coastal terrain shaping remain active. Hydraulic erosion now independently
protects the support mesh with cached stage-reference and live projected face
areas; every inward move is capped across its incident faces regardless of the
averaged vertex-normal angle.

The implementation is in `src/render_mesh.rs`, with ownership in
`src/terrain.rs`, native export changes in `src/ffi.rs` and `include/motu.h`, and
the matching Unity streaming/collider path in `IslandViewer.cs`, `MotuNative.cs`,
and `TerrainTileStreamer.cs`. `TRUE_3D_CLIFF_BASELINE.md` records the before and
after performance result.

## Objective

Generate convincing cliffs which can become vertical or locally overhanging,
while retaining the current free-form terrain generation, hydraulic and coastal
erosion, rivers, waterfalls, maps, streamed 8x8 LOD hierarchy, seam correction,
and first-person mode.

The result must be a triangle surface embedded in XYZ. It must not require every
render triangle to remain a single-valued height function over XY. At the same
time, systems that fundamentally need `z = f(x, y)` must continue to receive an
unambiguous support surface.

## Important terminology

### Support mesh

The authoritative, XY-safe terrain used for:

- hydraulic, thermal, coastal, and river simulation;
- downhill routing and sediment deposition;
- height, normal, sea-depth, and occlusion-map generation;
- decoration placement;
- LOD correction and coarse sampling;
- spawn and downward-ground queries; and
- a fallback collider.

Every support-mesh triangle must retain nonzero, consistently wound projected
XY area. A vertical line therefore intersects the support surface once.

### Render mesh

A derived triangle surface used for visual terrain geometry. Its vertices live
in XYZ and may overlap in XY. It may contain vertical faces, folded projections,
hard creases, and limited overhangs, but it must remain finite, consistently
wound in 3D, locally manifold, sliceable, and suitable for Unity's `Mesh` and
optionally `MeshCollider`.

### Surface remeshing

This plan does not call a 3D Delaunay tetrahedralizer. Three-dimensional
Delaunay triangulation fills a volume with tetrahedra and does not solve terrain
surface connectivity. The intended operation is local surface remeshing:
splitting and flipping triangle edges in 3D, moving only render vertices, and
duplicating vertices at intentional hard creases.

### Support anchor

Every render vertex retains the point from which it was derived on the support
mesh. An anchor contains enough data to reconstruct:

- the undisturbed support position;
- stable top-down texture coordinates;
- the displacement from support to render geometry; and
- a deterministic morph back to a coarse LOD boundary.

## Evidence from the current projects

The Rust `Mesh` already stores `Vec3` positions and calculates normals from 3D
cross products. The limitation is not Unity's triangle format or the vertex
storage type. It is the set of XY assumptions around the mesh:

- `Mesh::delaunay` creates connectivity from `Vec2` seed points.
- `Terrain::sample`, `sample_normal`, and `TriangleIndex` locate a single
  triangle beneath an XY point.
- height, normal, AO, sea-depth, raster, foliage, and decoration generation
  sample the support surface on an XY grid.
- river flow and most erosion decisions use Z as elevation and XY as the map
  plane.
- `correct_lods` relies on the original support-vertex prefixes and rewrites
  UVs from XY.
- the Rust grid slicer clips triangle projections in XY, then reconstructs Z
  with XY barycentric interpolation.
- grid-corner canonicalization currently selects one height for each XY corner.
- LOD boundary clamping keeps XY fixed and copies a coarse height and normal.
- Unity derives terrain UVs from horizontal world coordinates.

The Unity viewer itself already accepts arbitrary XYZ triangle arrays and uses
a `MeshCollider` for the current LOD0 tile. It does not require a Unity
`Terrain` height field.

The original C++ code provides two relevant precedents:

- `improveCliffs` moved selected coastline vertices horizontally, so the old
  visual effect was not purely Z displacement.
- `Mesh::slice` clipped `Triangle3WithNormals` against bounding planes and
  interpolated full 3D attributes. The current Rust slicer is more restrictive
  than the original in this respect.

## Chosen architecture

The island will own two related products after generation:

```text
XY seed triangulation
        |
        v
support LOD2 -> support LOD1 -> support LOD0
        |                         |
        |                         +-> maps, rivers, decorations, height queries
        |                         +-> fallback collider
        |
        +----------------------------> cliff-field derivation
                                      |
                                      v
                              3D LOD0 render mesh
                                      |
                                      v
                           3D tile clipping + LOD morph
                                      |
                                      v
                              Unity render/collider
```

The simulation mesh will not be replaced by the render mesh. Render remeshing
runs only after final river carving and `correct_lods`, so new connectivity can
never redirect water, create uphill river loops, disturb waterfall beds, or
invalidate support-vertex prefix alignment.

The initial release will generate true 3D cliff detail only for LOD0. LOD1 and
LOD2 remain support meshes and retain their baked detail-normal maps. A later
phase may add a simplified 3D LOD1 once LOD0 transition geometry is stable.

## Data model

### Keep `Mesh` as the exported triangle container

Do not make every mesh carry remeshing state. `Mesh` remains the simple public
container:

```rust
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub triangles: Vec<u32>,
    pub uv: Vec<Vec2>,
}
```

For render terrain, `uv` stores the support anchor's normalized XY coordinate,
not the displaced render vertex's XY coordinate. This keeps AO and other
top-down textures stable across an overhang.

### Add an internal `RenderMesh`

The proposed internal shape is:

```rust
struct RenderMesh {
    mesh: Mesh,
    support_positions: Vec<Vec3>,
    support_faces: Vec<u32>,
    support_weights: Vec<[f32; 3]>,
}
```

Requirements:

- all arrays have exactly one element per render vertex;
- original support vertices use an exact identity anchor;
- split vertices interpolate the anchors of their edge endpoints;
- crease-only duplicates copy the same anchor;
- render positions may move, but anchors never change during relaxation; and
- the structure is private to generation and slicing.

If `support_faces` and barycentric weights prove unnecessarily expensive, an
exact `support_positions` plus stable UV is sufficient for the first release.
Do not remove provenance until LOD boundary morphing works without resampling
ambiguity.

### Add a compact transient topology

Use a compact half-edge or equivalent edge-to-face representation while
remeshing:

```rust
struct SurfaceTopology {
    half_edges: Vec<HalfEdge>,
    face_edges: Vec<u32>,
    vertex_edges: Vec<u32>,
}
```

It is built once for a remeshing batch and discarded after flat triangle arrays
are rebuilt. Avoid `Vec<Vec<_>>`, per-face boxes, reference-counted nodes, and
allocations inside edge operations. Hash maps may be used during the first
correct implementation, but the target representation is sorted edge keys or
compact CSR buffers.

### Add a derived `CliffField`

The first implementation derives cliff eligibility from the final support
LOD0 rather than carrying mutable scalar fields through every earlier
tessellation stage:

```rust
struct CliffField {
    strength: Vec<f32>,
    retreat: Vec<f32>,
    protected: Vec<bool>,
}
```

Inputs include:

- final face and vertex normal angles;
- local height range divided by mean 3D edge length;
- normal variance and signed curvature;
- the configured hydraulic erosion strength;
- the existing shared geology/hardness field;
- elevation and distance from sea level;
- river, waterfall, and river-bank footprints; and
- island and requested tile-boundary safety regions.

This is intentionally a derived visual field. If later testing shows that
final geometry cannot distinguish hydraulic cliffs from unrelated steep
features, add an optional accumulated hydraulic-retreat buffer in a later
phase. Do not complicate every support-mesh topology operation pre-emptively.

## Cliff selection rules

A render cliff candidate must:

- be above the protected seabed threshold unless it belongs to an intentional
  coastal cliff;
- have a slope above the initial cliff angle, proposed as 55 degrees;
- have positive coherent strength across at least one neighbouring ring;
- not be an isolated one-vertex maximum;
- not be inside a river-bed or waterfall protection mask;
- not include the outer island perimeter; and
- have enough surrounding triangles to form a bounded patch.

Candidate strength should rise through the useful hydraulic range, peak before
the vertical limit, and taper smoothly at patch boundaries. Reuse the shared
terrain-noise geology so resistant coherent regions produce larger, connected
cliff patches rather than salt-and-pepper facets.

Do not select candidates solely from `normal.z`. That recreates the isolated
spike problem by allowing one anomalous vertex to drive topology.

## 3D remeshing algorithm

### 1. Build connected patches

- Mark candidate faces from the smoothed vertex field.
- Expand by one conforming ring so refined faces stitch into untouched terrain.
- Extract connected components by shared edges.
- Reject components below a configurable face count or physical area.
- Classify each component's upper lip, toe, interior, and outer boundary.

### 2. Selectively refine in 3D

- Split candidate edges whose 3D length exceeds the local target.
- Split edges with excessive dihedral angle even when their length is short.
- Share every midpoint across both incident faces.
- Conformingly triangulate neighbouring unsplit faces.
- Keep original support vertices and patch-boundary vertices fixed.
- Limit each component to a deterministic vertex and iteration budget.

The current midpoint tessellator can supply the first conforming split, but the
new topology must evaluate edge length, area, and angles in XYZ rather than
projected XY.

### 3. Form the cliff surface

- Move only newly appended render vertices during the first implementation.
- Apply the cliff retreat along the negative smoothed 3D normal.
- Reduce displacement at the patch boundary and at river-protected vertices.
- Allow the displaced surface to fold in XY; do not use projected-area
  inversion as a render-mesh validity test.
- Cap displacement by local 3D edge length and the patch's relief budget.
- Use multiple restrained iterations with recomputed 3D normals.

Keeping original support vertices fixed provides a watertight positional seam
between remeshed and untouched terrain and gives LOD morphing exact anchors.

### 4. Improve triangle quality

For interior candidate edges only:

- flip an edge when the replacement increases the smaller of the two 3D
  triangle angles;
- reject a flip that changes the patch boundary, produces duplicate faces,
  changes manifold edge incidence, or reverses 3D winding;
- tangentially relax newly inserted vertices;
- remove the normal component from smoothing so relief is not averaged away;
  and
- recompute the intended normal retreat separately after smoothing.

Do not implement edge collapse in the first release. Splits and flips are
sufficient to validate the architecture, while collapse complicates support
anchors, deterministic output, and boundary identity.

### 5. Create hard cliff lips without cracks

After positions stabilize:

- find edges whose face-normal dihedral exceeds the crease threshold;
- group incident faces into smoothing islands;
- duplicate render vertices by smoothing island while keeping positions and
  support anchors identical; and
- recalculate normals per smoothing island.

These duplicates are shading vertices, not disconnected geometry. Every side
of the visible seam remains colocated.

## Three-dimensional slicing

The current Rust `append_clipped_triangle` must not be used for render cliffs.
It clips `Vec2` projections and reconstructs Z from XY barycentrics, which is
undefined for vertical faces and ambiguous for folded projections.

Add an attribute-carrying clip vertex:

```rust
struct ClipVertex {
    position: Vec3,
    normal: Vec3,
    uv: Vec2,
    support_position: Vec3,
}
```

Clip each triangle against the four vertical tile planes using signed 3D plane
distance. When an edge crosses a plane, linearly interpolate every attribute
with the edge parameter. This is the Rust equivalent of the original C++
`Triangle3WithNormals::slice` behavior.

Requirements:

- vertical and overhanging triangles clip without division by projected area;
- one source triangle is processed once for a requested grid batch;
- the same source edge/plane intersection produces bit-identical attributes in
  sibling tiles;
- multiple vertices with the same XY and different Z are preserved;
- render-grid corner handling never collapses those vertices to one height;
- polygon fan triangulation preserves the source face's 3D winding; and
- normals are normalized after interpolation or recalculated after clipping.

Keep the existing support slicer unchanged until the new render slicer has
independent unit and integration coverage.

## LOD transition strategy

Same-LOD sibling seams are handled by clipping one global render mesh against
shared planes. Mixed-LOD seams require a morph back to the coarser support
surface.

For every side requested in `clamp_sides`:

1. Use the render vertex's support anchor or UV to obtain its support position.
2. Sample the coarser support LOD at the anchor XY.
3. Compute render detail as `render_position - support_position`.
4. Fade that detail to zero over a narrow normalized transition band.
5. At the exact boundary, use the coarse position and normal exactly.
6. Perform 3D clipping after the morph so the exported boundary is canonical.

This means overhangs can exist inside an active high-detail group but flatten
back into the coarse support surface at the side facing an active lower LOD.
Edges between two active LOD0 groups retain their full 3D detail.

The transition band must be based on local edge length and group extent, not a
fixed world-space magic number. Only sides present in `clamp_sides` are
morphed. This preserves the earlier fix that prevented all four tile edges from
being sewn down indiscriminately.

LOD1 and LOD2 remain support meshes initially. After the LOD0 implementation is
stable, a simplified LOD1 cliff mesh may be derived from the same cliff field
with lower refinement and retreat. LOD2 should remain a conventional support
surface unless testing shows objectionable distant silhouettes.

## Rust API and ownership changes

Proposed `Island` ownership:

```rust
pub struct Island {
    // Existing seed and options.
    terrain: Terrain,          // support LOD0 and XY sampling index
    coarser_lods: [Mesh; 2],   // support LOD1 and LOD2
    render_lod0: RenderMesh,   // derived after all support shaping
    // Existing rivers, river mesh, and decorations.
}
```

Rules:

- `Island::terrain()` and maps always use support LOD0.
- `Island::lod()` continues to return support meshes unless renamed explicitly.
- add a clearly named internal `render_lod()` or export-only method;
- do not clone the complete LOD0 in each tile request;
- the island owns global render buffers for its lifetime;
- a tile export owns only the clipped output returned through FFI; and
- all temporary topology and scratch arrays are allocated once per remeshing
  batch and reused across iterations.

Before implementation, settle names so `lod()` never silently changes meaning
for tests or internal algorithms.

## FFI changes

Update Rust, `include/motu.h`, and `MotuNative.cs` atomically.

### Terrain render export

Terrain render meshes need stable support UVs. Either extend `ExportMesh` with a
UV array or add a new `ExportTerrainMesh`. Because there are no older native
callers, extending `ExportMesh` is acceptable, but all Rust/C#/header layouts
must change in one phase and retain matching release ownership.

`CreateMesh` and `CreateMeshGrid` should export:

- LOD0 from the 3D render mesh;
- LOD1 and LOD2 from support meshes during the first release; and
- explicit UVs for every terrain vertex.

### Support collider export

Add an explicit support-mesh call rather than relying on `CreateMeshGrid` to
have two meanings. A narrow API is preferable:

```c
CreateSupportMesh(handle, area, lod, output)
```

Only the current LOD0 collider tile needs this export, so do not allocate
support collider grids for every visible tile.

### Ownership

Continue the existing boxed-handle pattern:

- Rust owns `Vec` buffers until the matching release call;
- C# copies them before release;
- every new export has exactly one release path; and
- FFI stress tests cover render meshes, support meshes, empty tiles, and
  repeated generation/release cycles.

## Unity changes

### Rendering

- Consume terrain UVs exported by Rust rather than regenerating them from the
  displaced render position.
- Keep Rust-to-Unity axis conversion and triangle winding reversal unchanged.
- Recalculate bounds after copying.
- Use geometric LOD0 normals directly.
- Keep LOD1/LOD2 detail-normal and AO maps tied to their support UVs.
- Keep LOD0 AO sampling tied to support UVs so an overhang does not smear the
  island-wide texture by displaced XY.

The existing procedural terrain colors already use world position and normal,
so vertical cliff coloring will work. A triplanar rock texture is optional and
must not block geometry delivery.

### Collider rollout

Use two stages:

1. Initially render the 3D mesh but create the current-tile `MeshCollider` from
   `CreateSupportMesh`. This isolates rendering and streaming failures.
2. Once manifold and Unity cooking tests are clean, use the 3D render tile as
   the collider. Retain automatic fallback to the support collider if Unity
   rejects the render mesh or the Rust validator marks it unsafe.

The current downward snap remains a top-surface query. Full cave navigation,
underside spawning, and arbitrary surface attachment are out of scope.

### Debug UI

Add temporary diagnostics:

- enable/disable 3D cliff render detail;
- color candidate, boundary, protected, and transition-band faces;
- display render/support vertex and triangle counts;
- display rejected remesh operations and unsafe-patch counts;
- switch between render and support collider; and
- retain the existing wireframe toggle.

Do not expose tuning sliders until the structural invariants pass. Later useful
controls are cliff detail strength, initial angle, overhang limit, and transition
width. Rust-side validation ranges are unnecessary; Unity may constrain its UI.

## Implementation phases

### Phase 0: Baseline and fixtures

Tasks:

- Record release-generation time, peak memory, LOD counts, tile-export time,
  and FFI stress-test time before the change.
- Capture fixed-seed screenshots at hydraulic strengths 1, 4, and 8 with
  wireframe enabled.
- Add metric helpers for 3D triangle area, projected area, edge incidence,
  dihedral angle, minimum angle, and vertical-line intersection count.
- Preserve the current spike-resistant hydraulic tests as support-mesh tests.

Acceptance:

- fixtures are deterministic;
- the baseline can be regenerated from documented commands; and
- no render-mesh code exists yet.

### Phase 1: Explicit support/render boundary

Tasks:

- Add `RenderMesh` and support anchors.
- Build an identity render mesh from final support LOD0.
- Split `lod()` and render-export naming so internal callers cannot select the
  wrong surface accidentally.
- Keep all FFI output bitwise equivalent while render detail is disabled.

Acceptance:

- height/maps/rivers/decorations are bitwise unchanged;
- identity render vertices and triangles equal support LOD0;
- every render vertex has a valid anchor and UV; and
- existing Rust and Unity tests pass.

### Phase 2: 3D clipper

Tasks:

- Implement `ClipVertex` and vertical-plane clipping.
- Add batched 8x8 slicing with shared deterministic intersections.
- Preserve coincident XY vertices at different heights.
- Add an identity path for support meshes only if it reduces duplication
  without changing their output.

Acceptance:

- horizontal, vertical, and overhanging synthetic triangles clip correctly;
- interpolated position, normal, UV, and anchor values are finite;
- union of tile triangle areas matches the unsliced source within tolerance;
- sibling boundary vertex multisets match exactly; and
- no projected barycentric division occurs in the render path.

### Phase 3: Cliff field

Tasks:

- Derive coherent candidate strength from final support geometry, geology,
  hydraulic strength, and protection masks.
- Smooth only the scalar field, not support positions.
- Extract connected patches and reject isolated components.
- Add diagnostic colors or a debug export.

Acceptance:

- a planar slope produces no isolated candidates;
- increasing hydraulic strength increases coherent candidate area rather than
  isolated vertex count;
- river beds, banks, and waterfall footprints are protected; and
- fixed seeds produce identical fields.

### Phase 4: Selective 3D refinement

Tasks:

- Build transient compact topology.
- Split long and high-dihedral candidate edges.
- Conformingly stitch the one-ring boundary.
- Maintain anchors and support UVs for every new vertex.
- Enforce deterministic operation ordering by stable edge keys.

Acceptance:

- untouched regions retain identical connectivity;
- every internal edge has exactly two incident faces;
- no duplicate or zero-area faces are introduced;
- all patch boundaries remain watertight; and
- triangle growth stays within the configured budget.

### Phase 5: Normal retreat and quality

Tasks:

- Displace new render vertices along negative 3D normals.
- Weight displacement by `sin(2θ)`, with its maximum at 45 degrees and zero at
  flat and vertical.
- Tangentially relax new vertices without flattening relief.
- Flip eligible interior edges when 3D minimum angle improves.
- Split normals across high-dihedral crease edges.
- Add bounded iteration and displacement budgets, plus projected-orientation
  backoff for proposed moves and edge flips.

Acceptance:

- selected fixtures strengthen steep faces without introducing new projected
  winding reversals;
- displacement rate is zero at 0 and 90 degrees and peaks at 45 degrees;
- original support vertices remain fixed;
- no vertex or normal is non-finite;
- no 3D triangle falls below the area threshold;
- no edge exceeds the maximum local-length ratio;
- no unbounded high spike survives the neighbour-relative diagnostic; and
- disabling cliff detail returns the identity render mesh exactly.

### Phase 6: LOD morphing and streamed seams

Tasks:

- Fade render detail only on sides requested by `clamp_sides`.
- Morph exact boundary anchors to the coarser support mesh.
- Perform 3D clipping after morphing.
- Preserve full detail on sides facing another active LOD0 group.
- Add seam diagnostics for all side and corner combinations.

Acceptance:

- same-LOD sibling edges match exactly;
- mixed-LOD boundary positions differ by no more than `1e-6` normalized units;
- only requested sides lose detail;
- corners shared by two clamped sides have one canonical coarse anchor;
- no wall or hole appears when crossing any streamed group boundary; and
- repeated tile creation produces identical buffers.

### Phase 7: FFI and Unity rendering

Tasks:

- Export terrain UVs.
- Route LOD0 `CreateMesh` and `CreateMeshGrid` through the render mesh.
- Add `CreateSupportMesh` and matching release coverage.
- Update C# layouts and copies.
- Add debug overlays and support-collider mode.

Acceptance:

- Rust/C/C# struct sizes and field offsets match;
- Unity imports every tile without exceptions;
- wireframe shows local 3D refinement only in cliff patches;
- AO remains applied to LOD0 via support UVs;
- normal orientation is correct after axis conversion; and
- entering, moving, and leaving first-person mode works with support collider.

### Phase 8: Render collider

Tasks:

- Validate each current LOD0 render tile before collider assignment.
- Use the render mesh for `MeshCollider` when safe.
- Fall back to the support collider on validation or cooking failure.
- Test cliff tops, faces, toes, tile edges, and LOD transitions.

Acceptance:

- Unity logs no mesh-cooking errors for the fixed-seed matrix;
- the player cannot fall through cliff patches or streamed seams;
- downward snap lands on the uppermost visible surface;
- support fallback remains functional; and
- collider switching does not retain native or Unity mesh objects.

### Phase 9: Simplified LOD1 cliffs, only if needed

Tasks:

- Measure LOD0 appearance popping against support LOD1.
- If necessary, resample the LOD0 cliff field onto LOD1.
- Apply lower refinement and retreat with the same support-anchor model.
- Disable the tangent-space detail normal on near-vertical LOD1 faces or use a
  geometry-aware shader path.

Acceptance:

- distant silhouette error is measurably reduced;
- LOD1 triangle cost remains within budget; and
- LOD1-to-LOD2 seams remain support-anchored.

## Validation matrix

### Rust unit tests

- 3D plane clipping for every box side.
- Attribute interpolation on vertical edges.
- Coincident XY/different-Z preservation.
- Compact topology edge incidence.
- Conforming edge split.
- Edge-flip acceptance and rejection cases.
- Tangential smoothing preserves prescribed normal relief.
- Crease duplication preserves positions and anchors.
- Support-anchor interpolation.
- Cliff patch connectivity and minimum-size rejection.
- River/waterfall protection masks.
- Render-detail disable identity.

### Rust integration tests

For multiple fixed seeds and hydraulic strengths 0, 1, 4, and 8:

- support terrain remains deterministic;
- maps and rivers remain finite;
- render mesh is deterministic and valid;
- nonzero strength strengthens steep render faces without folding valid support
  faces through vertical;
- strength 0 produces no hydraulic cliff detail unless another explicit cliff
  source is enabled;
- render vertex growth is bounded;
- all 8x8 tile batches are complete;
- all sibling and mixed-LOD seams meet tolerance; and
- FFI create/release loops remain balanced.

### Unity tests

- Batch-mode script compilation.
- Native plugin load and generation.
- LOD0/1/2 streaming around corners and island edges.
- Wireframe and cliff-field debug views.
- AO on support-anchored LOD0 UVs.
- First-person entry from overview.
- Support and render collider modes.
- Repeated regeneration without leaked meshes, textures, or handles.

### Manual visual review

Use the same seeds before and after implementation. Review:

- cliff silhouette at grazing angles;
- absence of isolated needles and star-shaped folds;
- coherent rock faces rather than uniformly crumpled slopes;
- hard but watertight cliff lips;
- natural toes meeting terrain and water;
- river and waterfall continuity through steep regions;
- tile seams while walking; and
- LOD transitions while approaching and leaving a cliff.

## Performance and allocation budgets

Performance is a release criterion, not a later cleanup.

- No allocation occurs inside an individual split, flip, relaxation, clipping,
  or triangle-validation operation.
- Reserve render vertices and triangles from candidate-face estimates.
- Use compact arrays and stable integer indices.
- Build topology once per remeshing batch and update it in place where practical.
- Process a complete requested tile grid in one source-mesh pass.
- Do not clone the complete global render mesh per tile.
- Keep LOD0 render triangle growth below 2.5x support LOD0 by default.
- Keep default release island-generation time within 15% of the recorded
  baseline; any larger cost requires a measured visual justification.
- Keep an 8x8 tile export within 15% of the support slicer's recorded baseline.
- Record peak resident memory and reject unbounded per-patch growth.

If the half-edge implementation exceeds the budget, first replace maps with
sorted edge arrays and reuse scratch buffers. Do not weaken manifold, seam, or
finite-value checks merely to improve a benchmark.

## Failure handling and rollback

The render stage is derived and may fail without invalidating the island.

- Validate the completed global render mesh before storing it.
- If a patch fails, discard that patch and retain its identity support geometry.
- If the global render mesh fails, export support LOD0 and report diagnostics.
- If a sliced render tile fails, export the corresponding support tile.
- If Unity rejects a render collider, use the support collider.
- Keep a generation/debug toggle that bypasses all 3D cliff detail during
  rollout and testing.

Do not mutate or roll back the authoritative support mesh in response to a
render-only failure.

## Explicit non-goals

- Volumetric tetrahedral terrain.
- Boolean cave excavation.
- Arbitrary tunnels or enclosed cave systems.
- Replacing rivers, erosion, coastlines, maps, or decoration placement with 3D
  volumetric simulation.
- Navigating on ceilings or spawning beneath overhangs.
- Runtime erosion inside Unity.
- Edge collapse in the first remeshing release.
- A new third-party geometry dependency before the in-house operations are
  proven insufficient.

## Recommended implementation order

Implement in this strict order:

1. Baseline metrics and fixtures.
2. Identity `RenderMesh` with anchors.
3. True 3D slicer and UV export.
4. LOD boundary morphing while render detail is still zero.
5. Cliff field and diagnostics.
6. Selective 3D splits.
7. Normal retreat and tangential relaxation.
8. Edge flips and crease normals.
9. Unity support-collider rendering trial.
10. Render collider trial.
11. Optional simplified LOD1 cliffs.

The slicer remains capable of handling vertical and overhanging input, but the
generator no longer deliberately displaces faces through vertical. This keeps
slicer and seam robustness without using folded projected faces as a cliff
generation technique.

## Definition of done

The change is complete only when all of the following are true:

1. The support terrain remains a deterministic XY-safe surface and all existing
   simulations and derived maps continue to use it.
2. LOD0 renders the deterministic support surface without a second cliff
   displacement or refinement pass.
3. Render geometry remains finite, manifold within its intended boundaries,
   bounded in density, and free of isolated high spikes.
4. The render slicer handles vertical and folded triangles directly in 3D.
5. Same-LOD and mixed-LOD streamed seams meet the positional tolerance, and
   only sides facing a lower LOD are morphed.
6. Rust exports support-anchored terrain UVs and Unity applies existing AO
   without regression.
7. First-person mode works with the support collider and, after validation,
   the true 3D render collider with automatic fallback.
8. Rivers, waterfall lips, beds, banks, coastal geometry, LOD correction, and
   maps pass their existing tests and visual fixtures.
9. Rust formatting, strict Clippy, all tests, FFI stress, Unity batch compile,
   and the fixed-seed visual matrix pass.
10. Release performance and memory remain inside the documented budgets.
