# Terrain Material Vertex Attributes

## Objective

Render terrain from the material state created by erosion rather than inferring
all surface types from elevation and slope. Every exported terrain vertex will
carry geological hardness, loose sediment cover, and final river-bed coverage.
LOD changes and tile clipping must not create visible material discontinuities.

## Attribute contract

Unity mesh colour channels are material data, not display colour:

- `R`: bedrock hardness in `[0, 1]`.
- `G`: loose/deposited cover in `[0, 1]`, normalized using the same 0.002
  terrain-unit depth at which loose cover almost completely masks bedrock
  hardness during erosion.
- `B`: final river-bed membership in `[0, 1]`.
- `A`: reserved and set to `1`.

The final LOD0 terrain owns the authoritative per-vertex field. The Rust FFI
samples this field using barycentric coordinates at each already-sliced export
vertex. Consequently the material array always has the same length and ordering
as that export's vertex array. Reordering, LOD selection, edge morphing, and new
clip vertices cannot detach values from positions.

## Implementation phases

1. Retain final material state
   - Preserve hardness and deposited depth after the last river/tessellation
     pass.
   - Return the final under-river topology mask from river mesh construction.
   - Convert these values into one compact `Vec3` field aligned with final LOD0.

2. Sample attributes for exported meshes
   - Reuse the final terrain triangle index for barycentric lookup.
   - Sample by global UV/XY after slicing rather than relying on source indices.
   - Extend `ExportMesh` ownership so the mesh and parallel attribute buffer
     live until `ReleaseMesh`.
   - Export empty material data for non-terrain meshes such as water and trees.

3. Upload to Unity
   - Extend the C# native layout with the parallel material array.
   - Copy terrain material values into `Mesh.colors`.
   - Require one material value per terrain vertex in background preparation and
     native validation.

4. Classify in the unified shader
   - Modulate slope-driven rock exposure with hardness and loose cover.
   - Colour loose coastal deposits as beach material near sea level.
   - Override the exposed terrain beneath final river coverage with a river-bed
     colour, varying rock/silt character with loose cover.
   - Preserve the hard snow line and shared AO/normal behavior.

## Acceptance criteria

- Every terrain export has `material.length == vertices.length`.
- River and decoration meshes remain valid with no required material buffer.
- A clipped vertex receives the barycentrically interpolated material values at
  its global XY position.
- All LODs sample the same authoritative final field and meet continuously at
  identical positions.
- Rust unit/integration tests and strict Clippy pass.
- The release native library builds and Unity validates the FFI layout.
- The unified shader compiles without errors on Metal.
