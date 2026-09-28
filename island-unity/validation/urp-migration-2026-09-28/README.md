# URP migration — 28 September 2026

Unity 6000.6.0f1, URP 17.6.0, Metal on macOS Apple Silicon.

The sample project now assigns `Assets/Settings/MotuURP.asset` at every quality
level. Render Graph is enabled. The main Forward renderer retains 2x MSAA,
full-resolution opaque colour, copied depth, four shadow cascades and a 150 m
shadow distance. A separate renderer draws simplified planar reflections.

The procedural terrain, rock, tree, foliage, grass, reed, fern, sky, water,
particle and GPU query shaders use URP shader libraries. Explicit shell passes
preserve grass and foliage layers. Ambient occlusion runs before the opaque
colour capture and underwater composition runs after transparents. Camera hooks
use SRP callbacks and offscreen render requests. The imported ship material was
converted using Unity's Standard material upgrader, preserving its textures.

The runtime package now depends on URP. `Motu > Rendering > Configure URP` creates
the required assets in a fresh host project. The package README describes the
renderer layout. The procedural shaders retain material properties and GPU
instancing; SRP Batcher constant-buffer optimization remains separate work.

## Validation

- Full project EditMode suite: **144 passed, 0 failed, 8 skipped**, recorded in
  `tests.xml`. The skipped cave geometry/physics tests require separately generated
  native cave snapshots or network fixtures; no Rust code changed.
- This includes actual GPU checks for waves, refraction, underwater clipping,
  foliage wind, caves and torch lighting, every custom shader pass, camera
  ownership, generation/streaming, and the new AO contact/clear-sky regression.
- Fresh tarball consumer: **12 package tests passed**, macOS player build passed,
  and the player rendered a generated island and passed streaming and ownership
  teardown checks. Retained logs:
  `/var/folders/29/fnbdz_fn4sjcgp2dqdf_jcb80000gn/T/motu-consumer.Z2zfXQ/`.
- Tree, ocean, underwater and populated-island images were inspected. The close
  island view rendered 279 objects with no missing-shader pixels. The full-project validation
  copy and logs are retained at `/tmp/motu-urp-validation/`.
- Main project macOS ARM64 development build passed with all four enabled scenes.
  Build: `/tmp/motu-urp-validation/island-unity/Builds/Motu.app`.
  Log: `/tmp/motu-urp-validation/near-and-build.log`.
- `git diff --check` passed.

The full suite can be repeated with `island-unity/validate.sh --suite all`;
`scripts/validate-unity-package.sh` creates and validates a fresh consumer.
