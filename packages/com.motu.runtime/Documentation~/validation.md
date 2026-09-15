# Validation

From the development repository:

- `scripts/validate-unity-package.sh`: creates deterministic tarballs and a fresh
  host project, runs core package tests without navigation or sample dependencies,
  builds a macOS Apple Silicon consumer, and runs its native generation/render/unload probe.
- `MOTU_IL2CPP=1 scripts/validate-unity-package.sh`: exercises the IL2CPP player variant
  when the editor platform module and Xcode toolchain are installed.
- `island-unity/validate.sh`: isolated development-project contract tests and player build.
  Set `MOTU_TEST_FILTER` to select another named suite.
- `cargo test --manifest-path island-rs/Cargo.toml --no-default-features`: CPU generator tests.
- `cargo clippy --manifest-path island-rs/Cargo.toml --no-default-features --lib -- -D warnings`.
- `Motu.Editor.NoiseBenchmark.Run`: original versus shared immutable-noise creation,
  four resident islands, warm-up plus three measurements. This is not a whole-frame benchmark.

Package tests require the consumer manifest's `testables` list to contain
`com.motu.runtime`. Tests are separate from runtime/player assemblies. GPU tests
need a functioning graphics device; no-graphics compilation is not visual validation.

The development project's detailed tests additionally cover navigation, authored
scenes, waves, shaders, seam correction, boulders and fallen logs. Recorded evidence
and unfinished full-plan gates live in `docs/rationalization` in the repository.
