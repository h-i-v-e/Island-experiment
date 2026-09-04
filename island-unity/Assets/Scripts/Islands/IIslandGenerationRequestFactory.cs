using UnityEngine;

public interface IIslandGenerationRequestFactory
{
    // This is the sole authority for island existence and configuration.
    // Return null when this grid location should remain open sea.
    IslandGenerationRequest CreateIslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition);
}
