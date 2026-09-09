# Rounded cave interiors — 10 September 2026

Revision 14 applies bounded shared-vertex relaxation to the oriented interior
mesh before joining its entrance. Twelve simultaneous passes, at most 0.6 m
movement, retain the floor and entrance region. Triangle topology is unchanged;
foldover/collapse moves are rejected. Normals follow the rounded mesh rather
than the original hard-union density gradient. Render and collision geometry
share the result; floor stones are placed against that final surface.

The implementation borrows the mesh mutably. Two cloned position buffers retain
simultaneous updates and the original positions bound total movement; adjacency
and normal accumulation buffers are local to generation.

Validation:

- 22 native cave tests passed, including explicit crease rounding, shared vertex
  and normal equality, anchored boundaries, movement limits, triangle facing,
  closed oblique interiors, walk clearance, pillars and deterministic stones.
- Strict production Clippy passed with and without default features.
- Release library and cave probe built successfully.
- 29 isolated Unity cave/runtime tests passed, none skipped: new native fixtures,
  decorated branch traversal at origin and 18 km translation, shader compilation,
  older snapshot compatibility and the existing precise junction regression.
- All 20 terrain-size-128 islands (seeds 0–19, branch probability 0.5) generated:
  18 caves and 242 branches.
  Largest structural mesh: 95,240 triangles.
- The initial smoothing attempt widened a sub-millimetre seam on seed 18; an
  isolated copy with smoothing disabled succeeded. Pinning unmatched boundary
  vertices until the existing seam repair fixed it. Final seed 18 has 45 branches
  and passes the full corpus run. A regression asserts boundary positions remain
  exact even when requested smoothing mobility is unrestricted.
- Inspected before/after torch-lit junction captures. Ceiling and passage edges
  are visibly rounded. Some real shadows remain; the live user scene has not
  been visually checked.

The native plugin was replaced atomically. SHA-256: `4f9011837814b208a1dc2c5a9cb43c8361746c88deed361f7b08f091f9e39b6b`.
Native snapshot version remains 10; cave algorithm revision is 14 in native and
Unity cache identity. Restart Unity to load the plugin and regenerate older
cached geometry. No commit or push was requested or performed.
