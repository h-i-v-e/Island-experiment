# Fallen logs validation — 2026-09-14

Implemented seeded fallen-log placement, combined wood LOD geometry, distinct end-grain shading, and LOD0-only oriented box colliders.

- `cargo test --manifest-path island-rs/Cargo.toml --lib --no-default-features forest`: 45 tests passed. Includes the real-island native export/release lifecycle, tree/foliage regression coverage, and four new log tests for topology, bark axes, cap tags/UVs, tile ownership, deterministic placement, clearance, terrain/river rejection and disabling density.
- The four log tests were rerun after adding a serialized forest round trip; all passed.
- `cargo clippy --manifest-path island-rs/Cargo.toml --lib --no-default-features -- -D warnings`: passed. All-target Clippy is blocked by existing warnings in cave tests and a bark-texture test; changed log code is clean.
- Unity 6000.5.6f1 EditMode tests ran with Metal in the isolated `/tmp/motu-ship-spray.7RcBx7` project. All six tests passed; see `unity-tests.xml` for final results. Coverage includes cap shading with and without bark parallax, uploaded cap coordinates, a single wood material, LOD0 box creation/orientation/hiding/unloading, native ABI entry point, stationary logs in strong wind, and the existing standing-tree/canopy streaming contract.
- `end-grain-preview.png` is a GPU-rendered shader fixture, inspected visually. It is not a screenshot of a generated forest.
- Native CPU release plugin rebuilt and deployed. SHA-256: `cec3e2590865efbbbba4e1c4acaa215af88030be9eac2e667d7bba901cf239ac`.

The user's running forest has not been visually reviewed. Restart Unity and regenerate to inspect placement density and appearance in the scene. Existing ocean/weather/scene edits remain separate from this change.
