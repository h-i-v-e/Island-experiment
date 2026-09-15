# Codebase rationalization, performance, and Unity package plan

Status: implementation in progress. Package extraction and host integration are implemented;
whole-repository review and performance/release gates remain open. See
[the implementation report](docs/rationalization/IMPLEMENTATION.md) for verified results.
Reviewed against the working tree on 15 September 2026, Unity 6000.5.6f1.

## 1. Outcome

Make Motu straightforward to understand, maintain, profile, and install into an
existing Unity project. Deliver a versioned Unity Package Manager (UPM) package
with a small public API, explicit scene dependencies, prebuilt native libraries
for supported platforms, optional demonstration content, and reproducible tests.

The package should support both a single generated island in a host-owned scene
and the existing streamed archipelago. A consumer should not need the Bevy
applications, Material Studio, a Rust installation, or this repository's
ProjectSettings to run a supported prebuilt release.

This is a whole-repository review followed by scoped changes. Every first-party
source file receives a recorded disposition; not every file needs rewriting.
Performance changes require measurements. Readability changes require clearer
responsibilities and contracts, not simply more files or shorter methods.

Preserve the current authored appearance and generation behaviour as the
baseline. Changes to terrain algorithms, erosion morphology, wave equations,
material tuning, or supported rendering pipelines are separate, explicit work.

## 2. Starting point and evidence

The September cleanup already introduced namespaces, runtime/editor/test
assemblies, typed settings copies, and stronger resource ownership. Build on
[its delivery report](island-unity/UNITY_CODE_CLEANUP_RESULTS.md); do not repeat
its namespace migration or propose replacing JSON cloning that was already removed.

The inspected source inventory is:

| Area | Source files | Lines | Review focus |
| --- | ---: | ---: | --- |
| `island-rs/src` | 85 | 57,306 | Generation, geometry, exports, caches, texture baking, experimental compute |
| `island-tree/src` | 21 | 15,097 | Tree algorithms, static meshes, rendering, authoring UI |
| `island-bevy/src` | 37 | 14,652 | Viewer, generation orchestration, caches, rendering, controls |
| `island-material-studio/src` | 17 | 6,539 | Recipe editing, preview scheduling, baking, file IO |
| Unity `Assets/Scripts` | 102 | 20,787 | Generation, streaming, world, rendering, gameplay, navigation |
| Unity `Assets/Editor` | 15 | 2,119 | Inspectors, scene setup, asset generation, diagnostics |
| Unity `Assets/Shaders` | 44 | 8,701 | Terrain, water, atmosphere, vegetation, compute/probes |
| Unity `Assets/Tests` | 52 | 11,161 | Contracts, native fixtures, rendering, lifecycle, navigation |

Counts include `.rs`, `.cs`, `.shader`, `.cginc`, `.compute`, and `.wgsl` under
those roots, including tests embedded in source. They exclude external tests,
assets elsewhere, scripts, and documentation; they are a starting inventory,
not proof of complete review. File length identifies places to inspect, not
an optimization metric.

Concrete integration issues found:

| Finding | Current evidence | Consequence |
| --- | --- | --- |
| Reusable and sample code share an assembly | `Assets/Scripts/Motu.Runtime.asmdef` covers `Gameplay`, preview controllers, world and core runtime | Installing core brings demo dependencies unless boundaries are separated |
| Runtime still reads demo input | `IslandRuntimeLoop.cs` handles debug key presses; `TreeMeshView.cs` handles a key | Core cannot assume the host's input configuration |
| Fixed project paths | `OceanFoamHistory.shader` includes `Assets/Shaders/...`; editor setup writes `Assets/Materials`, `Assets/Scenes`, and `Assets/Textures` | Moving code into Packages can break shaders or target another project's folders |
| Built-in rendering integration | `SeaWater.shader` uses GrabPass; `OceanUnderwaterView.cs` uses OnRenderImage | URP/HDRP support requires rendering work, not only packaging |
| Scene-global state | `WorldEnvironmentController.Environment.cs` writes RenderSettings; ship wave fields and clamps use shader globals | Ownership, camera sequencing, teardown, and coexistence need explicit contracts |
| Implicit scene discovery | Camera.main fallbacks and FindFirstObjectByType calls in world, installation, spray, and buoyancy | Host wiring is harder to predict with multiple cameras/worlds |
| Native release tooling is local-platform specific | `island-rs/deploy-unity.sh` builds a macOS dylib and writes into the Unity Assets tree | Package releases need explicit target artifacts and importer settings |
| Native importer metadata is minimal | `libmotu.dylib.meta` currently contains only format version and GUID | Record and test platform/CPU inclusion rather than relying on inferred defaults |
| Cache policy is fixed | `IslandSnapshotCache.cs` creates `GeneratedIslandCache` below persistentDataPath | Consumers need a documented cache location, budget, disable option, and ownership policy |
| Navigation is directly coupled to installation | `IslandRuntime` exposes the concrete navigation component; installer constructs it | Making navigation independently installable requires a lifecycle seam |
| Documentation spans different implementation generations | Older level, cleanup, shader and Rust performance plans coexist | Establish one index of current contracts and completed/superseded proposals |

Existing validation is useful but incomplete. `island-unity/validate.sh` performs
an isolated project copy, a contract-test subset, and a player build; it does not
prove that a packaged release installs into an unrelated project. No tracked
GitHub Actions files were found under `.github` during this review.

The recent [navigation evidence](island-unity/validation/navigation-2026-09-15/README.md)
records a 1.74-million-triangle full-island bake taking about 32 seconds, with a
sampled process RSS rise of 276 MiB during upload/baking. These are historical
single-fixture measurements, not fresh benchmarks, isolated allocation totals,
or a shipping performance budget. The same report records eight navigation
tests passing and an unrelated stale shore-depth assertion failing. Resolve
that assertion against the intended current contract before declaring a green baseline.

The [Rust performance plan](island-rs/PERFORMANCE_OPTIMISATION_PLAN.md) records
that a faster experimental erosion replacement failed visual validation. Retain
the accepted path-based behaviour; do not revive the rejected algorithm merely
to meet a timing target.

## 3. Definition of easy integration

A release is usable when another developer can:

1. Install the package in a clean supported Unity project, with dependencies resolved.
2. Import a small optional example, or create an island through an Inspector setup
   and a documented script without importing the open-sea demonstration.
3. Assign their own streaming target, camera, materials, lighting, and environment
   policy. Use their existing player controller and input system.
4. Generate, cancel, regenerate, unload, and query an island through documented APIs.
5. Enable Motu water/weather and navigation independently where needed.
6. Build and run a player using the shipped native binary without compiling Rust.
7. Upgrade without lost scene/prefab settings, duplicate script GUIDs, or manual
   edits to package source; uninstall without changing unrelated project assets.

Proposed first release scope: the existing Unity version and macOS Apple Silicon
with Built-in rendering. Revalidate this exact combination in a consumer project.
Windows, Linux, Intel macOS, URP, HDRP, mobile, WebGL, XR, and dedicated servers
remain unclaimed until their build/runtime requirements are implemented and tested.
Choose the next actual host project as the first additional compatibility target.

## 4. Target repository and package boundaries

Keep development applications in the monorepo. Extract reusable Unity content
into a package directory; make `island-unity` consume that package so development
exercises the same boundaries as a consumer.

Suggested layout (package owner/name to be chosen before publication):

```text
packages/
  com.<owner>.motu/
    package.json
    README.md
    CHANGELOG.md
    LICENSE.md
    Third Party Notices.md
    Runtime/
      Islands/ Interop/ Streaming/ Settings/ World/ Rendering/
      Shaders/ Materials/ Resources/ Plugins/
      Motu.Runtime.asmdef
    Editor/
      Inspectors/ Authoring/ Diagnostics/
      Motu.Editor.asmdef
    Tests/
      Editor/
      Runtime/
    Samples~/
      SingleIsland/
      OpenSeaWorld/
    Documentation~/
  com.<owner>.motu.navigation/       # after lifecycle decoupling
island-unity/                       # development and integration project
island-rs/                          # native library, tests, CLI and benchmarks
island-tree/                        # tree library and authoring application
island-bevy/                        # standalone viewer
island-material-studio/             # standalone material authoring application
```

Use UPM's conventional Runtime, Editor, Tests, Samples~, and Documentation~
layout with explicit assembly definitions and retained asset metadata.
[Unity package layout](https://docs.unity3d.com/6000.0/Documentation/Manual/cus-layout.html)
describes these conventions.

Begin with the existing `Motu.Runtime` and `Motu.Editor` identities. Give samples
their own assembly and move navigation into an extension once its core lifecycle
contract exists. Keep rendering in feature folders initially; split another
assembly/package only when optional dependencies or a real consumer justify it.
Do not create an assembly for every namespace.

Dependency direction:

- Samples and authoring tools depend on the public runtime.
- World orchestration invokes island generation and streaming; a single-island
  consumer must not require the demonstration world manager.
- Rendering consumes island/surface/weather data; generation does not call cameras.
- Navigation consumes prepared geometry and lifecycle events. Core does not need
  a concrete Unity navigation component to own an island's lifetime.
- Interop owns ABI knowledge. Gameplay and rendering never interpret raw pointers.
- Rust generation remains independent of Unity and Bevy presentation types.

Keep reusable buoyancy and surface interaction available; classify each file in
`Gameplay` by behaviour rather than moving that entire folder blindly into samples.
Ship input, camera control, minimap presentation, and demo weather policy belong
in examples. Procedural tree and material authoring applications remain optional
tools; their reusable algorithms stay in the libraries that already own them.

For separately distributed extensions, validate dependency resolution through
the actual chosen distribution channel. Avoid assuming sibling Git packages are
automatically installed; document the required manifest entries or publish them
together through a registry.

## 5. Whole-code review method

Create a tracked audit ledger before refactoring. Enumerate all first-party Rust,
C#, shader/include/compute code, tests, native headers, build scripts, manifests,
recipe/schema files, scene/prefab dependencies, and operational documentation.
Identify generated and third-party files separately; inspect their integration
and provenance without treating vendored source as a rewrite target.

Each ledger row must record:

| Field | Required content |
| --- | --- |
| File/subsystem and owner | Responsibility and authoritative implementation |
| Reachability | Runtime callers, serialized references, reflection/string lookup, tooling users |
| Boundary | Public API, internal implementation, sample, editor, generated, or third-party |
| Contracts | Units, coordinate space, thread, determinism, error and cancellation semantics |
| Lifetime | Who allocates, borrows, transfers, caches, and releases each resource |
| Cost | Invocation frequency, data scale, allocations/copies, measured hotspot reference |
| Disposition | Keep, simplify, merge, split, move, deprecate, delete, or investigate |
| Proof | Behaviour/visual fixture, performance comparison, compatibility checks |
| Completion | Reviewed revision, linked change/evidence, unresolved issue or rationale to retain |

Review in dependency order: native data/ABI → preparation → installation/lifetime
→ streaming/queries → rendering/environment → navigation/gameplay integrations
→ editor/tools → tests/documentation. Trace at least one complete generation,
cache-hit, travel, cancellation, and disposal path through all layers.

Deletion requires checking C# and Rust references, scene/prefab GUIDs, shaders,
Resources paths, editor menu methods, exported symbols, reflection, and external
API compatibility. An unused-looking public method or serialized field is not
proof of dead code. Replace parallel concepts only when their contracts match.

Readability rules:

- Use domain names and explicit units, such as `worldSizeMetres` and local/world
  position; document native Z-up to Unity Y-up conversion once.
- Keep settings validation and snapshot ownership explicit. Preserve the existing
  typed copies and deterministic random draw order.
- Extract collaborators around responsibilities and lifetimes, not arbitrary line limits.
- Prefer direct typed calls and small data contracts. Avoid generic service locators,
  excessive interfaces, or streamer base classes that obscure LOD differences.
- Explain non-obvious numerical invariants and rendering workarounds. Remove stale
  comments and document why retained compatibility fields still exist.
- Keep errors actionable: seed, stage, dimensions, native version, and relevant
  configuration; rate-limit repeated runtime warnings.

## 6. Performance baseline and acceptance policy

Create fixed fixtures covering small, typical, and large islands; dense forests;
logs/boulders; caves; river waterfalls; steep mixed-LOD boundaries; and open sea.
Include cold generation, warm snapshot loading, repeated travel, rapid teleport,
cancellation during every stage, regeneration, and complete unload.

For rendering, capture calm and storm seas, distant ocean, shore approach, river
rapids, partial camera submersion, ship contact spray, reflections, and haze/LOD3
transitions. Use fixed camera paths, weather/time, resolution, and quality settings.

Record measurements separately:

| Layer | Measurements |
| --- | --- |
| Native generation | Per-stage wall/CPU time, topology counts, thread count, allocations/peak RSS, cache encode/decode size and time |
| Interop/preparation | Export time, bytes copied, retained buffers, cancellation latency, handle counts |
| Unity main thread | Installation/streaming p50/p95/p99 frame cost, longest uninterruptible operation, GC bytes/frame |
| Rendering | GPU time by pass, batches, draw calls, SetPass calls, triangles, overdraw, texture/RT memory, readback latency |
| Residency | Active/dormant island memory, pooled object high-water marks, physics/collider counts, unload recovery |
| Navigation | Queue delay, source preparation, bake time, peak memory, readiness, complete/partial paths, agent ground error and crowd cost |
| Integration | Clean import time, package/download size, player size, shader variants, recompilation impact |

Use release Rust builds and both representative development-player and release-player
runs. GPU measurements require a working graphics device. Record hardware, OS,
Unity version, native hash, quality settings, seed, cache state, and revision.
Repeat generation/bakes at least three times after a warm-up; collect enough frame
samples to make percentiles meaningful. Keep screenshots alongside machine-readable results.

Phase 1 sets actual budgets from these measurements and the intended hardware.
For planning, use these provisional gates:

- No correctness, scene-binding, visual-contract, or resource-lifetime regression.
- No new steady-state managed allocations in previously allocation-free Motu loops.
- No sustained resource growth across at least 20 generate/travel/unload cycles,
  allowing documented bounded caches to reach their limit.
- No repeatable regression above 5% in frame-time percentiles or peak memory for
  an unrelated scenario; investigate against measurement variance before rejecting.
- Optimization candidates should normally improve their measured hotspot by at
  least 10%, or remove a demonstrated hitch/memory spike, without moving a larger
  cost elsewhere. Small complexity-reducing changes can qualify independently.
- Report whole-frame/whole-generation effects as well as local speedups. Do not
  describe asynchronous work as cheaper merely because it runs off the main thread.

Do not promise a universal FPS target or generation time before measuring a target
machine. Preserve configured installation budgets; document operations such as
mesh upload, collider cooking, and navigation source upload that cannot be yielded
mid-call, then bound their input sizes where profiling shows unacceptable spikes.

## 7. Delivery phases

### Phase 1 — Establish the baseline and audit ledger

Work:

- Record the current dirty changes and their provenance, then establish a reviewed
  baseline containing the intended seam and navigation work. Preserve authored scenes.
- Generate the complete inventory, dependency map, exported API list, and asset/GUID map.
- Reconcile existing plans with current code. Keep the old plans as historical
  records and link current contracts from an index; remove contradictory active guidance.
- Correct stale tests against intentional settings contracts. Separate default-value
  tests from tests of a tunable scene/profile asset; do not change wave tuning to pass a test.
- Expand validation entry points into named native, lifecycle, streaming, graphics,
  navigation, serialization, and player-build groups.
- Capture the performance/visual baseline and agree the initial compatibility matrix.

Deliverables: audit ledger, architecture map, current test report, benchmark fixtures,
performance budgets, and a short list of ranked bottlenecks.

Gate: failures are fixed or explicitly tracked with reproducible evidence; no old
validation run is represented as a fresh green run. Baseline assets are reproducible.

### Phase 2 — Define host integration and isolate the demonstration

Work:

- Specify a small generation API covering request creation, asynchronous generation,
  progress, cancellation, readiness, errors, surface queries, and disposal.
- Retain `IslandGenerator` as the facade rather than adding a competing generator.
  Distinguish terrain-ready, collider-ready, and navigation-ready state.
- Remove sample key handling from runtime loops; route debug visual toggles through
  explicit properties or commands invoked by sample controllers.
- Inject streaming targets, cameras, and optional environment providers. Keep any
  convenience auto-discovery in clearly documented sample/setup paths.
- Provide host-owned environment mode and explicit opt-in Motu environment mode.
  Installing or generating an island must not silently replace camera, sun, fog,
  sky, input settings, tags, layers, quality settings, or build scenes.
- Give shared shader globals a documented owner and camera update order. Test
  additive scenes, Scene/Game cameras, reflection cameras, and owner teardown.
  Declare a single active global ocean limitation if that remains the supported
  design; do not imply multiple independent oceans work without implementing them.
- Introduce a narrow optional-feature lifecycle/data seam for navigation. Preserve
  full unsliced LOD0 sources, height meshes, default 0.5 m radius, and cancellation.
  Make prepared geometry ownership explicit so an extension cannot retain freed data.
- Separate generic inspectors from scene/demo setup tools. Setup commands create
  content only at a user-selected destination and use Undo for scene authoring.

Deliverables: documented public contracts, sample assembly, host-owned scene fixture,
optional navigation boundary, explicit environment ownership.

Gate: a host camera/controller can generate and unload an island without importing
sample controls. No runtime assembly references sample or editor assemblies.

### Phase 3 — Extract and prove the first installable package

Work:

- Move assets with their `.meta` files; keep one canonical copy. Update the
  development project to reference the local package rather than duplicate scripts.
- Preserve runtime assembly/type identities where possible. For necessary migrations,
  use the appropriate type/field migration attributes and verify actual scene and
  prefab loading, managed references, UnityEvents, and custom inspector bindings.
- Replace absolute project shader includes with package-compatible includes. Audit
  test probes as well as runtime shaders. Validate both local and installed layouts.
- Collect required material/shader/texture references into explicit package defaults.
  Audit Shader.Find and Resources.Load paths for name collisions and player stripping.
  Keep generated/user-authored assets out of the read-only installed package directory.
- Declare only consumed package/module dependencies. Navigation currently calls
  UnityEngine.AI; establish whether AI Navigation package APIs are actually needed
  before making `com.unity.ai.navigation` a mandatory dependency.
- Move sample scenes, input bindings, ship assets, debug UI, and sample-only textures
  into optional sample content, subject to asset provenance. Keep useful generic
  interactions available without requiring the pirate-ship sample.
- Add a package manifest, version, changelog, license/usage terms, notices, minimal
  Inspector setup, script example, and supported-platform requirements.
- Keep the current project consuming this extraction continuously.

Deliverables: installable local package, minimal sample, navigation extension,
existing-project migration procedure, and a consumer smoke-test harness.

Gate: install into an empty project outside this checkout, import only the minimal
sample, generate an island, unload it, and build/run a player. Repeat in a fixture
with an existing camera, lights, input setup, and unrelated assets. Verify no
reference to the original Assets tree or warm Library cache is required.

### Phase 4 — Harden native ABI, lifetime, cache, and configuration boundaries

Work:

- Document every exported function's inputs, units, ownership, output validity,
  thread safety, and failure behaviour. Separate ABI declarations from generation
  orchestration in the large `ffi.rs` where responsibilities warrant it.
- Add an explicit native/managed ABI compatibility handshake before generation.
  Verify sizes, alignment, field offsets, integer widths, calling conventions,
  string encoding, and exported symbols for every shipped target.
- Return useful native errors for null handles and invalid/empty exports. Distinguish
  an optional empty mesh from a broken required mesh. Preserve index/finite checks.
- Audit unsafe blocks, borrowed export buffers, panic handling, and native handle
  lifetime. No panic may unwind across C ABI; review the current release panic-abort
  policy rather than assuming catch_unwind can recover every failure.
- Keep cancellation cooperative and honest: cancellation must not free a handle or
  Unity source mesh while native work still uses it. Define latest-request wins,
  bounded worker concurrency, partial installation cleanup, and idempotent unload.
- Map each native, managed, Unity, GPU, and navigation buffer through ownership
  transfer. Remove duplicate retained copies only after proving their consumers.
- Expose cache policy without requiring a cache-service framework: location, byte
  budget, enable/disable, reset scope, schema/version handling, and diagnostics.
  Audit atomic writes, corrupt snapshots, disk-full/read-only failures, and races.
- Keep native generation identity separate from derived material/navigation cache
  identity. Invalidate only what changed, but include every relevant dependency.

Deliverables: ABI contract/tests, ownership map, lifecycle tests, configurable cache
policy, and clear settings documentation.

Gate: mismatched native binaries fail with a clear diagnostic; repeated cancellation,
regeneration, cache corruption, and unload do not leak resources or crash.

### Phase 5 — Optimize native generation and preparation from profiles

Rank and implement one measured bottleneck at a time:

| Area | Investigation and candidate change | Required protection |
| --- | --- | --- |
| Erosion/topology | Repeated neighbour searches, connectivity reconstruction, scratch capacity, independent passes | Preserve hydraulic path ordering and terrain morphology; invalidate adjacency after topology changes |
| Mesh generation/clipping | Full-mesh clones, per-tile allocation, repeated boundary lookup/export | Preserve canonical edge positions, winding, indices, shared LOD profiles and post-clip seam treatment |
| Rivers/caves | Spatial query costs, repeated bounds/topology work, geometry assembly buffers | Preserve river continuity, waterfall shape, cave openings and valid intersections |
| Forest/decorations | Placement query acceleration, prototype reuse, lazy export, repeated settle queries | Preserve deterministic placement, cut log ends, boulder settling and obstacle records |
| Texture/material baking | Duplicate recipe evaluation, temporary arrays, repeated equivalent bakes | Preserve channel meanings, colour space, resolution and per-LOD map semantics |
| FFI/preparation | Repeated copies, unnecessary channels, export batching | Preserve borrowed-buffer lifetime; measure memory peak and main-thread upload together |
| Snapshot IO | Encode/decode copies, compression tradeoffs, loading only needed data | Preserve versioning, validation, deterministic content and bounded memory |

Use Rayon only for demonstrably independent work or deterministic reductions.
Consider Unity MeshData/NativeArray upload paths only when copying/upload is a
measured cost and their lifetime complexity is justified. No blanket unsafe,
Jobs/Burst, pooling, or ECS rewrite.

Deliverables: focused changes with before/after stage and whole-pipeline results.

Gate: fixed-seed invariants and representative visuals remain accepted; performance
improves under identical detail and settings. A faster but visually degraded
algorithm is rejected or kept explicitly experimental.

### Phase 6 — Optimize Unity streaming, physics, and navigation

Work:

- Separate desired residency selection, work scheduling, mesh upload, and object
  ownership where they are currently intertwined. Retain feature-specific LOD rules.
- Profile terrain batch rebuilds, GetIndices/SetIndices use, index format and
  remapping, repeated mesh copies, bounds calculations, and material instantiation.
- Share the frame budget across terrain, vegetation, colliders, and installation.
  Prioritize near-player correctness; bound queued and stale work during teleports.
- Pool only demonstrated churn. Cap pool sizes and release excess after travel.
  Compare batching/instancing strategies using both CPU and GPU costs.
- Measure collider cooking separately. Preserve logs' box and boulders' sphere
  colliders at LOD0 and terrain contact/seam correctness.
- Decouple physics-interest targets from a single player if distant NPC interaction
  needs colliders. Navigation alone does not provide terrain collision to NPC physics.
- Retain whole unsliced LOD0 navigation independent of visual LOD transitions.
  Profile obstacle-source count, source retention, queue delay and agent query cost.
- Evaluate pre-baking or caching navigation only after confirming supported Unity
  data serialization and defining keys for terrain, caves, obstacles, agent settings,
  bake settings, and engine compatibility. Do not cache NavMeshData object bytes
  through an invented unsupported runtime format.
- Treat streaming navigation tiles or lower-resolution bake sources as separate
  design changes, not incidental optimizations of the agreed full-LOD0 approach.

Deliverables: bounded scheduling, measured residency policy, smaller verified
allocation/upload costs, navigation readiness and memory diagnostics.

Gate: seam regression fixtures, all neighbour LOD combinations, steep cliffs,
rapid travel, distant navigation, unload-during-bake, and obstacle tests pass.
Ground-height comparisons and actual moving agents remain within recorded tolerances;
connectivity tests investigate partial paths rather than counting them as success.

### Phase 7 — Rationalize and optimize rendering

Work:

- Document the authoritative terrain material channels, world/local coordinates,
  wave displacement/sampling contract, masks, and shader globals.
- Review TerrainDetail, TerrainCoverageCommon, OceanWaves, sea/river shaders,
  reflections, underwater effects, vegetation, clouds and their shared includes.
  Share actual common calculations while retaining river-specific flow behaviour.
- Profile pass costs and texture fetches before changing them: terrain blending,
  parallax, grass layers, water background capture, planar reflections, foam history,
  cloud shading, shadow passes, and transparent particles.
- Reduce repeated work, unnecessary render-target updates, excessive precision,
  redundant variants, and invisible passes where measurements support it.
- Audit mipmaps, derivative-based filtering, distance/detail fades and shader
  variants. Preserve broad wave warp, distant antialiasing, animated waterfalls,
  calm-water foam suppression, viewport-edge rejection and partial underwater views.
- Keep sea rendering, buoyancy and spray sampling consistent. Reuse calculations
  where valid without adding synchronous GPU readbacks or stale-sample artefacts.
- Make quality controls explicit and measurable. Separate an intentional lower-cost
  visual preset from an optimization that promises identical appearance.
- Establish the rendering adapter boundary needed for future URP/HDRP work, but
  retain Built-in as the first tested implementation. Port depth/refraction,
  reflections, underwater and lighting deliberately when that work is scheduled.

Deliverables: shader contract reference, pass/variant inventory, profiled changes,
visual comparison captures, and documented quality controls.

Gate: GPU/player checks plus manual review of the defined camera/weather fixtures.
A successful shader compile does not establish visual correctness.

### Phase 8 — Review the remaining tools, tests, and repository structure

Work:

- Complete the file ledger across island-tree, island-bevy, material-studio, CLIs,
  scripts and external tests. Investigate large modules such as tree generation/UI,
  Bevy generation/HUD, and recipe document/preview code by responsibility.
- Keep one authoritative recipe evaluator, export contract, and generator algorithm;
  remove duplicated implementations only after comparing consumers and semantics.
- Profile tools separately: preview rebuild frequency, UI allocations, background
  generation cancellation, file watching, cache policy and large mesh/material uploads.
- Review Cargo dependency features and unnecessary renderer/tool dependencies in
  libraries. Consider a Cargo workspace only if it improves coordinated builds without
  forcing unrelated dependency or release changes.
- Align formatting, lint policy, error reporting and reproducible commands. Check all
  supported applications still build; Unity validation is not whole-repository validation.
- Replace broad reflection-heavy tests with narrow contracts where practical; retain
  numerical, visual, interop and lifecycle tests that protect real failure modes.
- Review repository ignores and release contents. Exclude build caches, temporary
  captures and local recovery data without hiding referenced scene/package assets.

Deliverables: complete audit ledger, clear tool/library boundaries, current developer
commands, and resolved or explicitly owned remaining findings.

Gate: every in-scope file has a disposition; all supported tool builds/tests have
current results. Retained complexity has a concrete reason and owner.

### Phase 9 — Validate distribution and release the package

Work:

- Build native artifacts for each declared platform/CPU; verify dependencies,
  symbols, hashes, matching ABI, and PluginImporter settings. Keep unsupported
  binaries out of unrelated player targets. Unity exposes per-platform/CPU plugin
  configuration in the [Plugin Inspector](https://docs.unity3d.com/6000.0/Documentation/Manual/plug-in-inspector.html).
- Parameterize native deployment output and validation scripts instead of hardcoding
  this checkout or the local Unity application path. Preserve atomic artifact publication.
- Test install from a packed release artifact and a pinned Git tag/path, not just
  a local folder. Unity supports subdirectory Git packages and revision pins;
  use that syntax in tested installation instructions.
  [Unity Git dependencies](https://docs.unity3d.com/6000.0/Documentation/Manual/upm-git.html)
- Check Git/LFS artifact availability if applicable; consider a package-only release
  repository or registry if fetching the monorepo becomes excessive.
- Add automated native tests/lint, package structure/reference checks, Unity tests,
  clean consumer imports, and player builds on available licensed runners. Separate
  graphics-required tests from no-graphics jobs. Test IL2CPP on declared targets.
- Enable package tests explicitly in the consumer test project and keep test
  assemblies out of ordinary player dependencies.
  [Unity package tests](https://docs.unity3d.com/6000.0/Documentation/Manual/cus-tests.html)
- Verify a previous version's scenes/settings survive upgrade and package removal
  leaves unrelated assets and project configuration intact.
- Review first-party usage terms and third-party assets/dependencies before external
  distribution. Current Cargo manifests say UNLICENSED; existing notices include
  a Unity Companion License shader adaptation. Inventory textures/models too and
  ship only assets whose redistribution terms permit the intended release.

Deliverables: versioned package artifact, native artifact manifest, changelog,
installation/upgrade guide, API reference, minimal examples, troubleshooting,
compatibility table, benchmark report, and release validation evidence.

Gate: someone can follow the written instructions in a fresh host project without
this development checkout, undocumented setup, Rust tooling, or imported demo
ProjectSettings. Each advertised platform/pipeline has a successful player run.

## 8. Change discipline and sequencing

Recommended order:

1. Baseline and ledger.
2. Host/API and sample separation.
3. First package import and consumer build.
4. ABI, ownership, and cache hardening.
5. Measured native/preparation optimizations.
6. Streaming, physics, and navigation optimizations.
7. Rendering optimizations.
8. Remaining tools and full audit closure.
9. Distribution, compatibility verification, and release.

Use small reviewable commits/PRs per responsibility. Separate file moves from
behaviour changes and separate algorithm changes from cleanup. Do not defer the
consumer build until the final phase; run it after every package-affecting change.

Every implementation change records the problem, invariant, affected public or
serialized surface, applicable tests, and before/after measurement when relevant.
If a gate fails, revert or revise that change before building more work on it.
Keep previous package/native artifacts and schema compatibility information so
rollback is concrete; do not silently downgrade incompatible cached data.

This plan governs cross-codebase sequencing. Existing feature plans remain useful
for detailed invariants, but their historical proposals are not automatically
unfinished tasks. Update their status only after checking implementation evidence.

## 9. Final acceptance checklist

- [ ] Every first-party file and relevant asset/build dependency has a reviewed disposition.
- [ ] Core, samples, editor tools, optional navigation and native ownership have clear boundaries.
- [ ] Current scenes, prefabs, settings and shader references migrate without lost bindings.
- [ ] Host camera, input, lighting and environment can remain host-controlled.
- [ ] Single-island and streamed-world use are documented and tested.
- [ ] Full-resolution navigation retains configurable agent dimensions, default radius 0.5 m.
- [ ] Performance results cover CPU, GPU, memory, streaming and navigation separately.
- [ ] Repeated cancellation, travel, regeneration and unload remain bounded and correct.
- [ ] LOD seams, terrain/material output, water behaviour and obstacle rules retain their fixtures.
- [ ] Clean package import, upgrade, and player execution pass outside the development project.
- [ ] Native artifacts, dependencies, notices, and actual compatibility claims are complete.
- [ ] Installation and example use require no undocumented scene edits or development tools.

The first implementation milestone should be **a minimal island working from the
package in a separate Unity project**, backed by the baseline. That gives the
subsequent optimization work a real integration constraint throughout the review.
