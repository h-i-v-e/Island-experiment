# Motu Islands

Procedural Rust-generated terrain, streamed LODs, caves, vegetation, rivers, and
optional Built-in ocean/weather rendering. This is a private 0.1.0 development
package targeting Unity 6000.5.6f1 on macOS Apple Silicon with Metal and the
Built-in render pipeline. Other platforms/pipelines are not currently supported.

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
player, global weather controller, or demonstration scene is required. For depth
refraction in rivers, enable depth textures on the camera you want to use; Motu
islands do not scan or alter every camera in the host project.

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
