using Motu.Interop;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        /// <summary>
        /// Read the highest final terrain triangle along world vertical without
        /// creating colliders, changing focus, loading terrain or approximating holes.
        /// Geometric normals include the island transform and nonuniform scaling.
        /// </summary>
        public bool TryGetAccurateTerrainSurface(Vector3 worldPoint, out float height, out Vector3 normal)
        {
            height = 0;
            normal = Vector3.up;
            if (islandRuntime == null || islandRuntime.State != IslandRuntimeState.Active
                && islandRuntime.State != IslandRuntimeState.Dormant) return false;
            return TerrainSurfaceNative.TrySample(islandRuntime.NativeHandle, islandRuntime.transform,
                islandRuntime.WorldSizeMetres, worldPoint, out height, out normal);
        }
    }
}
