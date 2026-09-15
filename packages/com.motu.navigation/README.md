# Motu Island Navigation

Optional extension for `com.motu.runtime` 0.1.0. Install the runtime first, then
this package. The extension requires Unity's AI engine module; it does not depend
on the AI Navigation authoring package because it uses UnityEngine.AI directly.

Enable Navigation in the island factory/SingleIsland settings. Agent radius
defaults to **0.5 m**, height to **2 m**, step to **0.4 m**, and slope to **45°**.
Voxel size zero selects radius/3; the height mesh remains enabled. Settings are
captured for each request; regenerate to change them. **Chunk Size** defaults to
128 metres (rounded to whole NavMesh tiles), and **Maximum Build Workers** defaults
to one. Smaller chunks reduce each bake's source/workspace size at the cost of
more builds and border connections. Supported chunk sizes are 32–256 metres.

Navigation uses unchanged full LOD0 triangles plus caves and static obstacles,
independent of visible LODs and player-focused colliders. Grass/leaves are not
obstacles. River beds and below-sea ground are excluded by default. This is land
navigation, not a wading/swimming system.

```csharp
using Motu.Navigation;

var navigation = island.Generator.Runtime.Navigation as IslandNavigation;
if (navigation != null)
{
    await navigation.BuildCompletion;
    if (navigation.IsReady && navigation.IsRegistered)
    {
        // Configure an inactive agent, sample a valid start using AgentTypeId,
        // and activate it only after it has been placed on the mesh.
        navigation.ConfigureAgent(inactiveAgent);
    }
}
```

The core exposes `IIslandNavigation` for readiness and lifetime without referencing
UnityEngine.AI. This extension registers its installer before scene loading and
in the editor. Builds are serialized; terrain can become visible tens of seconds
before navigation is ready. Unload cancels work safely, dormancy unregisters data,
and visual LOD changes do not rebuild navigation. Source meshes remain alive until
Unity's asynchronous bake operation finishes, including cancellation. Each chunk
uploads only nearby compact geometry and obstacles, bakes with the height mesh
enabled, then releases its temporary source meshes before the next upload.
Short bidirectional NavMesh links connect validated walkable border crossings;
normal NavMeshAgent automatic link traversal must be enabled, or the host must
handle link traversal itself. Border candidates exclude river-bed and steep faces; endpoint and edge queries
reject blocked crossings. Completed chunks and links are owned together
and registered/unregistered with the island. `Data` returns the first chunk for
compatibility; it no longer represents the whole island. `ChunkCount` and `Report`
expose progress, peak chunk source size and total counts. Readiness still means
all chunks and border connections have finished.

Removing this package leaves the core compiling and skips navigation export/build.
A new setting does not provide distant NPC physics colliders: that remains a
separate streaming requirement. Whole-island connectivity and exact visual foot
placement must be validated for the islands/characters used by the host.
