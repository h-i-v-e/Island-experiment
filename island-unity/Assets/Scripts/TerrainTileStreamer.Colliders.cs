using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using Unity.Profiling;

public sealed partial class TerrainTileStreamer
{
    private void UpdateColliderNeighborhood(Vector2Int center)
    {
        var created = new List<KeyValuePair<Vector2Int, ColliderTile>>(9);
        try
        {
            var minimumX = Mathf.Max(center.x - NearbyRadius, 0);
            var maximumX = Mathf.Min(center.x + NearbyRadius, Lod1Resolution - 1);
            var minimumY = Mathf.Max(center.y - NearbyRadius, 0);
            var maximumY = Mathf.Min(center.y + NearbyRadius, Lod1Resolution - 1);
            for (var y = minimumY; y <= maximumY; y++)
            {
                for (var x = minimumX; x <= maximumX; x++)
                {
                    var key = new Vector2Int(x, y);
                    if (!colliderTiles.ContainsKey(key))
                    {
                        created.Add(new KeyValuePair<Vector2Int, ColliderTile>(
                            key,
                            CreateColliderTile(key)));
                    }
                }
            }
        }
        catch
        {
            foreach (var entry in created)
            {
                DestroyColliderTile(entry.Value);
            }
            throw;
        }

        // Install the complete incoming set before retiring any old collider.
        foreach (var entry in created)
        {
            colliderTiles.Add(entry.Key, entry.Value);
        }

        removalScratch.Clear();
        foreach (var key in colliderTiles.Keys)
        {
            if (Mathf.Abs(key.x - center.x) > NearbyRadius
                || Mathf.Abs(key.y - center.y) > NearbyRadius)
            {
                removalScratch.Add(key);
            }
        }
        foreach (var key in removalScratch)
        {
            var tile = colliderTiles[key];
            tile.collider.enabled = false;
            colliderTiles.Remove(key);
            DestroyColliderTile(tile);
        }
    }

    private void EnsureCriticalCollider(Vector2Int center)
    {
        if (colliderTiles.ContainsKey(center))
        {
            return;
        }
        // An adjacent cell is already inside the previous 3x3 neighbourhood.
        // This fallback is only expected after a teleport or movement faster
        // than the transition queue and guarantees the player never enters a
        // cell without collision while the remaining tiles stream in.
        using (ColliderTransitionMarker.Auto())
        {
            colliderTiles.Add(center, CreateColliderTile(center));
        }
    }

    private IEnumerator UpdateColliderNeighborhoodIncremental(Vector2Int center)
    {
        var desired = NeighbourKeys(center, Lod1Resolution);
        foreach (var key in desired)
        {
            if (!colliderTiles.ContainsKey(key))
            {
                using (ColliderTransitionMarker.Auto())
                {
                    colliderTiles.Add(key, CreateColliderTile(key));
                }
                yield return null;
                if (center != requestedLod1)
                {
                    yield break;
                }
            }
        }

        var desiredSet = new HashSet<Vector2Int>(desired);
        removalScratch.Clear();
        foreach (var key in colliderTiles.Keys)
        {
            if (!desiredSet.Contains(key))
            {
                removalScratch.Add(key);
            }
        }
        foreach (var key in removalScratch)
        {
            var tile = colliderTiles[key];
            tile.collider.enabled = false;
            colliderTiles.Remove(key);
            DestroyColliderTile(tile);
            yield return null;
            if (center != requestedLod1)
            {
                yield break;
            }
        }
    }

    private ColliderTile CreateColliderTile(Vector2Int key)
    {
        var terrainData = new TerrainData
        {
            name = $"LOD 1 collision heightfield {key.x},{key.y}",
            heightmapResolution = ColliderSamplesPerTile,
        };
        GameObject tileObject = null;
        try
        {
            if (terrainData.heightmapResolution != ColliderSamplesPerTile)
            {
                throw new InvalidOperationException(
                    $"Unity rejected terrain heightmap resolution {ColliderSamplesPerTile}.");
            }

            var tileSize = worldSize / Lod1Resolution;
            terrainData.size = new Vector3(
                tileSize,
                colliderHeightMap.verticalSize,
                tileSize);
            terrainData.SetHeights(0, 0, colliderHeightMap.CopyTileHeights(key));

            tileObject = new GameObject($"LOD 1 terrain collider {key.x},{key.y}");
            tileObject.transform.SetParent(colliderRoot.transform, false);
            tileObject.transform.localPosition = new Vector3(
                -worldSize * 0.5f + key.x * tileSize,
                colliderHeightMap.verticalOrigin,
                -worldSize * 0.5f + key.y * tileSize);

            var hiddenTerrain = tileObject.AddComponent<Terrain>();
            hiddenTerrain.terrainData = terrainData;
            hiddenTerrain.allowAutoConnect = false;
            hiddenTerrain.drawHeightmap = false;
            hiddenTerrain.drawTreesAndFoliage = false;
            hiddenTerrain.enabled = false;

            var terrainCollider = tileObject.AddComponent<TerrainCollider>();
            terrainCollider.terrainData = terrainData;
            terrainCollider.enabled = true;
            return new ColliderTile(tileObject, terrainData, terrainCollider);
        }
        catch
        {
            DestroyUnityObject(tileObject);
            DestroyUnityObject(terrainData);
            throw;
        }
    }

    private void RemoveAllColliderTiles()
    {
        foreach (var tile in colliderTiles.Values)
        {
            tile.collider.enabled = false;
            DestroyColliderTile(tile);
        }
        colliderTiles.Clear();
    }

    private static void DestroyColliderTile(ColliderTile tile)
    {
        if (tile == null)
        {
            return;
        }
        DestroyUnityObject(tile.gameObject);
        DestroyUnityObject(tile.terrainData);
    }

}
