# Procedural tree and forest mesh plan

## Goal

Build forests as two generated meshes:

1. one combined woody mesh containing every trunk and branch;
2. one combined foliage mesh containing every leaf cluster.

The first milestone is deliberately smaller: generate one seeded procedural
tree containing only its trunk and branches, export that wood mesh to Unity,
and display it in a dedicated `TreeSandbox` scene. Do not place forests on the
island or generate foliage yet.

## Existing project seams

The implementation should extend the existing architecture rather than make a
second procedural system:

- Rust owns deterministic generation through the crate's `Rng` and `Mesh`
  types.
- `terrain::Decorations` already supplies accepted tree positions for the
  eventual forest-placement phase.
- The river-rock generator already demonstrates how many procedural objects
  can be appended into one native mesh.
- `ExportMesh`/`ReleaseMesh` and Unity's native mesh-copy path already define
  the ownership and axis-conversion rules.
- Editor setup scripts already create reproducible scenes and materials.

## Geometry contract

Add a focused Rust module such as `src/trees.rs` with these conceptual types:

```rust
pub struct TreeOptions {
    pub ring_vertices: u8,
    pub maximum_child_branches: u8,
    // Physical ranges for section length and bend.
}

pub struct TreeMeshes {
    pub wood: Mesh,
    pub foliage: Mesh,
}
```

`generate_tree(seed, options)` returns owned meshes because the generator
creates them from scratch and both outputs must cross the FFI boundary. The
generator itself owns one RNG, one queue of growing axes, and the two output
buffers. Helpers should borrow those buffers rather than clone intermediate
rings or meshes.

For milestone one, `foliage` is an empty `Mesh`, but it is present in the Rust
result contract so forest integration does not later require redesigning the
generator boundary.

Use normalized island coordinates internally, converting all physical tree
dimensions from metres with `ISLAND_WORLD_METRES`. Centre the standalone tree
at `(0.5, 0.5, 0.0)` so the existing Rust-Z-up to Unity-Y-up conversion produces
a metre-scaled tree at the Unity origin when copied with the normal 2,000 metre
world size.

## Branch growth model

Represent the trunk and every child branch as a `GrowingAxis`:

```text
current ring centre
current direction and transported ring frame
radius
next section length
remaining section budget
depth
```

The trunk is the root axis and is not counted as one of the eight child
branches. `maximum_child_branches = 8` means eight offshoots in addition to the
trunk. Keeping this as a named option makes changing the interpretation to
eight total woody axes trivial if desired later.

Process axes one section at a time through a deterministic queue. Round-robin
processing allows new branches to grow and potentially create descendants
before the trunk consumes the entire branch budget.

For each section:

1. Start from the axis's current ring.
2. Let a child branch's first segment follow the construction-time normal of
   its selected parent rim vertex exactly. On later child sections, apply a
   seeded bend and strongly constrain the result toward world Z, modelling
   growth toward the light. Apply the normal seeded bend to trunk sections.
3. Advance by the current section length.
4. Create the next ring perpendicular to the new direction.
5. Connect corresponding ring vertices with consistently wound quads split
   into triangles.
6. Keep the topology-generation width and nominal section length constant.
   Apply a seeded local twist of at most 45 degrees left or right to each newly
   extruded ring. Record the ring centres, indices, and inherited taper scales
   for a final deformation pass.
7. Keep the first two metres of the main trunk free of branches so a simple
   capsule collider can cover its base cleanly. Once that clearance is reached,
   or when growing any child axis, if fewer than eight child branches have been
   created, sample a branch-spawn
   probability that starts low on the axis's first section and rises on every
   later section. On success, first reject every side whose outward normal has
   a negative world-Z component. For children of the main trunk, select one
   random remaining face. For children of another branch, also reject the face
   with the strongest upward normal so growth is limited to the lateral
   left/right faces. In both cases, reject the face used by that axis's previous
   direct child. If no face satisfies the constraints, leave the section
   unbranched. Each descendant axis tracks its own previous face independently.
   Preserve enough late trunk opportunities to guarantee the global
   eight-branch target.
8. Requeue the parent axis if it remains above its minimum radius and length
   and has not exhausted its section budget.

Each growing axis tracks only the number of children it spawned directly. Its
actual segment advance uses full length with no children, then the softer
`(n + 1) / (n + 2)` multiplier for `n` direct children: one uses two thirds,
two use three quarters, three use four fifths, and so on. A newly spawned child
starts at full rate with a direct-child count of zero. Grandchildren update
their immediate parent only and never slow their grandparent.

Once eight child branches exist, stop spawning new axes but allow the trunk and
all eight existing branches to finish their remaining sections. This avoids
open or abruptly truncated tubes.

## Rings, frames, and junctions

Use exactly four rim vertices in a fixed-size array, producing a rectangular
prism for every axis without a per-ring heap allocation.

Construct the first trunk frame from world X/Y. For subsequent rings, transport
the previous frame onto the new direction, rotate it by a seeded angle between
-45 and +45 degrees around that direction, and re-orthonormalize it. Root and
branch-opening rings remain aligned with their caps or inserted parent faces;
the twist begins on their first extrusion.

The selected child direction starts as the outward radial direction represented
by the chosen rim vertex. This is the vertex's construction-time normal before
the completed wood mesh receives its final area-weighted normals. Use that
normal unchanged for the first branch section; introduce the upward light bend
only on subsequent sections.

When spawning a child, select one rectangular side face of the current parent
section and insert four new vertices forming a smaller square within it.
Replace that face's two triangles with four triangulated strips around the
square opening. Use the opening's four vertices directly as the child's first
ring and extrude subsequent branch sections from them. Do not add a cap,
overlapping tube, or hidden junction geometry: the parent and child share the
opening edges as one manifold mesh.

After all axes, openings, and triangles are complete, apply taper as one final
geometry pass. The trunk starts at scale 1.0 and each axis uses an eased curve
to retain its body before reaching 18% of its inherited root scale at the tip.
A child starts with a short collar: its opening is 78% of the parent radius,
then narrows to 54-66% (depending on depth) before regular growth begins. This
keeps the join broad without making every offshoot as thick as its parent. The
trunk root receives a 32% flare with restrained deterministic buttress
asymmetry. Deform each recorded ring once around its stored centre, then
calculate wood normals.

At the terminal centre of every axis, including the main trunk, append one
oriented cube to the separate foliage mesh. Centre the cube's bottom face on
the endpoint and align its height with the terminal direction. Use a separate
seeded foliage random stream to choose each side length between 2.8 and 5.6
metres without perturbing wood topology. Accumulate all cubes into one foliage
mesh so each adds only eight vertices and twelve triangles.

Treat the completed tapered wood and cube foliage meshes as LOD1 topology and
record an explicit LOD1-to-LOD0 vertex index map for each. Build LOD0 by
tessellating every triangle into four with shared edge midpoints. Keep every
original rectangular-cage vertex fixed. Project only the new wood vertices
radially onto the radius interpolated along their source tube segment. When a
source edge bridges the parent surface and child opening, make the child branch
ring authoritative and project directly onto its root tube. Parent-only and
child-only edges retain their corresponding tube projections, avoiding a
pinched average at the branch base. Record the plane through the barycentre of
every open wood end ring before projection, then project the ring and its
tessellated boundary-edge midpoints back onto that plane. Preserve this shaped
mesh as LOD1, then tessellate the LOD0 wood a second time and apply one free
Laplacian smoothing pass. The smoother preserves the open trunk and branch-tip
perimeters while relaxing every interior wood vertex into a rounder joined
surface. Apply a small deterministic normal displacement to non-perimeter LOD0
vertices so the silhouette is not mechanically perfect. Foliage retains its
own clustered subdivision and support-pinning pass. Finally retain the explicit
topological LOD1-to-LOD0 correspondence and recalculate normals on all four
meshes.

Leave the trunk base and every terminal axis ring open: those ends are hidden
by the ground or foliage, and omitting their caps keeps the wood mesh cheaper.
Recalculate area-weighted normals once after the complete tree is built, not
after every section. Use the wood UV stream for an octahedrally encoded local
branch axis. Preserve it through tessellation and smoothing, rotate it during
forest placement, and decode it in Unity so warped bark grooves, cracks, and
normal detail run along each branch rather than through the tree as isotropic
blobs. Keep connector ownership through both LOD refinements. After geometry
smoothing, displacement, and normal calculation are complete, duplicate only
the coincident vertices used by each parent-to-collar connector patch and give
those duplicates the parent axis. The child tube keeps its own axis, preventing
direction interpolation from stretching bark across the join without changing
the watertight silhouette or smooth normals. Foliage keeps its existing UV
semantics. The Unity wood shader uses that decoded axis to project the baked
`Bark.json` one-metre tile in two branch-local planes, blending around the
rounded cross-section. Each streamed wood vertex also carries its owning tree
root through the existing material sidecar. Projection is evaluated relative
to that root rather than the absolute island coordinate, preventing small
changes in a bent axis from multiplying a hundreds-of-metres world offset into
compressed or diagonal bark. Its authored albedo, height/parallax, normal and
occlusion maps replace the former synthetic crack pattern while broad 3D noise
remains only as low-amplitude per-tree colour variation.

## Initial parameter ranges

Keep all values in a `TreeOptions::default()` rather than scattering constants.
The first visual pass can start around:

- 4 vertices per ring;
- 8 child branches;
- trunk diameter: 1.1-1.6 metres;
- first trunk section: 0.8-1.2 metres;
- trunk section budget: 8-12;
- child radius: exactly the parent's radius until size hierarchy is introduced;
- child nominal section length: exactly the parent's nominal section length;
- topology-generation radius multiplier: 1.0;
- per-section nominal length multiplier: 1.0 until taper is introduced later;
- final tip radius: 18% of each axis's inherited root radius;
- child-branch probability: 5% on the first section, increasing linearly to
  100% on the final section;
- post-emergence child phototropism: 32% of the remaining bend toward world Z;
- direct-child growth multiplier: `(n + 1) / (n + 2)` for `n > 0`;
- minimum radius: approximately 2-4 centimetres;
- maximum child depth: enough to reach the global eight-branch cap, with a
  hard safety limit even though the global count already bounds growth.

These are tuning defaults, not serialized ABI fields in the first milestone.
The seed alone should be enough for the preview API.

## Rust implementation phases

### 1. Tree module and deterministic generator

- Add `trees.rs` to the crate module graph.
- Define `TreeOptions`, `TreeMeshes`, internal `GrowingAxis`, and generation
  statistics used by tests.
- Add small helpers for creating a ring, connecting adjacent rings, choosing a
  child direction, capping an axis, and appending indices with checked offsets.
- Use the crate's portable `Rng`; assign a dedicated tree seed salt so later
  forest generation does not perturb unrelated random streams.
- Reject non-finite or non-positive parameters before allocating large output
  buffers.
- Calculate wood normals after generation. Keep foliage empty.

### 2. Native preview export

Add an island-independent entry point so the preview scene does not generate a
full terrain merely to obtain one tree:

```text
CreateProceduralTree(seed, wood_output, foliage_output)
```

Both outputs should use the existing `ExportMesh` layout and be released with
the existing `ReleaseMesh`. Export the empty foliage result safely, but the
first Unity preview only needs to instantiate the non-empty wood output.

Make failure atomic: initialize both outputs to defaults, validate pointers,
generate the owned `TreeMeshes`, and only publish handles after both exports
are ready. Document that each non-null handle has independent ownership.

This changes the native export API. Therefore the normally ignored
`ffi_allocations_have_matching_release_functions` test must be run explicitly
for this milestone, in addition to ordinary tests.

### 3. Unity native binding and mesh copy

- Add the matching declaration to `MotuNative.cs`.
- Reuse `ExportMesh` and `ReleaseMesh`; do not introduce a second unmanaged
  mesh representation.
- Extract or expose the existing axis/scale conversion as a shared helper that
  can copy a generated tree mesh with `worldSize = 2000`.
- Always release both native outputs in `finally`, even when managed mesh
  validation or creation fails.
- Validate finite vertices, normals matching vertex count, triangle indices in
  range, and triangle count divisible by three before creating Unity meshes.

## TreeSandbox scene

Create these focused Unity assets:

- `Assets/Scenes/TreeSandbox.unity`
- `Assets/Scripts/ProceduralTreePreview.cs`
- `Assets/Editor/TreeProjectSetup.cs`
- a simple wood material, preferably backed by a small project-owned lit or
  triplanar wood shader rather than an external package.

`ProceduralTreePreview` should:

- expose a serialized integer seed;
- generate exactly one tree on enable/play;
- create one `Wood` child with `MeshFilter` and `MeshRenderer`;
- retain a reserved foliage result internally but create no foliage renderer
  in milestone one;
- support a context-menu/editor button to choose a new seed and regenerate;
- retain both generated mesh levels and toggle the displayed wood and foliage
  together between LOD0 and LOD1 when the user presses `L`;
- destroy replaced runtime meshes and materials correctly;
- never instantiate `IslandGenerator` or generate terrain.

The scene should contain only the preview root, a camera, a directional light,
and an optional neutral ground/reference plane. Frame the full generated mesh
bounds automatically so unusually tall or broad seeds remain visible.

`TreeProjectSetup` should reproducibly create/update the material and scene. It
must preserve `IslandSandbox` and existing build settings rather than replacing
them. The new scene may be added after the main sandbox or left as an editor
preview scene if no runtime build entry is needed yet.

## Tests for milestone one

### Rust unit tests

- The same seed and options produce byte-identical vertices and triangles.
- Different representative seeds produce different valid trees.
- Exactly eight child branches are recorded; the trunk is tracked separately.
- Every section uses four cross-section vertices and retains its axis width and
  nominal section length.
- Branch probability rises monotonically from the lowest to highest section.
- Every first child-branch segment follows its source rim vertex normal, and
  every subsequent segment bends closer to world Z than its predecessor.
- Segment advance follows the direct-child divisor without counting descendants.
- Every child opening is square, shares its four boundary vertices with the
  branch, and leaves the complete wood mesh manifold.
- Every vertex and normal is finite.
- Every triangle index is in range and every generated side triangle has
  non-zero area.
- Trunk base and completed branch tips are capped.
- Every independently capped wood tube remains closed.
- Wood is non-empty and foliage is empty in milestone one.
- Generation terminates under adversarial but valid option limits.

### FFI tests

- Null output pointers do not allocate or crash.
- A valid call produces a releasable, non-empty wood mesh and a valid empty
  foliage result.
- Releasing either output resets it to default and does not affect the other.
- Run the explicitly ignored full FFI allocation lifecycle test because the
  export API changed.

### Unity validation

- The setup command creates and reopens `TreeSandbox` without altering
  `IslandSandbox`.
- The scene contains exactly one preview component and one wood renderer.
- Changing the seed regenerates the mesh and disposes the previous mesh.
- The generated tree is upright, correctly scaled in metres, front-face wound,
  lit, and fully framed by the camera.
- No foliage object or foliage geometry is visible in milestone one.
- Validate editor compilation and the scene in a clean temporary Unity project
  copy, then rebuild/deploy `libmotu.dylib` and restart Unity before visual QA.

## Deferred forest phase

The detailed implementation contract for this phase now lives in
`FOREST_PLACEMENT_AND_LOD_PLAN.md`. That document is authoritative for forest
placement, combined wood/foliage batching, and terrain LOD behavior; this
section remains as historical context for the standalone-tree milestone.

After the single-tree preview is approved:

1. Generate a small deterministic library of tree prototypes rather than a
   unique high-detail topology for every placement.
2. Use the existing `Decorations::trees()` points, terrain normals, moisture,
   and seed-derived scale/rotation to choose and place prototypes.
3. Append every transformed trunk/branch prototype into one combined wood mesh.
4. Generate leaf-cluster geometry at terminal branch rings and append all of it
   into one combined foliage mesh.
5. Export the two forest meshes through island-owned APIs, tile them if vertex
   counts or streaming require it, and assign separate wood and foliage
   materials in Unity.
6. Add density, slope, river-clearance, rock-clearance, LOD, culling, wind, and
   shadow policies only after the single-tree shape and scale are accepted.

The first milestone should not implement any of these forest-placement or
foliage tasks. Its success criterion is one attractive, deterministic,
branch-only procedural tree in `TreeSandbox`, with the native ownership and
two-mesh boundary ready for the next phase.

## Validation sequence

1. Run focused tree geometry and determinism tests.
2. Run focused FFI creation/release tests.
3. Explicitly run the ignored FFI allocation lifecycle test.
4. Run `cargo fmt --all`.
5. Run `cargo clippy --all-targets -- -D warnings`.
6. Run the ordinary Rust test suite and distinguish any known baseline
   integration failures.
7. Rebuild and deploy the release native plugin; verify checksums.
8. Compile and validate `TreeSandbox` in Unity.
9. Restart the open Unity editor and visually inspect several seeds before
   beginning foliage or forest placement.
