# Fallen forest logs

Fallen logs are generated after standing trees, using a separate deterministic seed domain. `fallen_log_density` defaults to 0.18: each standing tree has an 18% chance of requesting one log, with up to four placement attempts. Set it to zero to disable logs. Unity exposes this as **Forest > Fallen Log Density**; regenerate after changing it.

At the standard 2 km island scale, logs are 3–8 m long with radii of 0.18–0.45 m. Placement samples nine points along the log and checks both sides of its footprint. Logs require forest coverage and soil, reject sea, rivers, steep ground, stones, reeds and cave exclusion regions, and clear standing trunks and earlier logs along their entire length. A rigid terrain fit with slight burial rejects hollows that would leave a log floating.

## Geometry and streaming

- Wood LOD0/1/2 uses 10/6/4 sides, with tapered, irregular profiles and broken end rims.
- `ForestMeshes.logs` owns each log's three mesh ranges separately from live trees and foliage clusters. Existing tree placement ordinals and fern/trunk correspondence are unchanged.
- Each owner grid copies a whole log to its centre's tile. Logs crossing a tile edge are neither clipped nor duplicated.
- Logs join the existing wood mesh and material. There is no extra end-cap submesh or material draw.
- Native `CreateForestLogColliders` returns endpoint axes and half widths in the existing `ExportForestTrunkColliders` layout. Release with `ReleaseForestTrunkColliders`.
- Unity creates one oriented box per log with the owning LOD0 group. Hiding forests disables these colliders, and unloading the group destroys them. Higher LODs have no log colliders.

## Shader contract

UV0 retains the encoded bark axis. Vertex colour RGB holds the tree root or log centre in normalized native coordinates. Alpha identifies live wood/foliage (`0.5`), stationary log bark (`0.25`), or stationary end grain (`0`). The shared wind helper leaves both log surfaces still, including shadow rendering.

Wood's exported `environment` channel maps to Unity UV1 (`mesh.uv2`) and stores cap texture coordinates; live-tree and log-side vertices use zero. Foliage retains its existing empty environment stream. End rims have separate vertices so normals and face tags do not interpolate into the bark.

The existing wood shader samples `Assets/Resources/Motu/TreeEndGrain.png` on exposed ends, with configurable **End Grain Tint** and **End Grain Detail**, and per-log rotation/tint variation. This deterministic grayscale growth-ring/crack texture is imported as linear data with mipmaps and trilinear filtering. Bark parallax and bark normals do not affect caps. At distance the grain fades into a stable wood tint; simplified reflections use that tint too.

## Native/cache compatibility

`MotuForestOptions` now contains the trailing `fallenLogDensity` float (32 bytes). Rust, C header, Unity interop, settings and snapshot-key hashing agree. Generated snapshot version is 11 and Unity cache-key schema is 8, so old cached islands cannot hide the new geometry. Legacy generation-parameter files use the default log density when regenerated.

After rebuilding with `./deploy-unity.sh`, restart Unity to load the replaced native library, then regenerate islands. The release build/deployed library must have matching SHA-256 checksums.
