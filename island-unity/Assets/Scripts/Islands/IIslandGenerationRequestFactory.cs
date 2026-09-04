using UnityEngine;

public interface IIslandGenerationRequestFactory
{
    // Return null when this grid location should remain open sea.
    IslandGenerationRequest CreateIslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition);
}
