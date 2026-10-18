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

Forest detail is exported only for nearby LOD1/LOD0 regions, through one forest
worker across resident islands. Uploads use the installation frame budget, then
release their managed source arrays. Coarse canopies remain visible while detail
loads; moving away or clearing focus releases detailed Unity meshes and LOD0
trunk/log colliders. Native forest geometry and coarse rendering remain resident.
A single mesh upload or collider operation can exceed the cooperative budget.

For lower-level use, construct `IslandGenerationRequest` from a profile and palette
and call public `IslandGenerator.GenerateAsync(request, cancellationToken, budget)`.
`Runtime`, `HasRuntime`, `HasActiveRuntime`, `Status`, `IsGenerating`, terrain
queries, and `Clear()` expose lifetime and readiness. Terrain ready does not mean
navigation is ready; use the optional navigation package's readiness contract.

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
The default rode length reaches the bottom at 45 degrees when taut. Windage
preserves existing momentum, takes up the slack, and lets bow tension turn the
ship into the wind while normal land collisions remain active. Explicit ship
teleports should call `ResetAfterTeleport()` after moving the rigidbody.
While deployed, bounded yaw torque also turns the physical bow towards the
wind's source (`-wind flow`), including while the rode is slack or the anchor is
dragging in deep water. This applies force through physics and leaves collisions
and the rode limit in control of the resulting motion.

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
