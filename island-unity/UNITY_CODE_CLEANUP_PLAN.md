# Unity code cleanup and rationalisation

Status: implemented. See [delivery and validation results](UNITY_CODE_CLEANUP_RESULTS.md).
Reviewed: 8 September 2026, Unity 6000.5.6f1, branch `main`.
Baseline checkpoint: `e205fe68567ad8f7050fc8330c05903690e9fe96`.

## Recommendation

Introduce the `Motu` namespace hierarchy first, keeping script filenames, asset
GUIDs, serialized fields and assembly membership stable. Then extract editor
validation from runtime classes, remove competing environment ownership, and
simplify generation and streaming responsibilities. Introduce assembly definitions
only after their dependency boundaries work in code.

This is a behaviour-preserving cleanup. Keep the current island distribution,
soil policy, river tuning, material appearance, wave phases, cloud motion, wind
response, teleport behaviour and streaming budgets. Do not combine it with new
terrain algorithms, shader tuning, an engine upgrade or a rendering-pipeline change.

This plan follows the factory/world ownership direction of
[Open-Sea World Architecture](OPEN_SEA_WORLD_ARCHITECTURE_PLAN.md). It supersedes
the older [level plan](../UNITY_LEVEL_ARCHITECTURE_PLAN.md) where that document
places authored island settings or global environment ownership on a standalone
`IslandGenerator`.

## Findings at the original checkpoint

There are 73 C# files under `Assets/Scripts` (19,757 lines) and 9 under
`Assets/Editor` (3,303 lines). None declares a namespace. There are no `.asmdef`
or `.asmref` files under `Assets`.

| Finding | Concrete evidence | Intended outcome |
| --- | --- | --- |
| Global type names and a mixed script root | Controllers, interop, streamers, configuration and player input coexist under `Assets/Scripts` | Namespaces and folders identify each responsibility |
| Global environment ownership still has a legacy path | `IslandGenerator.Environment.cs`, `controlsWorldEnvironment`, and `Islands/IslandRuntimeLoop.cs` coexist with `WorldEnvironmentController.Environment.cs` | One environment owner for all supported scenes |
| Splitting files has not split responsibilities | `IslandGenerator.Materials.cs` is 809 lines; `IslandPreparationPipeline.cs` is 979; `ForestTileStreamer.cs` is 1,089 | Small collaborators with explicit inputs and resource lifetimes |
| Validation reaches through implementation details | `IslandGenerator.Validation.cs` is a 1,282-line runtime partial; editor validation uses private reflection and `GetComponent("OceanWaveMaskComposer")` | Editor/test suites exercise stable contracts |
| A test has become stale | The soil settings test is commented out after the coherent factory moved to height-dependent soil | Keep the useful request/cache assertions and test the current factory policy separately |
| Authored settings and request snapshots remain easy to confuse | `IslandGenerationProfile` clones through JSON, and `IslandGenerationRequest` clones the profile again while exposing mutable settings | One explicit snapshot boundary with tested isolation |
| Generic utility names obscure intent | `RandomTools` is a struct containing only static methods; `RandomPositiveFloat` applies `Abs` to `NextDouble` | Focused static helpers without changing random draw order |

These are investigation findings, not instructions to delete every apparently
redundant path. Check runtime callers and scene references before removal.

## 0. Establish a reproducible baseline

Before the first structural change:

- Record script GUIDs, scene/component bindings, serialized values, custom editor
  targets and public entry points. Include `OpenSeaWorld`, `IslandRuntimeSandbox`,
  `OceanRuntimeSandbox`, `TreeSandbox`, and the ocean profile asset. Keep `_Recovery`
  out of migration and fixture discovery.
- Capture fixed-seed island output and request/cache identities, plus fixed-camera
  renders at fixed weather and solar times. Compare exact native data where the
  refactor should not affect it; use repeatable visual comparisons for rendering.
- Exercise generation, cancellation during preparation and installation,
  regeneration, unloading, minimap teleporting, weather updates and teardown.
  Record native-handle counts and installed Unity resource counts across cycles.
- Make package dependencies and validation entry points work from a clean import,
  then add a player build check. Editor compilation alone will not catch editor-only
  code leaking into a player assembly.

The checkpoint's runtime/GPU wind and cloud suite and native generation with
2.5 m soil passed in an isolated project. That run required removing Visual Studio
integration and explicitly enabling `com.unity.modules.jsonserialize` in the
**temporary project only**. The original clean package configuration hit missing
`UnityEditor.TestTools` types in the IDE package. Resolve and pin the proper IDE,
Test Framework and JSON-module dependencies before calling clean-checkout validation
reproducible; do not rely on an old `Library` directory.

The existing Bevy `TreeBark` match error is outside this Unity refactor. Track it
separately rather than treating a Unity cleanup as a fix for the whole repository.

## 1. Namespace migration — first code change

Use block-scoped namespaces. Choose the destination namespaces now so the same
serialized type does not need repeated namespace migrations merely to match later
folder moves.

| Namespace | Existing code to group |
| --- | --- |
| `Motu.World` | World manager, environment, weather state/driver, clouds, ocean controllers, world surface queries and frame scheduling |
| `Motu.Islands` | `IslandGenerator` and all its partials, runtime/installer/lifecycle, descriptors, requests, factories, preparation and snapshot cache |
| `Motu.Settings` | Authored island settings, environment configuration/settings and wave profiles; runtime weather snapshots remain in `Motu.World` |
| `Motu.Interop` | `MotuNative`, `NativeIslandHandle`, native mesh copying and ABI representations |
| `Motu.Streaming` | Terrain, collider, forest, reed and fern streaming |
| `Motu.Rendering` | Material/texture support, reflections, ambient occlusion, tree mesh rendering and waterfall effects |
| `Motu.Gameplay` | Demo/minimap, orbit camera, first-person control, coherent weather driver and tree preview |
| `Motu.Editor` | Inspectors, project setup and standalone editor validation classes |

Keep tightly coupled prepared-data types with the island preparation pipeline at
first; do not force a namespace split that creates awkward dependencies. Keep every
partial declaration in its owning type's namespace, including nested helper classes
and validation partials that have not yet been extracted.

Migration procedure:

1. Inventory types and classify them with the table above. Update code references
   atomically, including custom inspectors, editor setup, test fixtures and example
   scripts. Do not rename classes or move files in this commit.
2. Preserve `.meta` GUIDs and serialized field names. Audit old fully qualified type
   names and use `MovedFrom` mappings for serialized types that need migration,
   identifying the previous empty namespace and actual source assembly. A namespace
   move is not a serialized-field rename; do not add `FormerlySerializedAs` to
   unchanged fields. Unity's [migration attribute implementation](https://github.com/Unity-Technologies/UnityCsReference/blob/master/Runtime/Export/Scripting/APIUpdating/UpdatedFromAttribute.cs)
   distinguishes namespace, assembly and type identity.
3. Replace string component lookup with typed access where assembly visibility
   permits it. Otherwise provide a narrow test seam rather than a new fragile
   namespace string. Update batch `-executeMethod` names to include `Motu.Editor`.
4. Check `m_EditorClassIdentifier`, UnityEvent targets, custom editor associations
   and any saved type names through Unity loading and reserialization. Do not
   blindly replace strings across scene YAML. No `SerializeReference` usage was
   found in the inspected source, but include an asset audit: managed-reference
   serialization records the [fully qualified class name](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SerializeReference.html).
5. Open all supported scenes and verify zero missing scripts, unchanged component
   references and settings, working custom inspectors, and an operational weather
   driver. Run the baseline suites and player build before accepting the migration.

Acceptance: namespace-only changes preserve output, asset references and cache
keys. Keep assembly names `Assembly-CSharp` and `Assembly-CSharp-Editor` for this step.

## 2. Separate validation from production code

Extract the editor-only validation partials from `IslandGenerator` and
`TerrainTileStreamer` into standalone suites under `Assets/Tests/Editor`. Their
partial declarations cannot simply be moved to a different assembly: first turn
them into separate types that use narrow internal contracts or public behaviour.

Split the large `IslandGeneratorValidation` by concern: native interop and ownership,
scene/factory wiring, rendering, streaming and interaction. Retain the useful
focused wind, cloud, shore-wave, transition and teleport suites and GPU probes.
Create one stable batch entry point that runs the requested group and returns a
clear failure; keep batch methods callable while introducing Test Framework tests.

Restore soil validation against the current contract: standard settings are
configurable; the coherent factory derives depth from its height sample. Avoid
reflecting into `CreateGenerationSettings` just to assert a removed property.
Use behaviour tests for request isolation, cache separation and finite output.

Keep assertions about genuine contracts. A particular seed producing a river is
not a general native-interoperability contract: use a deliberate river fixture for
river UV tests, and keep basic island creation independent of that expectation.

Acceptance: runtime types no longer contain entire test suites, and editor tests
have explicit access rather than broad private reflection. Production validation
at native boundaries stays in production code.

## 3. Make environment and island ownership unambiguous

Keep `IslandWorldManager` as the scene composition point for the environment,
factory, discovery/residency and generation scheduling. Keep factories responsible
for island existence, seeds, palettes and settings. An installed island must not
select its own weather policy or write global sky/fog state.

Audit all callers of `controlsWorldEnvironment` and `worldManaged`. Route any
remaining supported standalone/demo path through the world/factory setup before
removing the legacy environment implementation. The tree preview can remain a
preview tool with explicitly owned preview resources.

Then:

- `WorldEnvironmentController` owns weather state, cloud/sky/solar resources and
  global shader updates. `OceanSurfaceController` keeps ownership of ocean mesh,
  wave transitions and coastal mask composition. Supply explicit references during
  setup instead of expanding use of `FindOrCreate`/scene-wide searches.
- Split environment implementation into focused cloud, solar and shader-binding
  collaborators where useful. Shared shader IDs and coordinate conventions should
  have one documented home. These helpers need not all become MonoBehaviours.
- Remove the duplicated cloud/solar/noise/global-material paths from
  `IslandGenerator` only after their callers are migrated. Extract a shared noise
  texture factory instead of having the world call an island component's static
  texture builder.
- Keep `IslandRuntime` as the owner of installed native handles, materials,
  textures and streamers. Make preparation-to-installation ownership transfer
  explicit and keep cancellation/partial-install disposal idempotent.
- Leave `IslandGenerator` as a small lifecycle facade: accept a request, start or
  cancel preparation, install prepared content and expose state/surface queries.
  Extract material construction and status presentation from the facade.

Acceptance: unloading an island cannot reset global weather, invalidate a global
texture or destroy the sky/ocean; regeneration and cancellation release native and
Unity resources exactly once. All four scenes remain supported.

## 4. Rationalise preparation, settings and streaming

Split `IslandPreparationPipeline` by native export domain where it reduces coupling:
terrain/colliders, vegetation, water/effects and texture data. Keep raw pointers and
ABI layout knowledge behind `Motu.Interop`; expose validated managed results to
installation and rendering. Preserve existing CPU/background work and main-thread
Unity object creation boundaries.

Make a single validated snapshot at request creation. Measure the JSON clone path
before replacing it; if changed, use explicit copy/value types and prove that
mutating authored settings cannot change a pending request. Do not silently change
cache hashes, field defaults, units or native struct layouts during this cleanup.
Centralize only actual duplicated defaults/ranges, not unrelated values that happen
to match. Keep coherent island/weather formulas as policy code with deterministic
random draw order.

For streaming, separate desired tile selection, generation scheduling, upload
budgeting and renderer ownership. Start with the repeated forest/reed/fern tile
lifecycle operations. Extract small shared helpers after comparing the actual
behaviour; avoid a generic streamer hierarchy that hides their distinct LOD and
collider requirements. Reuse `UnityFrameBudget` and make one documented installation
budget apply across collaborators rather than restarting independent budgets.

Move files into feature folders once responsibilities settle, carrying every
`.meta` file with its asset. Convert static-only `RandomTools` to a named static
seed/random helper, remove unused imports and commented-out code, and narrow public
APIs that have no external caller. Preserve useful partials; file length alone is
not a reason to invent another service.

Acceptance: identical generated inputs/output, correct LOD transitions and
teleport recovery, bounded installation work, and no additional steady-frame GC
allocation or resource growth in repeated travel/regeneration tests.

## 5. Add assembly boundaries

Start with a small assembly graph:

- `Motu.Runtime`: production code, initially all runtime feature namespaces.
- `Motu.Editor`: Editor-only tools, referencing runtime.
- `Motu.Tests.Editor`: Editor tests/probes, referencing runtime and the required
  test assemblies. Add a PlayMode test assembly when actual PlayMode tests exist.

Use narrow `InternalsVisibleTo` access for test assemblies where necessary. Do not
make every internal type public merely to compile tests. Keep native plugin import
settings and platform restrictions explicit and verify the player build.

Only extract a separate interop/core assembly later if dependencies and compile-time
measurements justify it. Namespace folders do not each need an assembly. Unity
[does not allow custom assemblies to reference predefined assemblies](https://docs.unity3d.com/6000.0/Documentation/Manual/assembly-definitions-referencing.html),
so a partial move must not leave a runtime dependency stranded in `Assembly-CSharp`.
Assembly changes also require another serialized-type identity check; preserving
namespaces alone does not establish assembly-migration safety.

## Delivery order and completion criteria

Deliver independent, reviewable changes in this order:

1. Reproducible validation/package setup and baseline captures.
2. Namespace-only migration, with scene/serialization checks.
3. Editor validation extraction and current soil/river fixtures.
4. Single environment ownership and removal of verified legacy paths.
5. Island preparation/resource ownership simplification.
6. Streaming/settings cleanup and feature folder moves.
7. Runtime/editor/test assembly definitions and build checks.

Each change must compile, keep the applicable behavioural and visual baselines,
pass its focused checks and leave a runnable project. Keep algorithm/tuning changes
separate and record any intentional migration. A failed scene binding, changed
cache identity, different generated output, leak or player-build failure blocks
that step rather than becoming work hidden in the next refactor.

The implementation follows these boundaries. The delivery report records the
completed changes, validation evidence and retained compatibility fields.
