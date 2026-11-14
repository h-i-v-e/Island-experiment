using Motu.Interop;
using UnityEngine;

namespace Motu.World
{
    public sealed partial class IslandWorldManager
    {
        /// <summary>
        /// Query an already resident island's exact highest terrain triangle.
        /// Missing terrain and holes miss; this never generates, loads or moves focus.
        /// </summary>
        public bool TryGetAccurateTerrainSurface(Vector3 worldPoint, out float height, out Vector3 normal)
        {
            height = 0;
            normal = Vector3.up;
            return TerrainSurfaceCoordinates.IsFinite(worldPoint)
                && managedIslands.TryGetValue(WorldToCell(worldPoint), out var entry)
                && entry.Generator != null
                && entry.Generator.TryGetAccurateTerrainSurface(worldPoint, out height, out normal);
        }
    }
}
