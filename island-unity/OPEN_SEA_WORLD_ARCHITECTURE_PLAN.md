# Open-Sea World and Multi-Island Architecture Plan

## Goal

Separate the global environment from an individual generated island so that:

- the sky dome, deep-ocean surface, sun, moon, stars, clouds, fog, distance haze,
  and planar reflection system exist independently of any island;
- the sky and ocean follow the player across open sea without exposing their
  finite mesh boundaries;
- islands can be generated, installed, streamed, suspended, and released as
  independent runtime instances;
- generated islands can be serialized by Rust, evicted from memory, and loaded
  from a disk cache without running procedural generation again;
- future voyages can discover and generate deterministic islands ahead of the
  player while the player remains free to move through the global ocean;
- a replacement single-island scene uses the same world/factory path as the
  open-sea scene, without a legacy generator ownership mode.

The first objective is an ownership refactor with visually identical output.
Procedural world placement, long-distance coordinates, persistence, and
generation caching are later stages built on that separation.

## Implementation Status

The first four deployable ownership milestones are now implemented:

- `WorldEnvironmentController` owns the player-relative sky dome, moon light,
  cloud weather texture, and reflection binding outside the generated-island
  root;
- `OceanSurfaceController` owns a snapped, player-relative ocean plane at the
  fixed sea level;
- clearing or regenerating island terrain no longer destroys the global
  environment;
- atmosphere and cloud sampling use stable world/weather coordinates rather
  than the global transform of a single island;
- `_IslandWorldToLocal` is now assigned only to island-bound materials. The sea
  material no longer receives an island matrix or mask;
- `SeaWater.shader` is now the single global deep-ocean refraction, reflection,
  opacity, distortion, lighting, and open-water pass;
- each generated island owns a bounded, edge-faded coastal overlay with its own
  sea mask and transform. The overlay contributes shallow tint and shore foam
  without repeating the deep ocean's GrabPass, refraction, or reflection work;
- immutable `IslandDescriptor` and `IslandGenerationRequest` values now carry
  copied island-generation inputs independently of `IslandGenerator` fields;
- one `IslandRuntime` owns the installed native handle, inactive installation
  root, terrain streamer, per-island materials, generated textures, surface
  maps, texture arrays, and coastal overlay, with idempotent partial-install and
  active-runtime disposal;
- global deep-ocean noise now has an environment lifetime separate from river
  noise, so disposing an island cannot invalidate the persistent ocean.

The first six phases are complete. `IslandWorldManager` now delegates every
grid cell to its required `IIslandGenerationRequestFactory`. The factory owns
both deliberately configured and generated island definitions, returns a
complete request for an island or `null` for open sea, and is the sole source
of per-island settings. There is no manager-owned authored list, generator
template, occupancy fallback, or pre-placed runtime generator. All accepted
requests use the same serialized CPU-generation, incremental installation,
focused terrain-query, active/dormant, unload, and snapshot lifecycle. Phase 6
also provides velocity-aware request priority, obsolete-request cancellation,
stale-result rejection, and a time-based main-thread installation budget. A
hard resident-island budget evicts the least relevant non-focused runtime
before the next native generation can allocate, keeping native handles and
per-island GPU/mesh memory bounded.
Its remaining gates are extended play-mode travel, turn-away cancellation,
unload, return, and determinism testing. Phase 6.5 is now in progress: Rust can
atomically save and directly restore a versioned, compressed, checksummed full
native island snapshot; Unity performs cache lookup/save on the existing worker,
routes restored handles through the normal prepared installer, and enforces a
configurable shared LRU byte budget. Palette-dependent recipe outputs now use a
separate content-addressed, checksummed bundle keyed by a Rust-owned revision of
the embedded recipes and texture algorithm, so cache hits also avoid recipe
baking. The manifest/free-space limits, durable deltas, and extended travel/load
testing remain before Phase 7 introduces a floating origin.

## Starting Architecture and Constraints

### `IslandGenerator` originally owned both the world and one island

Before the ownership extraction, `Assets/Scripts/IslandGenerator.cs` owned two
different lifetimes:

Global environment state:

- the sea plane and sea material;
- the generated sky-dome mesh and material;
- sun and moon lighting, solar clock, lunar phase, and star visibility;
- clouds, the weather texture, fog, distance haze, and shader globals;
- the planar-water reflection integration.

Per-island state:

- `NativeIslandHandle`;
- terrain, river, vegetation, rock, and collider data;
- `TerrainTileStreamer` and its child streamers;
- island material textures and surface maps;
- the island transform and `_IslandWorldToLocal` matrix.

`ClearGeneratedContent()` and `DestroyRuntimeMaterials()` consequently remove
the global environment at the same time as the island. That makes an open-sea
period with no active island impossible and prevents two islands from owning
their resources independently.

### The prepared island result contains global geometry

`IslandPreparedData` includes a prepared sky-dome mesh. A sky dome is therefore
generated, transferred, installed, and destroyed as though it belongs to each
island. In the target architecture the global environment creates its enclosure
once from an environment radius; island generation does not return sky data.

### Terrain streaming is already close to being per-island

`TerrainTileStreamer` owns a native island pointer, per-island tile data,
materials, vegetation streamers, and collider neighborhoods. Its use of
`transform.InverseTransformPoint(worldPosition)` means it can remain attached to
an island-local root at an arbitrary world position.

It should become an implementation detail of `IslandRuntime`, with a world
manager deciding which runtimes receive detailed player updates. It must not
publish state that assumes it is the only island.

### Shader globals currently assume one island

`IslandGenerator.UpdateMaterialTransforms()` publishes
`_IslandWorldToLocal` globally as well as assigning it to materials. Global
cloud and sky calculations also use that island transform. Two islands at
different transforms cannot both be correct under that contract.

The refactor must distinguish:

- global atmosphere/weather coordinates;
- global ocean coordinates;
- per-island local coordinates supplied on each island material or renderer.

No global shader property may represent "the current island" after the
multi-island stage.

### The sea shader combines two different responsibilities

`Assets/Shaders/SeaWater.shader` renders the global body of water but also uses
one `_SeaMask`, `_WorldSize`, and `_IslandWorldToLocal` to calculate local
depth, shore waves, and coastal effects. A player-relative ocean cannot bind one
island's mask when several islands are visible.

The design therefore splits:

1. a global deep-ocean surface, responsible for the water body, lighting,
   reflection, refraction, distortion, horizon continuity, and open-sea waves;
2. a per-island coastal-effects surface, responsible for that island's mask,
   shallow-water transitions, shore waves, echo waves, and foam.

The coastal surface must be an effect overlay or local patch, not a second
opaque copy of the complete sea surface. Its depth/blend policy must prevent
z-fighting and double refraction.

### Native generation concurrency is not yet a proven contract

Island preparation currently runs through `IslandGenerationWorker` and owns one
native result handle. Until the Rust generator, FFI allocations, recipe baker,
and Unity marshaling paths have explicit concurrent-generation tests, the world
manager will serialize native island generation. Generation remains
asynchronous from the Unity main thread; the initial worker concurrency limit is
one.

## Target Ownership Model

```text
OpenSeaWorldRoot
├── WorldEnvironmentController
│   ├── PlayerRelativeSkyDome
│   ├── PlayerRelativeOcean
│   ├── SunLight
│   ├── MoonLight
│   ├── CloudWeatherState
│   └── PlanarReflectionBinding
├── WorldCoordinateService
├── IslandWorldManager
│   ├── IslandRuntime [island A]
│   │   ├── NativeIslandHandle
│   │   ├── TerrainTileStreamer
│   │   ├── Rivers and waterfalls
│   │   ├── Vegetation and colliders
│   │   └── CoastalWaterOverlay
│   ├── IslandRuntime [island B]
│   ├── GenerationQueue
│   └── IslandDiskCache
└── Player / vessel
```

### `WorldEnvironmentController`

Add `Assets/Scripts/World/WorldEnvironmentController.cs` as the sole owner of:

- the player-relative sky enclosure;
- the player-relative deep-ocean mesh and material;
- global sea level;
- sun, moon, stars, solar time, lunar phase, and ambient lighting;
- global clouds, weather map, wind offset, cloud shadows, and celestial
  obscuration;
- first-person fog and distance haze;
- the global planar-reflection plane and reflection shader state.

The environment exists before the first island and remains alive after every
island has been unloaded. It accepts a follow target independently from the
island streaming target.

Move global settings into `WorldEnvironmentSettings`, preferably a serializable
settings object first to preserve the current inspector workflow. A later
ScriptableObject asset may support reusable climate presets. Keep island
palette, terrain, river, tree, and vegetation settings with island generation.

### `OceanSurfaceController`

Add `Assets/Scripts/World/OceanSurfaceController.cs` to own ocean placement and
material binding. Its responsibilities are deliberately narrow:

- maintain a fixed global sea-level Y;
- follow the authoritative player/camera in XZ;
- snap its XZ anchor to a configurable grid so the transform does not change on
  every sub-pixel movement;
- provide the reflection plane to `PlanarWaterReflection`;
- publish stable logical ocean coordinates to the shader;
- guarantee overlap with the sky enclosure beyond the far clip distance.

The ocean shader must sample waves and distortion from logical world
coordinates, not raw coordinates relative to the moving mesh. Moving or
snapping the mesh must therefore be visually invisible.

### Player-relative sky enclosure

The sky dome follows the same snapped XZ anchor as the ocean, but retains a
stable vertical reference to sea level. Do not center it vertically on a player
who climbs a mountain or flies upward, because that can expose the lower dome
or horizon skirt.

The dome remains an opaque enclosure. Its radius and lower skirt must overlap
the ocean beyond the camera's useful view distance at all supported player
altitudes. Its shading may remain based on view direction, but cloud/weather
lookup must use global weather coordinates rather than island-local space.

Move the environment anchor once in `LateUpdate` using the authoritative player
camera. Do not update it in `Camera.onPreCull`: the planar reflection camera
would otherwise compete with the main camera and move the world twice during a
frame.

### `IslandRuntime`

Add `Assets/Scripts/Islands/IslandRuntime.cs` as the owner of one installed
island:

- stable `IslandDescriptor` and world anchor;
- one `NativeIslandHandle`;
- one runtime root and island-local transform;
- `TerrainTileStreamer` and its child streamers;
- terrain, river, tree, foliage, reed, fern, rock, and waterfall resources;
- generated material textures and surface maps;
- local coast mask and coastal-water overlay;
- collider and terrain-query availability;
- deterministic disposal of all per-island resources.

Suggested explicit lifecycle:

```text
Generating -> Prepared -> Installing -> Active -> Dormant
     |             |            |          |         |
     +----------> Failed <-------+          |         v
                                             +-> Serializing -> DiskCached
                                                                    |
                                             Loading <--------------+
                                                |
                                                +-> Prepared
```

Installation creates Unity objects on the main thread under an inactive root.
Only after every required renderer and streamer has been configured should the
root become active. Failure or cancellation disposes the partial runtime and
its native handle exactly once.

### `IslandDescriptor`

Add a small immutable descriptor containing:

- stable `IslandId`;
- integer world-cell coordinate;
- logical double-precision XZ position;
- generation seed;
- rotation and optional generation profile;
- estimated bounding radius;
- generator/schema version.

Descriptor creation must be cheap and deterministic. Merely discovering an
ocean cell must not generate terrain or allocate a native island.

### `IslandWorldManager`

Add `Assets/Scripts/Islands/IslandWorldManager.cs` to own:

- the player/follow target;
- world seed and deterministic descriptor discovery;
- queued, generating, prepared, active, dormant, failed, and unloading islands;
- generation concurrency, cancellation, retry, and stale-result rejection;
- disk-cache lookup, load, write, invalidation, quota, and eviction queues;
- activation, detail, dormancy, and unload radii with hysteresis;
- per-frame main-thread installation and disposal budgets;
- routing of terrain queries to nearby active islands;
- optional look-ahead based on player velocity and direction.

This manager must never block open-sea movement while waiting for generation.
A failed island leaves navigable ocean, records a diagnostic failure, and may
retry with backoff.

### `IslandGenerationRequest` and `IslandPreparedData`

Decouple preparation from the `IslandGenerator` MonoBehaviour:

- `IslandGenerationRequest` contains copied, validated generation values and a
  descriptor, not scene references;
- the worker returns per-island `IslandPreparedData` only;
- a successful Rust cache load returns the same native island-handle contract
  as generation, allowing the existing export and incremental-install path to
  consume either source without branching throughout Unity;
- Unity `Object` creation remains on the main thread;
- remove `skyDome` and all global environment state from the prepared result;
- make ownership transfer of `NativeIslandHandle` explicit and single-use.

Keep the existing Rust-to-Unity mesh, material-channel, river, and vegetation
contracts unchanged during this extraction. Architectural separation should
not silently alter procedural output.

### `WorldCoordinateService`

Player-relative meshes hide finite rendering geometry, but they do not solve
floating-point precision during long voyages. Add a coordinate service before
unbounded travel is enabled:

- logical world positions use double-precision XZ values;
- Unity scene positions are floats relative to a movable origin;
- island descriptors retain logical positions while island roots retain local
  scene positions;
- an origin shift moves the player, all island roots, and the environment in
  one controlled operation;
- ocean, cloud, wind, and procedural shader coordinates use a high/low split or
  bounded modulo offset so an origin shift does not jump animation patterns.

Physics and character movement should be paused for the shift and resumed after
all transforms and cached spatial values have been updated in the same frame.

## Shader and Rendering Contract

### Eliminate the global current-island matrix

Audit every use of `_IslandWorldToLocal`.

- Terrain, river, rock, vegetation, and tree materials receive their owning
  island's matrix through their material instance or a `MaterialPropertyBlock`.
- A shared material may only be shared across islands if all island-dependent
  values are supplied per renderer.
- `Shader.SetGlobalMatrix("_IslandWorldToLocal", ...)` is removed.
- `CloudCommon.cginc` gains world/weather conversion independent of islands,
  such as `_WorldToWeather` plus a bounded logical weather offset.
- The sky uses camera direction and environment/weather coordinates.
- The global ocean uses logical ocean coordinates and no island transform.
- Rivers continue to use their owning island's local transform.

Add a development assertion or shader-contract validation test that fails if a
global current-island matrix is reintroduced.

### Split deep ocean from coastal effects

Refactor `SeaWater.shader` in two deliberate passes.

Global deep ocean:

- no `_SeaMask`;
- no `_WorldSize` tied to an island;
- no `_IslandWorldToLocal`;
- owns base water colour, open-sea waves, distortion, lighting, cloud shadows,
  reflection, refraction, fog, and horizon behavior.

Per-island coastal overlay:

- receives the island's `_SeaMask`, size, transform, and sea level;
- covers only the mask's bounded world area;
- owns shallow transition, shore-wave, reverse echo, and foam contributions;
- fades to zero before the patch edge;
- samples the same global ocean wave coordinates so motion does not separate;
- uses an explicit render queue, depth offset, and blend model to avoid
  z-fighting, duplicate opacity, or duplicate grab/refraction work.

Initially create the coastal patch as a bounded horizontal mesh slightly above
or depth-biased against the global ocean. If a transparent overlay cannot
preserve the current shallow-water look, use a local stencil/mask pass that
modifies a shared ocean material pass instead of returning to a single global
mask.

### Reflection behavior

`PlanarWaterReflection` remains camera-scoped but is configured against the
global ocean plane. The environment controller owns that binding.

- Only one reflection camera should render for the authoritative gameplay
  camera unless split-screen is explicitly supported.
- Reflection globals are updated for that camera and reset on disable.
- The player-relative ocean and dome are moved before reflection rendering.
- Per-island coastal overlays and rivers remain excluded or included according
  to their current water-layer policy, without recursively reflecting water.

## World Discovery and Island Placement

Use deterministic sparse spatial cells rather than pre-generating a world map.

1. Divide logical XZ space into cells larger than the maximum island diameter
   plus the required navigation gap.
2. Hash `(world seed, cell X, cell Z)` to decide whether a cell contains an
   island and to derive its seed, profile, rotation, and bounded jitter.
3. Reject or deterministically resolve neighboring candidates whose estimated
   bounds overlap.
4. Materialize only cheap descriptors inside the discovery radius.
5. Queue full generation only inside the generation radius or velocity-based
   look-ahead corridor.

Suggested ordering of distances, all with hysteresis:

```text
detail radius < active radius < generation radius < discovery radius
                         unload radius > active radius
```

The generation radius should normally lie beyond the visible horizon or the
maximum island draw distance, giving the worker travel time to finish. Faster
vessels increase the forward look-ahead time rather than globally increasing
the number of active islands.

The first implementation uses one generation worker. Add bounded parallel
workers only after native thread-safety tests and profiling demonstrate that
parallelism is safe and beneficial.

## Runtime Streaming Policy

Each island may occupy one of these operational levels:

- **Descriptor only:** no native handle or Unity objects.
- **Generating:** request in the background queue.
- **Prepared:** native/prepared result waiting for main-thread installation.
- **Active distant:** island root installed, coarse LOD visible, no local
  colliders or expensive vegetation detail.
- **Active focused:** terrain/forest LOD and collider neighborhoods follow the
  player through that island's `TerrainTileStreamer`.
- **Dormant cached:** optional bounded cache retaining prepared CPU/native data
  but no expensive render objects.
- **Disk cached:** no native handle or Unity objects; a validated Rust snapshot
  and durable gameplay deltas remain on disk.
- **Released:** runtime and native handle disposed; deterministic descriptor and
  saved deltas remain.

Only islands close enough to affect the player receive high-frequency streaming
updates. Distant active islands can update visibility at a lower cadence.

Use separate budgets for:

- active island count;
- native handles/prepared CPU memory;
- generated texture GPU memory;
- main-thread mesh uploads per frame;
- collider creation/destruction per frame;
- background generation time and queue length.

Per-island procedural texture palettes may remain unique, but cache identical
recipe/palette results by a stable key where possible. Global sky, cloud, ocean,
and reflection textures are created once and never duplicated per island.

## Player and Gameplay Integration

`FirstPersonController` currently talks directly to one `IslandGenerator` for
streaming preparation and terrain snapping. Replace that dependency with an
interface such as `IWorldSurfaceQuery` owned by `IslandWorldManager`.

The query service should:

- find the nearest active island whose bounds contain the query;
- ask that island runtime to prepare local collider/detail neighborhoods;
- return terrain height and normal when land exists;
- return no land result over open sea;
- avoid selecting a distant island merely because it was generated first.

Boat/swimming behavior is outside this refactor, but open-sea queries must be a
normal state rather than an error.

Convert `IslandDemoController` into a bootstrap/debug controller that can show
world seed, logical player position, queued generation, active island states,
and memory budgets. Retain a compatibility path that creates one descriptor at
the origin for the existing single-island scene.

## Persistence and Reproducibility

Persistence has two separate contracts:

1. a disposable, disk-backed snapshot of the complete generated base island;
2. durable gameplay deltas keyed by stable `IslandId`.

The base snapshot exists to avoid regeneration. Rust must serialize enough of
the native island to recreate a fully usable `NativeIslandHandle` with minimal
work, including terrain and LOD/tile data, material channels, surface-query and
collider inputs, rivers and waterfalls, vegetation placements, rocks, reeds,
ferns, coast masks, palettes, and every other generated result exposed through
the current FFI. Generated recipe textures may either be embedded or referenced
through a content-addressed texture bundle, but a cache hit must not rebake them.
Unity must still recreate transient `Mesh`, `Texture`, `Material`, renderer, and
collider objects on the main thread; that upload is incremental and is the only
substantial reconstitution work after Rust has loaded the snapshot.

Do not serialize Rust heap layout, pointers, trait objects, or Unity objects.
Define stable snapshot wire structs separate from runtime structs. Use a
versioned binary container with:

- a magic value, snapshot-format version, byte order, and generator/schema
  version;
- the full cache key and immutable descriptor;
- a chunk table with bounded offsets and lengths;
- per-chunk checksums and an overall integrity checksum;
- independently compressed large arrays so loading does not require one huge
  managed allocation;
- explicit upper bounds before allocating from corrupt lengths.

The initial implementation should favor reliable reconstruction into owned
Rust arrays over a fragile zero-copy format. The container may later be memory
mapped or selectively loaded by terrain tile after profiling, without changing
the Unity-facing island-handle API.

The cache key must cover every input that can affect generated output: world
and island seeds, descriptor position/rotation/profile, normalized generation
parameters, palette colours, recipe versions, generator/schema version, and
snapshot-format version. A mismatch is a cache miss, never a best-effort load.
Corrupt, truncated, or obsolete entries are quarantined or deleted and safely
fall back to generation.

Expose Rust-owned file operations through a narrow C ABI so the snapshot never
passes through a giant C# `byte[]`. Conceptually:

- save one native handle to a temporary path, validate it, then atomically
  rename it to its cache-key path;
- load and validate a cache path into a new native handle;
- report structured status, byte counts, version/key metadata, and errors;
- support cancellation and guarantee that partial loads/writes publish no
  handle and leave no apparently valid cache entry.

Unity calls those synchronous Rust operations only from its existing background
worker. `IslandWorldManager` performs cache lookup before generation, schedules
loads using the same priority and stale-token rules as generation, and feeds a
loaded handle through the normal prepared/install pipeline. On eviction it
writes a snapshot before releasing the last native handle unless a valid entry
for the same key already exists. Installation and teardown retain their
per-frame budgets; file I/O never runs on the Unity main thread.

Maintain a disk-cache manifest with last-access time, byte size, key/version,
and integrity state. Enforce configurable total bytes and entry counts with LRU
eviction, never delete a file being loaded or written, and reserve enough free
disk space that island caching cannot recreate system disk pressure. Cache
files are disposable; gameplay state is not.

Durable gameplay deltas remain separate and small:

- visited/discovered state;
- collected or destroyed objects;
- player-built or modified content;
- other non-procedural island state.

After loading or regenerating the base island, apply those deltas by stable
generated-object IDs. Snapshot invalidation must therefore never erase player
progress.

## Implementation Phases

### Phase 0: Record behavior and enforce ownership boundaries

1. Capture current day, sunset, night, high-altitude, coast, river-mouth, cloud,
   and reflection screenshots with fixed seed/settings.
2. Record generation time, first-frame installation cost, draw calls, native
   handle count, managed memory, and GPU texture memory.
3. Inventory all `Shader.SetGlobal*` calls and classify each as environment,
   camera/reflection, or accidentally island-global.
4. Add lightweight resource counters around native island handles and runtime
   island roots.
5. Document global sea level as a world contract rather than an island value.

Acceptance: the baseline can distinguish an architectural regression from an
existing visual issue.

### Phase 1: Extract the global environment with no visual change

1. Add `WorldEnvironmentSettings` and `WorldEnvironmentController`.
2. Move sky, sun, moon, stars, cloud texture/state, fog, haze, ambient lighting,
   sea material, and reflection-plane ownership out of `IslandGenerator`.
3. Create the sky enclosure and sea once under `OpenSeaWorldRoot`.
4. Remove the sky dome from `IslandPreparedData` and island cleanup.
5. Keep the environment stationary at the origin temporarily.
6. Update the replacement scenes to reference the extracted world environment
   directly; do not retain island-owned environment compatibility behavior.

Acceptance:

- deleting or regenerating the island does not remove or reset sky, sea,
  clouds, time, fog, or reflection;
- current fixed-seed visuals match the Phase 0 baseline;
- no global resource is destroyed by `IslandGenerator.ClearGeneratedContent()`.

### Phase 2: Make ocean and sky player-relative

1. Add `OceanSurfaceController` and one environment anchor.
2. Follow the authoritative player XZ position in `LateUpdate`, snapped to a
   configurable grid; preserve world sea-level Y.
3. Convert ocean, cloud, and sky sampling to stable environment/logical
   coordinates so transform movement is invisible.
4. Size the ocean and opaque dome/skirt from far clip distance plus a safety
   margin, not from one island's size.
5. Configure `PlanarWaterReflection` from the global ocean transform.
6. Test walking, fast simulated sea travel, high altitude, sunset, moonlight,
   and reflection rendering.

Acceptance:

- no square sea edge or gap beneath the dome is visible;
- anchor snapping causes no wave, cloud, star, or reflection jump;
- the reflection camera cannot move the environment anchor;
- the environment remains valid with zero islands loaded.

### Phase 3: Split global ocean and island coast rendering

1. Remove island mask inputs from the global ocean shader.
2. Add a bounded `CoastalWaterOverlay` renderer/material to each island.
3. Move shore waves, shallow transition, foam, and reverse echo to the overlay
   or an equivalent island-local mask pass.
4. Add patch-edge fading and explicit depth/blend behavior.
5. Test two mock island transforms and overlapping camera visibility before
   adding multiple generated islands.

Acceptance:

- each island has correctly aligned shore effects from its own sea mask;
- the deep ocean is continuous between islands;
- no z-fighting, doubled opacity, duplicated refraction, or mask leakage occurs;
- river/sea mouth behavior remains consistent with the current carving and
  shader-mask contract.

### Phase 4: Extract one self-contained `IslandRuntime`

1. Move native handle, runtime root, per-island materials/textures, streamers,
   local renderers, colliders, and coastal overlay into `IslandRuntime`.
2. Introduce `IslandGenerationRequest` independent of MonoBehaviour fields.
3. Make preparation return only per-island data.
4. Replace global island matrices with per-material or per-renderer values.
5. Implement idempotent cancellation, partial-install cleanup, and disposal.
6. Drive the origin island through the same request-factory and runtime path as
   every other cell.

Acceptance:

- one runtime can be installed at a non-zero transform;
- disposing it releases exactly one native handle and all owned Unity objects;
- global environment state is unchanged;
- a second runtime can coexist without material or transform contamination.

### Phase 5: Prove multiple deliberately configured islands

The implementation is present in `Assets/Scripts/World/IslandWorldManager.cs`
and the `IWorldSurfaceQuery` integration. Add two or three fixed cell entries
to an `IIslandGenerationRequestFactory`. The manager creates runtime generators
only after receiving requests, generates them serially, and is the only object
that assigns detailed terrain streaming focus.

1. Add `IslandWorldManager` with a request factory that owns two or three fixed
   cell definitions.
2. Generate them serially in the background and install them incrementally on
   the main thread.
3. Route terrain/detail queries to the correct runtime.
4. Add active/focused/dormant state transitions and hysteresis.
5. Stress repeated approach, departure, unload, and return.

Acceptance:

- two islands at different positions render correct terrain, rivers, trees,
  coast masks, clouds, and LODs simultaneously;
- the player can leave one island and approach another across uninterrupted
  ocean;
- unloading either island cannot affect the other or the environment;
- no native handle, mesh, material, texture, or collider leak remains after a
  repeated lifecycle test.

### Phase 6: Add deterministic ocean-cell discovery and generation

`IslandWorldManager` scans deterministic grid cells and asks its required
factory about each one. The factory owns fixed definitions, population policy,
and complete per-island parameters. A `null` response is open sea. A returned
request remains data-only until it enters the generation corridor, and the
manager retains a single native generation worker.

1. Add deterministic `IslandDescriptor` construction from world seed/cell.
2. Add sparse placement, jitter, separation checks, and generation profiles.
3. Add discovery, generation, activation, and unload radii.
4. Add velocity-based look-ahead and request prioritization.
5. Add cancellation and stale-result generation tokens.
6. Throttle mesh/material installation to a per-frame time budget.

Acceptance:

- the same world seed and route discover the same island IDs and placements;
- generation begins while the player is in open sea and does not stop movement;
- rapidly changing direction cancels or deprioritizes obsolete work safely;
- failures leave open sea and a retryable diagnostic state rather than blocking
  the world.

### Phase 6.5: Add Rust island snapshots and disk-backed swapping

1. Inventory every field and derived structure behind `NativeIslandHandle` and
   classify it as required snapshot data, cheaply rebuilt transient data, or
   forbidden process-local state.
2. Define versioned Rust wire structs, the binary container header/chunk table,
   integrity checks, allocation bounds, and the complete cache-key algorithm.
3. Implement Rust save/load round trips that return a normal native island
   handle and preserve the current export APIs exactly.
4. Include or content-address all baked material texture outputs so a restored
   island does not rerun recipe baking.
5. Add file-oriented FFI entry points with structured errors, cancellation,
   atomic writes, and exact ownership/release behavior.
6. Add an `IslandDiskCache` service and manifest in Unity. Run lookup, reads,
   writes, checksums, and cleanup exclusively on background workers.
7. Route cache hits through `Prepared -> Installing`; route misses through
   generation followed by a background cache write.
8. Change resident-budget eviction to serialize dirty or missing snapshots
   before disposing the native handle and Unity runtime. Permit immediate
   disposal when an identical validated snapshot already exists.
9. Prefetch cached islands using the existing velocity-aware generation
   corridor, while prioritizing load over generation and rejecting stale
   results with the same request token.
10. Add configurable memory, in-flight I/O, disk-byte, entry-count, and minimum
    free-space budgets, plus LRU disk eviction and cache diagnostics.
11. Keep durable gameplay deltas outside the disposable snapshot and reapply
    them after either load or regeneration.

Acceptance:

- leaving the resident radius releases the island's Unity resources and native
  handle after a valid disk snapshot exists;
- returning loads that snapshot without procedural generation or recipe baking
  and recreates an equivalent island through the normal incremental installer;
- a load remains responsive during fast travel and can be cancelled or rejected
  without leaking native handles or publishing partial Unity objects;
- corrupt, partial, obsolete, or wrong-key files fall back to generation and
  cannot crash Rust or allocate unbounded memory;
- repeated generate/save/unload/load cycles produce stable resource counts and
  preserve gameplay deltas;
- disk quotas and the minimum-free-space reserve prevent cache growth from
  exhausting the host volume.

### Phase 7: Add floating origin for long voyages

1. Introduce logical double-precision positions.
2. Shift the Unity origin beyond a configurable threshold.
3. Update all island roots, environment anchors, spatial caches, and physics in
   one operation.
4. Preserve ocean, clouds, stars, wind, and procedural-noise phase across the
   shift.
5. Test repeated travel far beyond ordinary float precision.

Acceptance:

- terrain, water, vegetation, physics, and camera remain stable after repeated
  origin shifts;
- logical island positions and saved IDs do not change;
- no visible weather or wave discontinuity occurs.

### Phase 8: Persistence, caching, and performance refinement

1. Persist descriptor discovery and per-island gameplay deltas independently
   from the disposable Phase 6.5 base snapshot.
2. Profile snapshot compression, load latency, Unity upload cost, and cache hit
   rate; add memory mapping or tile-selective reads only where measurements
   justify the complexity.
3. Add distant island representations if profiling shows full coarse runtimes
   are too expensive.
4. Evaluate more than one native generation worker only after thread-safety and
   memory-pressure tests pass.
5. Tune radii and budgets against vessel speed and target hardware.

## Validation Strategy

### Automated C# tests

- environment-anchor grid snapping and fixed sea-level Y;
- descriptor determinism and neighboring-cell separation;
- state-machine transitions and hysteresis;
- generation request priority, cancellation, retry, and stale-result rejection;
- idempotent runtime disposal and native-handle accounting;
- terrain-query routing with zero, one, and overlapping island bounds;
- floating-origin logical-to-scene conversion and round trips;
- no global `_IslandWorldToLocal` publication;
- global ocean material has no required island mask;
- coastal overlay receives only its owning island's mask and transform.

### Rust and FFI validation

- retain existing deterministic generation and export tests;
- validate independent handles can coexist even while generation remains
  serialized;
- validate cancellation/release ownership and allocation pairs;
- add explicit concurrency tests before increasing worker count;
- preserve generated mesh and channel contracts during the Unity ownership
  refactor.
- snapshot round trips reproduce every exported array, scalar, descriptor,
  palette, and baked texture byte-for-byte where the current contract is exact;
- a snapshot written in one process loads in a fresh process with no retained
  pointers or hidden generator state;
- truncated chunks, invalid lengths, checksum failures, wrong keys, and old
  versions return structured errors without panics, leaks, or excessive
  allocation;
- save/load cancellation and repeated handle release remain race-free.

### Unity batch validation

- existing native interop and scene validation;
- environment-only scene with no island;
- two-island install and property-isolation scene;
- repeated generate/install/unload cycle with resource counters;
- generate/save/unload/load/reinstall cycles, including application restart and
  corrupt-cache fallback;
- streaming and collider focus transfer between island runtimes;
- reflection camera lifecycle and global reset behavior;
- finite-value mesh checks before every mesh upload.

### Visual validation matrix

Check each relevant phase at:

- sea level, ordinary first-person height, mountain height, and elevated debug
  camera height;
- noon, sunset, moonlit night, dark night, and cloud cover;
- open sea, island approach, coast, river mouth, and between two visible islands;
- stationary, slow movement, rapid movement, anchor snap, and floating-origin
  shift;
- normal view and planar reflection.

Specifically verify opaque horizon closure, clouds meeting the horizon, sea and
sky color continuity, shore-mask alignment, water distortion, celestial
occlusion, and absence of bright halos.

### Performance gates

- environment following allocates no managed memory per frame;
- descriptor discovery does not create Unity objects or native handles;
- background generation never performs Unity API calls;
- snapshot reads, writes, compression, checksums, and manifest maintenance never
  block the Unity main thread;
- main-thread installation observes a configurable millisecond budget;
- active native handles and GPU texture memory stay within explicit caps;
- unload work is amortized when immediate teardown would cause a frame spike;
- cache load plus reinstall is materially faster than regeneration, and cache
  quotas preserve the configured minimum free disk space;
- single-island performance does not regress materially after extraction.

## Migration Policy

- Replace obsolete scenes with factory-owned equivalents rather than retaining
  parallel authored-generator and request-factory modes.
- Remove obsolete serialized fields and compatibility APIs once the replacement
  scenes have been regenerated and validated.
- Do not change procedural defaults, palette generation, terrain channels,
  river geometry, or vegetation placement as part of the ownership refactor.
- Preserve current native-plugin ABI unless a phase explicitly requires an FFI
  addition.
- Land each phase separately so the visual and lifecycle effects can be tested
  and, if necessary, reverted independently.

## Explicit Non-Goals

- A spherical planet, tides, or different sea levels per island.
- Boat, swimming, navigation, or ocean gameplay mechanics.
- Simultaneous native generation before thread safety is demonstrated.
- One enormous fixed ocean mesh.
- A single combined mask texture containing every island coast.
- Per-island skies or weather systems in the initial world model.
- A forever-compatible archival island format; generated base snapshots are
  versioned, disposable caches and may be deliberately invalidated.
- Serialization of Unity `GameObject`, `Mesh`, `Material`, physics, or renderer
  instances; these remain transient and are rebuilt incrementally.

## Completion Criteria

The architecture is ready for long-term multi-island travel when all of the
following are true:

1. Sky, ocean, atmosphere, clouds, lights, fog, and reflection exist and operate
   with no island loaded.
2. The ocean and opaque sky enclosure follow the player without visible edges,
   swimming patterns, reflection errors, or horizon gaps.
3. Island cleanup cannot destroy or reset global environment resources.
4. Two independently transformed islands render and stream correctly at the
   same time, including distinct coast masks and material values.
5. The player can cross open sea while deterministic islands generate ahead in
   a cancellable, bounded background queue.
6. Island installation and unload are incremental, leak-free, and do not cause
   large main-thread stalls.
7. Long voyages use logical coordinates and floating-origin shifts without
   rendering or physics jitter.
8. Returning to an island reproduces its deterministic base and reapplies saved
   gameplay deltas.
9. An evicted island with a valid cache entry returns through Rust snapshot
   loading rather than generation, stays within disk and memory budgets, and
   exposes the same native export contract as a newly generated island.
