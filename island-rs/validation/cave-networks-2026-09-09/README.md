# Cave network checkpoint — 9 September 2026

Algorithm revision 8; native snapshot version 8. This extends the earlier
[revision 7 performance checkpoint](../caves-2026-09-09/README.md), whose figures
remain historical single-passage measurements.

## Configuration and sample results

Release native build, default features, profiling disabled; sequential seeds 0–9,
terrain size 128, default cave settings, maximum two branches, lengths 18–30 m,
side chamber scale 1.25. Full results: [corpus.jsonl](corpus.jsonl).

- Eight of ten islands accepted one cave; seven had two side passages and one
  retained its original passage. Two islands had no suitable entrance.
- Whole-island generation ranged from 2.43 to 3.43 seconds
  (median 2.86 seconds).
- Branched islands had 33,608–42,619 render triangles and 36,150–44,875 combined
  passage/entrance-collider triangles. Native cave mesh buffer payload was
  2.14–2.52 MB; this excludes terrain and runtime graphics/physics overhead.
- Branched islands exported five or six chunks including the entrance collider.
  Interior batches are limited to 10,000 triangles each.

Reproduce without writing large island snapshots:

```sh
cargo build --manifest-path island-rs/Cargo.toml --release
island-rs/target/release/island-cave-probe --seed 0 --count 10 --terrain-size 128 --branches 2
```

## Validation

- Full native library suite: 391 passed, zero failures, three existing ignored tests.
- Focused native cave suite: 13 passed. Checks include deterministic branching,
  snapshot topology, invalid settings, walkability of every route, and exact
  shared-edge closure across the entrance and completed network.
- Strict production library/binary Clippy passed both with and without default features.
- Unity 6000.5.6f1 isolated-project EditMode suite: 26 passed, no skips or failures
  (caves, torch, ship views and native runtime contracts). The real seed-0 snapshot
  contains two branches and round-trips branch paths through the native plugin.
- The macOS development player build passed; the validated native plugin was
  installed atomically into the working Unity project. Restart Unity to load it.
- A real Unity CharacterController walks each synthetic branch to its chamber and
  back, both at the origin and translated 18 km. Tests also check collider scope
  restoration, roof traversal and disposal.
- Torch-lit side-junction and chamber renders from the controlled fixture were
  inspected. This is isolated-project evidence; the user's live scene was not
  restarted or saved, and broad lighting/reflection/residency acceptance from the
  earlier checkpoint remains separate.

## Delivered scope

Up to three optional branches off the main passage, each ending in a larger
chamber; configurable lengths and chamber scale; safe rejection/fallback;
connected rounded doorways; branch/chamber diagnostics and debug visit buttons.
Each cave still has one exterior entrance. Loops, additional exits and recursive
branching remain future extensions.
