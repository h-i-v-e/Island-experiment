# Random-walk caves — 9 September 2026

Algorithm/native snapshot revision 9; Unity cache schema remains 7, with the new
24-byte walk settings included in its key. The previous generation entry points
and their option layouts remain supported.

## Behaviour

Each covered step can end its passage or spawn another random walk. There is no
target length, branch count or mandatory terminal chamber. Branches can branch;
paths can cross/reconnect. Smooth changes in heading, width and height feed one
air-volume union, meshed with sparse cube-edge contouring and shared-face decisions.
Only underground surfaces are meshed; the original exterior and entrance are retained.

Defaults: ending probability 0.09, branching probability 0.06 per surviving step,
4 m steps, 55-degree turning control and 0.35 width variation. Branching must be
less likely than ending, ensuring the process dies out on average. Terrain cover,
river hazards, the protected entrance and other cave footprints constrain routes.
A short fixed vestibule preserves the entrance. Floors stay walkable and level.

Emergency work/sampling/installation checks still exist (4,096 attempted steps,
8,192 active sampling chunks, and the existing serialized geometry limits). These
raise an error; they do not silently cut passages short or substitute another
layout. Very low ending probabilities can therefore produce expensive or rejected
generations. Chunking colliders into 10,000-triangle batches does not change shape.

## Validation and sample measurements

- Full native library suite: 395 passed, zero failures, three existing ignored.
- Seventeen native cave tests include probabilistic endings, deterministic paths,
  recursive branches, unioned crossings and a loop retaining a central rock pillar.
- Exact shared-edge checks cover the aligned entrance/volume fixture. A rotated
  fixture checks the closed interior and capsule clearance at large coordinates.
- Unity 6000.5.6f1 isolated-project EditMode tests: 28 passed, zero failures/skips.
  Real CharacterControllers traverse both old and wandering branch fixtures and
  return, at the origin and 18 km away. The seed-5 wandering snapshot restores
  its geometry and branch paths; every new setting changes the snapshot key and
  pending requests keep their own settings copy.
- Strict production library/binary Clippy passed with and without default features.
  The macOS development player build passed.
- Torch-lit wandering junction and passage renders were inspected. These are
  controlled-fixture renders, not validation of the user's unsaved live scene.
- The [20-seed corpus](corpus.jsonl), terrain size 128, generated 18 caves and
  6 branches; two islands had no suitable entrance. Caves had
  8,471–33,393 render triangles.
  Whole-island generation ranged from 2.12 to 3.94 seconds
  (median 3.15 seconds). Other validation processes ran concurrently;
  these are sample costs, not an isolated performance comparison or hardware budget.

Reproduce the corpus:

```sh
cargo build --manifest-path island-rs/Cargo.toml --release
island-rs/target/release/island-cave-probe --seed 0 --count 20 --terrain-size 128 --walk 1
```

## Remaining exterior issue

A stricter rotated synthetic-cliff audit found unmatched exterior terrain-lip
edges, including an approximately 0.8 mm strip reaching the throat. The same
exterior failure reproduces using the old swept passage generator. It is outside
the new closed interior; the existing terrain cutter still needs a separate
precision repair. This checkpoint does not claim every angled exterior join is
watertight. Broad live-world lighting/reflection and residency acceptance also
remain separate from these tests.

## Ownership

The walk owns its paths and final chunk meshes; the terrain is borrowed for cover
checks. Finished geometry is moved into the cave. The short entrance section is
copied into a temporary reverse portal because that helper owns its section;
large terrain or completed network meshes are not copied for each walking step.

Restart Unity after installing the rebuilt plugin. Revision-8 snapshots regenerate
automatically. Set Cave Settings > Random Walk off to use the previous generator.
