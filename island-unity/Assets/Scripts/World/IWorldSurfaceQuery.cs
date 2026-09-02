using UnityEngine;

public interface IWorldSurfaceQuery
{
    void SetStreamingTarget(Transform target);
    void PrepareStreamingAt(Vector3 worldPosition);
    bool TrySnapToTerrain(Vector3 approximateWorldPoint, out Vector3 worldPoint);
    float GetTerrainOrSeaHeight(Vector3 approximateWorldPoint);
    void SetFirstPersonViewActive(bool active);
}
