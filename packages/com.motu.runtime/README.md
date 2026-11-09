# Motu Islands

Procedural Rust-generated terrain, streamed LODs, caves, vegetation, rivers, and
optional URP ocean/weather rendering. This is a private 0.1.0 development
package targeting Unity 6000.6.0f1 on macOS Apple Silicon with Metal and the
Universal Render Pipeline 17.6 with Render Graph. Other platforms/pipelines are not currently supported.

## Install

For local development, use Package Manager's **Install package from disk** and
select this directory's `package.json`. For transfer to another project, run
`python3 scripts/pack-unity.py /path/to/output` in the development repository,
then use **Install package from tarball** and select `com.motu.runtime-0.1.0.tgz`.
The prebuilt native library is included; the consumer does not need Rust.

Navigation is optional. Install `com.motu.navigation-0.1.0.tgz` after the runtime
if needed. The host project's manifest must contain both local packages; the
navigation package's runtime dependency then resolves to the installed runtime.
Set macOS Build Settings architecture to **Apple Silicon**; this release does not
ship an Intel binary. Restart Unity when replacing the native binary in an already-running editor.

## Single island in an existing scene

1. Create an empty GameObject at the island's intended centre. Use unit scale and
   Y-axis rotation only. An island spans 2,000 metres; arbitrary world sizes are
   not supported by the current generator/streaming contract.
2. Add **Motu > Single Island**. The required IslandGenerator is added automatically.
3. Assign your player/camera Transform as **Streaming Target**, then tune the
   generation, vegetation, material and navigation settings in the Inspector.
4. Enter Play mode. **Generate On Start** creates the island; turn it off to invoke
   **Generate Island** explicitly. **Clear Island** cancels and releases it.

Your camera, input, lighting, and scene settings remain yours. No sky, ocean,
player, global weather controller, or demonstration scene is required. For river refraction, enable **Depth Texture** and **Opaque Texture** on the URP
asset (opaque downsampling: None). Add `MotuRendererFeature` to a Forward renderer
for the grass/foliage shells and camera effects. Use Copy Depth After Opaques,
Intermediate Texture Always, and disable depth priming. Custom procedural materials
retain their existing properties and instancing; they do not yet use the SRP Batcher.

The sample project has `Assets/Settings/MotuURP.asset` with two renderers: the main
renderer at index 0, and the simplified reflection renderer at index 1. The latter
has an opaque layer mask of Nothing and a `MotuRendererFeature` with Simplified
Reflections enabled; its feature draws simplified passes and falls back to normal
URP passes for imported materials. Hosts using `PlanarWaterReflection` should use
this renderer layout. Render Graph remains enabled; compatibility mode is not used.
The package's **Motu > Rendering > Configure URP** command recreates this
configuration and upgrades Standard materials through Unity's material upgrader.

Material templates are optional: package shader references preserve the runtime
shaders in a player build. Authored overrides are borrowed; generated materials
are owned and released by the island. Identical procedural ground noise is shared
between resident islands and released after the final owner unloads.

LOD1 and LOD2 terrain use separate mesh UV unwraps and normal/occlusion atlases
baked from the detailed native terrain. Their page sizes are 2048 and 1024 pixels
respectively; unused rows are cropped. When fragmented terrain requires multiple
pages, they share a larger texture with remapped UVs and the same texel padding.
Streaming preserves UV chart seams,
and textures use three mip levels to keep filtering within the chart gutters.
LOD0 keeps its existing planar maps. Atlases are derived when preparing an island,
including islands loaded from existing snapshots; no cache deletion is required.
Native development builds must enable `surface-atlas` (`deploy-unity.sh` does this).

```csharp
using Motu.Islands;
using UnityEngine;

public sealed class MyIslandHost : MonoBehaviour
{
    public SingleIsland island;
    public Transform player;

    public async void Generate()
    {
        island.StreamingTarget = player;
        bool ready = await island.GenerateAsync();
        if (!ready) Debug.LogWarning(island.Generator.Status);
    }

    public void Unload() => island.Clear();
}
```

Call Unity APIs on the main thread. A concurrent request to an already-busy
IslandGenerator returns false; cancel it and await completion before retrying.
Generating again after a completed request clears the previous island. Different
islands queue native preparation to bound peak transient memory. Cancellation is
cooperative: an in-progress native call finishes before its handle is released.

Forests use shared instanced broadleaf, pine and palm catalogs. Latitude and
altitude select species; `Variants Per Species` controls the shared prototypes.
Older experiment flags no longer select a different renderer. Every standing
tree within URP's per-camera shadow range casts a live shadow using its current
LOD, independently of the 96-tree detail budget. Nearby offscreen regions submit
shadow-only instances so turning the camera does not discard their shadows.
The range respects the pipeline distance, camera far clip and camera shadow toggle;
distant instanced crowns outside it use the baked ground mask below.
Wood in non-shadow-casting draws receives ambient light only, while foliage
keeps its usual lighting. Nearby 3x3 owner tiles install budgeted trunk and fallen-log colliders;
clearing focus releases those colliders while the visual forest remains resident.
Authored fallen-log graphics use a separate sparse grid of at most 64 mesh tiles.

The shared Rust core bakes canopy-sized overhead circles from all standing tree
placements into a binary R8 shadow mask. Rust renderers can use
`Island::forest_shadow_mask` or reuse their own pixel allocation with
`Island::write_forest_shadow_mask`. Unity's preparation worker calls the same
generator through `CopyHybridForestShadowMask`, writing directly into its upload
buffer; its main thread only uploads the texture and builds mipmaps.
Existing snapshots derive the mask from their saved placements and shared catalog,
so this feature does not invalidate caches or enlarge snapshots.
Filtered mipmaps retain coverage in the
distance; a 2 km island uses a 1024-square texture (about 1.33 MiB including mips),
with no retained CPU pixels or per-frame bake. Ground lighting blends the mask
into URP's live shadow fade, including the last cascade boundary, without
darkening ambient light. Simplified terrain reflections use the same mask.
The bake follows the island transform, is released on unload, and follows the
forest visibility toggle. It approximates an overhead sun, independent of time
of day; it does not replace nearby moving shadows.

Native snapshot version 20 stores climate and compact catalog metadata, restoring
shared prototypes on load. Climate, variants and catalog version participate in
cache identity; older caches regenerate automatically. Restart Unity after
replacing the native plugin.

Reeds and rushes store compact native placements and share one 32-vertex clump
mesh across islands. Only the nearby 3x3 LOD0 owner tiles acquire draw batches,
with at most 128 instances per batch. Height, width, bank slope, colour variation,
stiffness and phase remain per clump; colour, shadow and reflection passes share
the same geometry and wind.

For lower-level use, construct `IslandGenerationRequest` from a profile and palette
and call public `IslandGenerator.GenerateAsync(request, cancellationToken, budget)`.
`Runtime`, `HasRuntime`, `HasActiveRuntime`, `Status`, `IsGenerating`, terrain
queries, and `Clear()` expose lifetime and readiness. Terrain ready does not mean
navigation is ready; use the optional navigation package's readiness contract.

## Building footprint queries

`IslandWorldManager` and a standalone `IslandGenerator` implement
`ITerrainFootprintQuery`. Give the query a rectangle in world XZ, with full width
and depth in metres and an optional Unity Y-axis rotation:

```csharp
var footprint = new TerrainFootprint(
    new Vector2(buildingPosition.x, buildingPosition.z),
    new Vector2(12f, 8f),
    yawDegrees: 30f);
var status = world.QueryTerrainFootprint(footprint, out var ground);

bool canBuild = status == TerrainFootprintQueryStatus.Complete
    && ground.IsDryLand
    && ground.MinimumSeaClearance >= 1f
    && ground.HeightRange <= .75f
    && ground.MaximumSlopeDegrees <= 8f;
if (canBuild)
{
    // Example policy: place a level foundation above the highest terrain.
    float foundationHeight = ground.MaximumHeight;
}
```

An axis-aligned Unity `Rect` is also accepted through
`new TerrainFootprint(worldXZRectangle)`. Rectangle queries use the prepared CPU
collision heightfield, including every intersected lattice cell inside the
rectangle rather than just its corners. Height bounds and maximum slope are
conservative at that lattice's resolution; boundary cells can extend slightly
outside the rectangle. The default 2,000-metre island has about 0.244-metre
heightfield spacing. These are terrain bounds, excluding caves, trees, rocks,
other buildings and animated water waves.

Results include `MinimumHeight`, `MaximumHeight`, `HeightRange`,
`MaximumSlopeDegrees`, `MinimumSeaClearance`, `RequestedArea`, `TerrainArea`,
`OpenSeaArea`, `UnknownArea`, `CoverageFraction` and `TestedCellCount`. Areas are
square metres. A complete query can contain underwater terrain or known open
sea; `IsDryLand` additionally requires full terrain coverage at or above the
base sea plane. The clearance, slope and relief limits in the example are
building policy, so choose them for each structure. Check `HasTerrain` before
using height or slope values: those are `NaN` when no terrain is available.

World queries cross section boundaries, including the rotated south coast.
They leave streaming focus, generation requests and collider residency alone,
and do not depend on the GPU material readback. Expected terrain without a
prepared heightfield returns `Incomplete`; a factory-confirmed empty sea cell
contributes known open-sea coverage. A standalone generator treats portions
outside its terrain as unknown. Scaled or tilted terrain returns
`UnsupportedTransform`; translation and yaw are supported.

The synchronous main-thread call has a shared default budget of 262,144 world
section and heightfield-cell candidate tests. Pass `maximumCellTests` explicitly
for a larger footprint. `WorkLimitExceeded` preserves partial evidence and
never reports complete coverage. Invalid dimensions, non-finite coordinates,
positions beyond the world grid's float precision or an invalid budget return
`InvalidInput`. The query reuses existing sample blocks
and small stack buffers, without copying heightfields or creating colliders.

## Runtime building foundations

Add **Motu > Placement > Terrain Foundation** (`Motu.World.TerrainFoundation`)
to a building or the root of a building/prop group. Active child mesh bounds
define a rectangle aligned with the root's yaw, including scaled and rotated
children. Collider bounds are used when the group has no active meshes.
**Footprint Root** can instead point to a parent group from a child component.

At runtime the component finds an `IslandWorldManager` or `IslandGenerator`,
or uses the explicitly assigned **Terrain Source**. It queries the entire
padded rectangle through `ITerrainFootprintQuery`. Width and depth round up
before probing so the query includes the complete enlarged base. Dimensions
are whole texture repeats on every face: width uses the tile width, depth
uses the shortest shared repeat of tile width and height (for walls and caps),
and thickness uses the tile height. With the default 2.4 by 1.2 metre tile,
both horizontal dimensions grow in 2.4 metre steps and thickness grows downward
in 1.2 metre steps. The level platform top is
exactly `MaximumHeight`; its bottom reaches `MinimumHeight`, plus the optional
**Embedment Metres** below that point. **Minimum Thickness Metres** keeps flat
ground foundations solid. By default the content root moves only vertically
so the lowest building/prop bound rests on the top. Disable **Align Object To
Platform** if a placement system already positions the content.

The generated child has a shared stonework material and a collider. Stonework
repeats in physical metres on all faces instead of stretching to fill each
platform. Padding, thickness, embedment, texture tile dimensions and a material
override are available in the Inspector. An assigned material override uses
the same metre-based UVs. The default tile is 2.4 by 1.2 metres.

Unknown, missing, over-budget and partly open-sea footprints never produce a
base or move the group. Dry land is required by default. Automatic placement
retries missing terrain/source data every half second for up to **Terrain Wait
Seconds** (default 30), then stops with a status message. It never requests
terrain generation or changes streaming focus. A successful component does no
per-frame work. Runtime placement code can build/refit explicitly:

```csharp
var foundation = building.AddComponent<TerrainFoundation>();
foundation.PaddingMetres = .5f;
if (foundation.TryBuild(world))
    Debug.Log($"Foundation top: {foundation.GroundSample.MaximumHeight}");
```

`TryBuild()` auto-detects the provider; `TryBuild(ITerrainFootprintQuery)` accepts
one directly. Inspect `Status`, `GroundSample`, `Footprint`, `IsBuilt` and
`Platform` for placement feedback. Rebuild after moving or changing a group;
the same owned mesh/object is reused, and the generated base is excluded from
subsequent bounds. Failed rebuilds leave a previously valid base untouched.

`Materials/FoundationStonework.recipe.json` is editable in **Island Material
Studio**, the standalone Rust application. The **Coursed masonry** model uses
half-stone staggered courses, slightly varied stone widths, bevelled/chipped
edges, textured stone faces, per-stone colour variation and recessed mortar.
Even course counts make the running bond tile seamlessly vertically.
Its baked albedo, normal, AO,
height, packed mask and metadata live in
`Runtime/Resources/Motu/FoundationStonework/`; the shared Unity URP material is
`Runtime/Resources/Motu/FoundationStonework.mat`. Re-bake with the same Rust
generator used by the Studio, preserving Unity `.meta` files:

```sh
cargo run --manifest-path island-rs/Cargo.toml --no-default-features \
  --bin island-texture-baker -- \
  --recipe packages/com.motu.runtime/Materials/FoundationStonework.recipe.json \
  --output packages/com.motu.runtime/Runtime/Resources/Motu/FoundationStonework \
  --profile motu_unity_terrain --normal-convention open-gl --force
```

## Periodic coastal strips

**Terrain Shape > Coastal Strip** selects a flat coastal section instead of a
circular island. Each section still occupies a 2,000-metre square and uses the
normal preparation, terrain streaming, rivers, materials, vegetation, colliders,
and optional navigation path. Local X follows the strip; local Z runs from the
low seaward edge to the high inland edge. The terrain repeats along X rather
than being physically bent into a cylinder. Each section forms a broad bay
between headlands, with a noisier mountain mass behind it. Fine relief continues
through the shallow bay rather than following the coastal ramp, allowing small
offshore islands to emerge. The existing continental/detail noise controls tune
that relief; water ratio shifts the shoreline without compressing the inland
climb into a thin wall.

`CoherentIslandFactory` builds a complete cylindrical world with a coastal row
at both north/south limits and noise-selected islands in between. The geometry
stays flat and repeats along X. **Coastal Section Count** in the shared coastal
terrain profile sets the circumference: the default eight sections repeat after
16 kilometres. **World Half-Height (Cells)** sets the north/south coast rows;
the existing serialized archipelago radius is retained for scene compatibility.
Both coastlines face the interior, with their higher inland faces pointing out
of the world. Each coastline shares a field and seed across its sections.

The two existing coarse/fine world-noise layers decide whether each interior
square contains an island. They sample a periodic field around the circumference,
without the old circular boundary or sine-based radial weighting. Island seeds,
settings and cached terrain repeat with their canonical X cell while every
physical copy keeps its own world placement. Negative X works the same way.

The player starts at the centre of the square one row north of the bottom coast,
at X=0. That square and its periodic copies always remain empty, regardless of
the seed or island threshold. The demo applies this start to the ship or flying
player while preserving the authored height. `StartingCell` and
`StartingPosition(worldY)` expose the same location for other game controllers.

**Coastal Seam Segments** controls initial boundary sampling. Each terrain
refinement doubles the seam and rear-edge segments and samples their heights
from the original noise. Finished terrain and normals blend back to that field
over 30 metres on either side of joins and inward from the rear mountains. All
render LODs retain the final dense seam profile. LOD1/2 also retain the smoothing
strip with an additional triangle budget to preserve interior detail. **Coastal Seam Blend Width
Metres** controls the wider watershed and river-exclusion collar.
**Coastal Seam Ridge Height Metres** adds a drainage divide at the join, fading
towards the sea; zero disables that extra ridge. The ridge does not
replace boundary constraints: terrain processing and render LODs retain the
shared seam. Pinning freezes the shared original noise profile, so the edge's
height varies with the field instead of becoming a level ridge. The section
boundaries remain grid-aligned in plan; the shorelines and mountain profiles
are irregular. Sections are independent drainage basins, so rivers do not continue
from one section into another.

Coastal settings and the coastal algorithm revision participate in snapshot
cache identity. Repeated copies of a canonical section reuse its generated data
while retaining distinct world placement. Standalone generation settings still
default to circular islands. The bay revision automatically bypasses cached
wall-shaped coastal terrain. A native revision check prevents an older loaded
library from writing that old geometry into the new cache entries.
Coastal generation currently uses the CPU pipeline; explicitly selecting GPU
generation for a coastal section returns an error.
After installing the updated native library, restart Unity before generating a
coast in an editor that has already loaded Motu.

## Coherent weather and travelling squalls

The game and island demo's `CoherentWeatherDriver` components own automatic
squall formation. Their **Travelling squalls** controls include the active limit
(default three, adjustable from one to three) and a **Squall Formation Threshold**
(default 0.45). Formation activity combines cloud coverage (65%) with wind
strength (35%, reaching full activity at 25 m/s). Above the threshold this is
smoothly mapped to a zero-to-one activity value. Current activity continuously
controls the supported population (up to the authored limit), cell strength
and formation rate within the configured minimum/maximum interval. There is
no separate random spawn clock: fair weather resets formation progress, and
a long frame creates at most one new cell.

An assigned coherent driver retains ownership when disabled, so disabling
the driver pauses formation. Existing cloud/rain/lightning and local wind gains
follow the changing weather, blending over **Fade Seconds**. Improving weather
reduces the supported population; excess cells dissolve even when nearby.
Lifetime expiry still protects encounterable cells while weather supports them.
Movement follows the current ambient wind speed, with independent coherent
heading noise wandering up to **45 degrees either side of downwind**. The cone
tracks the live wind direction; each cell has its own heading sequence. The
**Maximum Heading Deviation Degrees** and **Heading Variation Seconds** controls
default to 45 degrees and 90 seconds. A zero angle restores straight downwind
travel. Speed retains its separate coherent variation, from zero to **eight
times ambient wind speed** by default. The wind/wave field's
advection velocity matches the curved motion. A standalone environment with
no scheduling driver, including the weather playground, retains its explicit
automatic preview controls. Manual horizon spawning remains available on both
the driver and environment. New horizon cells try to keep their rain cores
apart; explicitly positioned cells can overlap.

`WorldEnvironmentController.ActiveSquallCount` and `GetSquall(index)` expose the
active cells. `Squall` retains the first active cell for existing displays.
`SpawnSquall(out WorldSquallState spawned)` returns the actual new cell, while
`ClearSquall()` clears all cells. `WindSpeedAt(worldPoint)` and
`SquallWindMultiplierAt(worldXZ)` sample the strongest local cell. Overlapping
gains do not multiply. The same three fields and their advection velocities
drive vegetation, rendered waves and buoyancy queries; each GPU query snapshots
all three fields so a later weather update cannot change an in-flight result.

Under a squall the ocean's existing waves grow with the square root of the local
wind multiplier, including when ambient weather has reached its ordinary height
cap. Three shorter, crest-biased wind-wave bands add geometric chop inside the
same footprint. They use the integrated wind phase and fade through the storm's
edge and lifecycle; outside water keeps its original wave field. Whitecaps cover
more of each crest and deposit more foam, including in already windy weather.
The rendered surface, normals, foam history and buoyancy queries share this
response. Shallow-water limits and island shelter still apply, and culling
bounds include both the increased height and horizontal storm chop.

Squall diameters range from **200 to 1,000 metres across**. Lightning frequency
scales continuously with travel speed: the authored **Lightning Interval Seconds**
applies at top speed, slower cells strike less often, and stopped cells never
strike. Its clock retains phase across speed changes and does not depend on
visual culling. Each strike has one thunder clap, including bolts with several
flash sub-pulses.

Rain intensity increases smoothly from zero at the rain-core edge to full
intensity at its centre, for both the distant rain volume and nearby droplets.
Leaving the cell stops new drops while airborne drops finish their world-space
fall and fade over their lifetime. They remain renderable when the cloud itself
has moved out of view.

The storm also carries a depth-clipped volumetric rain curtain, independently
visible with nearby droplets disabled. Broad wind-slanted shafts and finer
noise fall through the full cloud-to-water column. Their texture filtering
follows ray-sample spacing and camera resolution to avoid distant shimmer.
The whole curtain slants downwind with the actual weather wind and local squall
boost, independently of the cell's wandering travel speed. Nearby drops use
the same drift and spawn upwind so they still pass the viewer in strong wind.
Sea mist stays anchored to the real sea level while the cloud proxy sinks at
the horizon. Rain and cloud share the same integration and scene/water depth,
so nearer hulls and terrain occlude the rain while distant surfaces disappear
gradually through its depth.

A `Resources/SquallAudio` (`SquallAudioProfile`) asset supplies near, medium
and far thunder banks, volume and listening range. TerraNullus uses all four
recordings in each group under `Assets/LightningSounds`. Selection happens at
sound arrival: near is reserved for the emitting storm's active rain core;
medium covers outside the core up to half the current squall visibility range;
far covers the outer half and beyond. With4km visibility, far starts at2km.
The boundary uses the live storm centre, independent of bolt jitter and the
4km hearing range. Each bank varies recordings without immediate repeats.
The former single-clip field remains a fallback for older audio profiles. Thunder travels at a configurable default **343 m/s**, using the
active listener's distance from the fixed strike position and updating for player
movement while the sound is on its way. The default 4 km spatial range carries
horizon thunder, with distance muffling. Custom 3D volume rolloff retains full
volume within 50 m, reaches **75% at 1 km** and **50% at 2 km**, then fades smoothly
to silence at 4 km. These gains multiply the recording and storm's base volume.
Emitted sounds survive storm movement,
culling and retirement; environment release stops their bounded voice pool.

Waterfall audio now follows its spatial range independently of close-detail
mist. Leaving the nearest terrain-detail neighborhood hides mist without
stopping or restarting an audible loop. Slots retire beyond both the visual
retirement margin and the authored audio range, after the linear rolloff is silent.

Each cell has its own cloud/rain/lightning visuals. The render pass shares one
depth copy and draws every visible volume, including when another cell is off
camera. Cloud noise textures remain shared, and close-rain particle budgets
remain bounded per cell. Additional visible volumetric clouds add GPU work;
the active limit is the quality control for that cost.

## World and water

`IslandWorldManager` and `WorldEnvironmentController` retain the existing streamed
archipelago and ocean/weather setup. Opting into that controller deliberately
lets it own global sky, fog, lighting, and Motu shader globals. The current water
implementation supports one global Motu ocean/environment. Multiple independent
water worlds, URP, HDRP, XR and arbitrary origin shifting require additional work.
Sample ship controls, minimap, preview UI and scene construction remain in the
development project, outside this runtime assembly.

With `CoherentIslandFactory`, the player's world Z position controls the shared
solar latitude continuously across the archipelago radius. At the south edge
(`-radius`, in 2 km grid cells), the noon sun is directly overhead; at the north
edge (`+radius`), it is 10 degrees above the horizon. Positions beyond either
edge retain that edge's latitude. Radius edits apply to lighting immediately.
The moon follows the same observer latitude; **Moon Equator Offset Degrees**
tilts its orbit towards the equator from that latitude (zero matches the sun's
orbit latitude). Solar time and lunar phase continue normally while travelling.
Other island factories use the authored world **Sun Latitude Degrees** value.

Island lee attenuation comes from the shared GPU `MotuOceanShelterWaveScale`
field used by ocean geometry and buoyancy. `OceanHullSpray.TryGetWaterlineSurface`
also exposes that sampled fraction through its overload with `out waveScale`,
without another GPU query. `OceanShoreSampler.Sample.Amplitude` includes island
shelter; its `WaveScale` lets audio retain the same attenuation after volume
saturation (`min(WaveScale, Amplitude / fullVolumeAmplitude)`). This keeps even
large storm breakers quieter in protected water.
The **Ship and Shore Wave Audio** sample contains the corresponding hull and
shore audio components and an isolated native audio validation command. TerraNullus
already has these components in `Assets/Scripts`; avoid importing duplicate copies.

`ShipAnchor` adds a slack bow rode to a rigidbody. A sailing controller calls
`SetDeployed(true)` when sails are furled and `SetDeployed(false)` when they are
unfurled. It catches only `TerrainCollider` seabed within 10 metres below mean
sea level; otherwise it keeps drifting and searching beneath the moving bow.
The default rode length reaches the bottom at 45 degrees when taut. Hull windage
acts with every sail setting, including eased sheets in irons and an anchor that
is raised or dragging. Its acceleration scales with squared relative downwind
air speed and is capped in storms. Forward velocity uses the same unboosted
speed as sail pressure through `OceanBuoyancy.ForwardSpeedMultiplier`; the hull's
water resistance continues to oppose the resulting drift. Existing momentum is
preserved. Windage takes up slack, and the native bow rode holds the ship inside
its swing circle while normal land collisions remain active.
Bounded windward yaw assistance starts only when the seabed catch first has a
taut rode carrying real load (`IsTaut`). `HasBitten` retains that first bite until
the anchor is raised, so passing waves or changing wind cannot stop the turn by
temporarily slackening the rode. A dropped, dragging or never-loaded slack
anchor does not automatically turn the bow. Explicit ship teleports should call
`ResetAfterTeleport()` after moving the rigidbody. TerraNullus's
`ShipWindDriftValidation.Run` exercises irons, sailing-speed windage and the
transition from slack drift to an anchored swing using native physics.

`ShipAnchorAudio` plays separate spatial clips for `Dropped`, `BottomContact`
and `Tensioned`. The last event waits for an extended rode carrying real joint
force, and fires once per deployment even if waves or wind changes reload the
rope. `IsTaut` exposes the current loaded state separately from seabed contact.
The [TerraNullus anchor integration patch](../../integrations/terranullus/anchor-integration.patch)
records the host controller hooks, configured ExplorerShip components and prefab
validation command. It references the host's existing sound assets without
redistributing them; those changes are already applied in the local TerraNullus project.

An uninitialized world controller does not change host shader globals or fog.
Initializing it acquires the single global environment slot; a second active
owner is rejected before it creates resources. Disable the owner to restore
host fog, ambient lighting, borrowed sun properties, camera backgrounds/depth
flags, camera effect bindings and Motu cloud/wind globals. Added underwater
components are removed; pre-existing effects are retained. Re-enabling captures
fresh host settings and resumes the same generated sky/ocean resources.

Keep the host scene active while the environment is enabled. Loading/unloading
Motu in a separate additive scene is supported. **Disable the environment before
calling SetActiveScene**: Unity keeps RenderSettings per active scene. An unexpected
active-scene switch disables Motu to protect incoming lighting, but cannot restore
the departed scene without changing the host's active scene again. Camera/light
properties controlled by Motu should have no competing writers while it owns them.

## Cache and native compatibility

Generation settings expose **Use Snapshot Cache**, **Snapshot Cache Budget GiB**,
and an optional absolute **Snapshot Cache Directory**. Empty uses
`Application.persistentDataPath/GeneratedIslandCache`. Choose a directory owned
by Motu: cache eviction removes only completed hash-named `.motusnapshot` and
`.motumaterials` entries. Temporary and unrelated files are left alone.
Constructing a request does not create directories. Disabled caching performs no
snapshot cache writes. Material caches use the same selected directory.

Managed/native ABI version and record layouts are checked before native entry points.
An unavailable or outdated binary reports an actionable error. Runtime generation
still retains the original geometric, indexing and finite-data checks.

See [architecture and ownership](Documentation~/architecture.md), and
[validation](Documentation~/validation.md). License/third-party notices are included;
this private package is not an open-source or public asset distribution grant.
