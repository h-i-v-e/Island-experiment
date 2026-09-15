using UnityEngine;

namespace Motu.Streaming
{
    internal static class TileNeighborhood
    {
        internal static bool Contains(Vector2Int tile, Vector2Int center, int radius, int resolution) =>
            tile.x >= 0 && tile.y >= 0 && tile.x < resolution && tile.y < resolution
            && Mathf.Abs(tile.x - center.x) <= radius && Mathf.Abs(tile.y - center.y) <= radius;
    }
}
