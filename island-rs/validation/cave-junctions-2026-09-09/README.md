# Junction triangle facing — 9 September 2026

Cave algorithm/native snapshot revision 10. Old cache keys regenerate caves.

At a passage intersection the density gradient can disagree with the orientation
of an individual contour triangle. Revision 9 used that gradient to choose every
triangle's facing independently. The geometric edges remained closed, so the
existing undirected-edge audit missed triangles that faced the wrong way and
were culled by rendering and one-sided collision.

The volume mesher now orients each connected surface using its shared-edge
adjacency. A component-wide gradient vote selects which side faces air. Triangle
indices are adjusted in place; no vertex or mesh buffer is cloned. Zero-area
corner triangles caused by f32 coordinate collapse are removed first, and
non-manifold edges or conflicting orientation are rejected before installation.

Validation:

- The revision-9 wandering fixture has three shared edges with inconsistent
  facing. The new directed-edge assertion fails before the fix and passes after.
- Unity's one-sided ray misses a known junction floor triangle in the old
  fixture and hits it at the expected distance/normal with the new fixture.
- Seventeen native cave tests plus the collapsed-corner regression passed.
  Crossing/loop and wandering fixtures check consistent facing as well as traversal.
- All 29 isolated Unity EditMode cave, torch, ship-view and runtime-contract tests
  passed without skips, including snapshot restoration and translated traversal.
- Strict production library/binary Clippy passed with and without default features.
- The macOS development player build passed. The validated native plugin was
  installed atomically; the live editor was not restarted or saved.
- The [twenty-island corpus](corpus.jsonl) completed without generation errors.
- The fixture retains 45,593 interior triangles. No tessellation was added.

This fixes interior facing at junctions. The previously documented precision
issue in the old exterior terrain-lip cutter remains separate.
