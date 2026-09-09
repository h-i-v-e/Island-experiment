# Cave floor stones — 2026-09-10

Cave revision 13, additive CreateCaveFloorStoneMesh export; snapshot format 10
and existing native option blocks are unchanged. Stones are derived from each
saved cave's actual triangles on export rather than stored in a second cache.

Placement samples walkable floor area near actual wall triangles, protects every
route's 1.3 m centre corridor and checks support under each stone's footprint.
The river generator supplies rock sizes, density noise, shape variation and
partial burial. Each cave has one optional decorative mesh, surface tag 3,
with stone albedo and cave lighting but no collider. Decorations have a separate
1,024-stone/20,480-triangle limit and never truncate passages.

Validation:

- 21 native cave tests passed. Edge placement, support, clear routes, finite
  meshes, unchanged structural geometry and deterministic snapshot round-trip
  are checked for swept and wandering caves. Fixtures produced 103 and 455 stones.
- Eight existing river-rock tests passed.
- Strict production Clippy passed with and without default features.
- 29 isolated Unity tests passed, including native export/release and snapshot
  round-trip, absence of decoration colliders, walking all eleven branches with
  stones at origin and translated 18 km, and the previous junction regression.
- Torch-lit fixture renders were inspected (edge-stones.png and chamber-stones.png).
- Release native library rebuilt and atomically installed; SHA256 `418e5cc05510bf0c2f741ac435da82248d0716355e6f21398cb00ab17d88f3f9`.

The live user scene was not restarted or visually rechecked. Restart Unity to
load the new export and regenerate under the new revision. No standalone player
build, commit or push was performed for this change.
