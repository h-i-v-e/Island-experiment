# Unity cleanup delivery

Implemented on 8 September 2026 against Unity 6000.5.6f1. The original runtime
checkpoint was `e205fe6`; the namespace and validation-extraction checkpoints are
`8cb436c` and `9e83565`. This implements the boundaries in
[the cleanup plan](UNITY_CODE_CLEANUP_PLAN.md).

## Delivered

- Block-scoped `Motu` feature namespaces and matching folders. Existing scripts
  retain their `.meta` GUIDs; serialized components/settings have `MovedFrom`
  mappings from their original global `Assembly-CSharp` identities.
- `Motu.Runtime`, `Motu.Editor` and `Motu.Tests.Editor` assemblies. Runtime code
  contains no UnityEditor references or embedded test suites. Only editor and
  test assemblies receive internal access.
- Editor validation is organized into native, ownership, scene, rendering,
  settings, streaming, weather and interaction suites. The old soil test now
  checks the coherent factory's height-dependent policy. River presence/flow
  progression has a deliberate fixture rather than a minimum UV-span assumption
  in general island creation. Tree preview validates selective foliage refinement
  while retaining its shared-vertex/topology checks.
- `IslandWorldManager` supplies the environment explicitly to generated islands.
  Islands borrow global sea/weather resources and cannot install sky, run a
  competing solar/cloud clock, or reset global fog/cloud state during teardown.
  The shared procedural texture factory replaces calls into `IslandGenerator`.
- Terrain, water, vegetation and material exports are separate interop helpers.
  The preparation pipeline orchestrates them and retains its cancellation and
  native-handle `finally` ownership boundary. Raw-pointer checks remain in
  production. Material upload/construction and status formatting have dedicated
  helpers; installation and live material wiring remain cohesive partials.
- Typed settings copies preserve exact value fields and Unity asset references.
  Public request construction snapshots its input; factories transfer fresh owned
  profiles directly. The unused preparation overload with duplicated defaults is
  removed. `IslandRandom` is static and preserves random draw order.
- Reeds and ferns share uploaded-mesh ownership and tile-neighbourhood checks.
  Their incremental scheduling remains distinct, and desired-tile checks no longer
  allocate temporary hash sets. Forest LOD/collider behavior remains specialized.
  Installation continues to share one `UnityFrameBudget` across collaborators.
- Explicit Test Framework/JSON module dependencies fix the original clean-import
  IDE compilation problem. `validate.sh` provides a reproducible isolated import,
  contract-test run and player build, with optional extended native fixtures.

## Verification

The editor contract suite covers all four supported scenes and ocean profile,
script/assembly bindings, weather/cloud/wave behavior, shore breaking, minimap
teleporting, soil policy/cache separation, profile-copy isolation, material cache
round trips, overlapping/cancelled generation, interrupted vegetation travel,
installation cancellation, regeneration and repeated unload/disposal.

A fresh isolated import passed all 12 editor contract tests (46.48 seconds),
followed by a successful macOS development player build. The dedicated seed-666
river fixture and broader native export/render/collider suite also passed.
The clean import caught and resolved stale relative include paths in the relocated
GPU probes; the checked-in probes now use explicit `Assets/Shaders` paths.

The final isolated project also loaded, reserialized and reopened all four scenes
and the ocean profile with no missing scripts. Unity retained the original
`m_EditorClassIdentifier` text while resolving the preserved script GUIDs to
`Motu.Runtime`; those strings were not manually rewritten. The temporary
reserialization was not copied over authored scenes.

Exact comparisons against the original code:

| Fixture | SHA-256, unchanged after cleanup |
| --- | --- |
| Seed 17, 2.5 m initial soil, 33×33 native heightmap | `b68c7c24ea277c4a3524c65f84dea2a2cc5b8bb077c1029cf0f41a436fdb7ca9` |
| Snapshot filename / cache identity | `b104178fb5efc1a57a2b6501e9d9eb557a08b16f1cf356d22c40fcfefa75e712` |
| Initialized OceanRuntimeSandbox, 320×180 RGBA, fixed wave time | `43a8139b8bf5140d44fc8d8c5eb8d1f7ec5590f3eda56f41b1d8989c33537ad6` |

The ocean comparison initializes the sandbox before rendering and contains 4,731
colours. An earlier cleared-camera capture was discarded as insufficient visual
evidence. Wind, cloud, shore and wave-transition GPU probes exercise additional
shader behavior. This is automated validation, not a claim of manual gameplay QA
or exhaustive visual coverage of all procedural seeds.

A local editor measurement of 1,000 profile copies changed from 38.45 ms with JSON
cloning to 0.34 ms with typed copying. This measures copying, not total frame time.

## Preserved scope

Native algorithms, native ABI, cache schema, shaders used by the game, weather
formulas, wave tuning, materials, authored scene values and plugin importer settings
are unchanged. Legacy serialized cloud/solar authoring slots remain readable;
island runtime code no longer uses them to own global weather. Further removal of
those serialized slots would be a separate asset-schema migration.

Useful installation/live-material partials and specialized forest scheduling are
retained. There is no generic streamer hierarchy or separate assembly for every
namespace. The existing Rust caves work, island-tree lockfile and Unity recovery
assets are outside this delivery. The unrelated Bevy `TreeBark` build issue remains
outside the Unity checks.


Local validation evidence retained for this run:

- `/var/folders/29/fnbdz_fn4sjcgp2dqdf_jcb80000gn/T/motu-unity-validation.ewnygg/tests.xml`
- `/var/folders/29/fnbdz_fn4sjcgp2dqdf_jcb80000gn/T/motu-unity-validation.ewnygg/player-build.log`
- `/tmp/motu-cleanup-native-final.log`
- `/tmp/motu-cleanup-final.log`

These temporary paths are local evidence, not repository dependencies. Use
`./validate.sh` to reproduce the clean import, editor suite and player build.
