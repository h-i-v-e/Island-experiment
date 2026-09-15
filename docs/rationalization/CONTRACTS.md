# Current integration and rendering contracts

This is the current contract index. Earlier feature plans and dated validation
folders remain historical records; their old `Assets/Scripts` and `Assets/Shaders`
paths refer to the corresponding code now under `packages/com.motu.runtime`.

## Coordinates and mesh ownership

`IslandMeshInterop.CopyMeshData` converts normalized Rust Z-up geometry into
Unity Y-up island-local metres. It reverses triangle winding because the axis
swap changes handedness. An island spans 2,000 m, uses unit world scale and may
rotate around Y. These assumptions also apply to streaming keys, surface queries,
coastal masks and collider coordinates; arbitrary scale or origin shifting is
not advertised by this package.

Native export records borrow buffers from an owning native export handle. Copy
and validate before the matching release call in `finally`. Prepared data owns
managed buffers; the installer transfers the native island handle into
`IslandRuntime`. Background terrain exports acquire a native handle lease;
unload cancels queued work and closes the owner without waiting on the main
thread. The final lease releases the native island after its reads/copies end.
Navigation owns its uploaded Unity sources until its async bake is finished,
including cancelled bakes. A caller must await generation after cancellation
before reusing a busy facade.

## Terrain channel map

| Unity channel | Authoritative meaning |
| --- | --- |
| Vertex colour R | Geological hardness; not automatically exposed rock |
| Vertex colour G | Loose surface cover |
| Vertex colour B | River-bed contribution |
| Vertex colour A | Sea proximity |
| UV2 X | Forest-floor contribution |
| UV2 Y | Fallen-stone contribution |

`TerrainCoverageCommon.cginc` owns coverage evaluation. Terrain texture-array
layers are dirt, forest floor, rock, river bed, beach, and stones, in that order.
`IslandMaterialTextureCache` stores derived material maps separately from native
geometry identity. Terrain, grass and LOD shaders consume those same coverage
contracts; material changes must keep them synchronized.

These UV2 meanings apply to terrain only. Fallen wood uses the generic mesh's
second UV channel for end-grain coordinates. River and particle channels have
feature-specific meanings and must not be repacked as terrain attributes.

## Water and environment

`OceanWaves.cginc` owns analytic sea/coastal waves. Shared query shaders use its
integrated phase banks and wind offsets. Geometry detail fades do not mean the
analytic wave has zero amplitude: buoyancy, whitecaps and spray must distinguish
those quantities. Existing broad domain warp, distant antialiasing and calm-water
foam thresholds were preserved by extraction.

`OceanSurfaceSampler` handles GPU samples/readback confidence. Avoid synchronous
readbacks in interaction loops. River surface flow, depth refraction and waterfall
noise remain in their dedicated calculations. Planar reflections use a separate
camera with clipping/viewport guards; underwater rendering supports partial
submersion. The current implementation uses Built-in-specific rendering hooks.

`WorldEnvironmentController` is an opt-in owner of global ocean/weather, sun/fog
and Motu shader globals, with one active ocean assumed. `SingleIsland` does not
create it. A world controller's convenience camera/environment discovery is not
a multi-world routing mechanism. Initialize acquires an exclusive global owner;
disable/unload restores captured fog, ambient, sun, camera and cloud/wind state.
Pre-existing camera effects are borrowed and restored; added underwater effects
are disabled immediately and destroyed using Unity's normal deferred lifecycle.
Re-enable captures fresh host state. No per-frame snapshot allocation is required.

The host scene must remain active during ownership. Disable before switching the
active scene: [Unity stores RenderSettings per active scene](https://docs.unity.cn/Manual/setupmultiplescenes.html).
Motu can live in and unload from a separate additive scene while that host scene
stays active. An unexpected active-scene switch disables the owner and avoids
restoring the old fog/ambient values into the incoming scene; restoration of the
departed scene requires the documented disable-before-switch sequence. Reflection
cameras are excluded from host camera binding, avoiding recursive underwater effects.

## Scheduling and readiness

- Native preparation is queued before `Task.Run`; one active preparation bounds
  its largest transient allocations. Native instructions cannot be interrupted
  at arbitrary points by a managed cancellation token.
- `UnityFrameBudget` is shared across installation collaborators. It yields
  between operations, not halfway through mesh upload or collider cooking.
- First focus and subsequent terrain transitions use the same incremental path.
  One terrain grid export at a time runs on a worker across all islands, separate
  from the generation queue. Uploads reuse the configured installation budget.
  Coarse/previous terrain stays visible until a complete replacement is ready;
  the critical player collider is still created immediately. No whole-frame
  time limit is guaranteed by a cooperative budget.
- Existing terrain profiler markers cover player focus, LOD0/LOD1, colliders,
  forest LODs, reeds and ferns. Cave export/copy/upload/cook have separate markers.
- An active runtime means terrain installation completed. Collider residency
  follows streaming targets. Optional navigation readiness comes from
  `IIslandNavigation.BuildCompletion` and `IsReady`, independently of visual LOD.
- Full LOD0 navigation does not automatically provide distant NPC physics
  colliders. Logs retain LOD0 box colliders; boulders retain LOD0 sphere colliders.

## Developer entry points

Use package README files for installation and host code. Use the root plan and
`IMPLEMENTATION.md` for current work status. Keep the September 8 cleanup report,
Rust performance plan and dated feature validations as evidence of their own
revisions; do not reapply superseded namespace, clone or shader-path migrations.

## Chunked navigation

The optional navigation package builds 128-metre chunks sequentially by default,
with one worker inside a bake. It retains the full LOD0 geometry and height mesh.
Short validated NavMesh links join chunk borders; hosts must enable automatic
link traversal or implement it. Readiness covers all chunks and connections.
`IslandNavigation.Data` is now the first chunk, not the entire island. Unload and
dormancy remove all owned links and data instances. Smaller chunks constrain
transient bake inputs; they do not reduce all resident vegetation/native data.
See [NAVIGATION_CHUNKS.md](NAVIGATION_CHUNKS.md) for measured bounds and the remaining
standard long-distance path-query limitation.

## Forest detail residency

Only overview forest meshes and collider records are prepared for an entire
island. LOD1 requests cover an 8 by 8 group of owner tiles; LOD0 requests cover a
single owner tile. One forest export worker runs across islands, independently
of the terrain worker and generation queue. It leases native ownership through
export and copying. Native buffers are freed per leaf export, and managed arrays
are released as tiles upload; there is no persistent managed detail cache.

Forest owners use half-open leaf bounds, except the final island edge includes
1.0. The native inclusive-bound API receives the previous representable float
as interior upper bounds. Whole trees/clusters, vertex order, channels, materials,
wind weights and end-grain UVs remain unchanged. Retired details restore their
coarse owner before replacement allocation. Pending groups remain hidden until
complete; cancellation/unload removes pending meshes and cancels worker requests.

The installation budget applies between forest tile uploads. Individual meshes,
LOD0 collider groups and initial overview installation are still atomic. This
bounds duplicate detail buffers, not total native, GPU, process or world memory.
