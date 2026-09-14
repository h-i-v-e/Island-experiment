# Large boulders — 2026-09-14

The preceding fallen-log/ocean work was committed and pushed as `ccecdd5fdde17107e86df003b467a6a0c8428798`; `origin/main` was verified against that hash. Boulder changes are a subsequent local change. Existing scene/weather/island-factory/wave-tuning edits were preserved.

## Behaviour

Large boulders use the same steep-face source weighting, randomized drop height/velocity and settling simulation as stones. A separate deterministic seed domain supplies one large-body candidate per 32 small-stone candidates, with nominal diameters of 2–6 m at the standard 2 km island scale. Both populations simulate together, so large rocks can displace or support smaller stones. The maximum simulated diameter fits the existing CPU/GPU contact grid's neighbouring-cell search.

The deformed, embedded rock geometry is appended to the existing rock feature mesh, using the same rock material. That feature group is visible during terrain LOD1 and LOD0 and inactive at LOD2. There are no new per-boulder renderers or materials. Geometry can be sliced at the existing rock tile boundaries.

Each large boulder has one sphere enclosing its final rendered vertices. Its centre and radius are persisted with decorations, exported by `CreateBoulderColliders`, copied into managed owner-tile buckets, and installed only with the corresponding LOD0 terrain group. Inactive, hidden, retired and cancelled groups cannot leave active boulder colliders behind. Small stones have no added colliders. Hiding rocks also disables the boulder spheres.

The native export deliberately owns a clone of the small collider array: callers can release the island independently, and `ReleaseBoulderColliders` releases that export. Snapshot version 12 and Unity cache-key schema 9 invalidate older cached generations. Existing cave exclusion and a wider support mask prevent vegetation beneath large boulders.

## Validation

- Rust forest suite: 45 passed, including the generated-island FFI fixture extended with boulder export/release checks and snapshot round trips.
- Rust decoration suite: 10 passed, including deterministic boulder sizing/drop sources, contacts, actual generated boulders, and serialized decoration data.
- Rust river-rock suite: 9 passed, including final-mesh sphere containment and unchanged visual-only small stones.
- Rust boulder-export null-argument/release guards: 1 passed.
- Library Clippy with `--no-default-features -- -D warnings`: passed. All-target Clippy reports only existing cave-test and bark-texture-test warnings.
- Unity 6000.5.6f1 / Metal in the isolated `/tmp/motu-ship-spray.7RcBx7` project: 10 passed (four boulder tests and six fallen-log regressions); see `unity-tests.xml`.
- Scoped `git diff --check`: passed.
- CPU release plugin rebuilt/deployed; build and destination SHA-256 match: `52d29639538357d24fe571e37ffa33612bc6228ccb25b33f0ef6c0d8756fa17f`.

Restart Unity and regenerate islands to inspect the new boulders. The user's running scene has not been visually reviewed; the GPU generation path was not executed during this validation.
