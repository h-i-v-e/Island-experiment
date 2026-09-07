using UnityEngine;

namespace Motu.Islands
{
    public interface IIslandGenerationRequestFactory
    {
        // This is the sole authority for island existence, seed, settings, and
        // material palette selection.
        // Return null when this grid location should remain open sea.
        // Vector2Int X/Y correspond to world X/Z respectively.
        IslandGenerationRequest CreateIslandGenerationRequest(
            Vector2Int islandGridPosition);

        // This query must be deterministic, side-effect free, and substantially
        // cheaper than constructing a generation request. It is used for broad
        // world previews such as the minimap.
        bool HasIsland(Vector2Int islandGridPosition);
    }
}
