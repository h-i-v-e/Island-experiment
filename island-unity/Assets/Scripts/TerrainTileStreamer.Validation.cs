using System;
using UnityEngine;

public sealed partial class TerrainTileStreamer
{
#if UNITY_EDITOR
    internal static void ValidateTerrainRenderBatching(Material material)
    {
        if (material == null)
        {
            throw new ArgumentNullException(nameof(material));
        }

        var host = new GameObject("Terrain render batching validation host");
        TileGroup group = null;
        try
        {
            var streamer = host.AddComponent<TerrainTileStreamer>();
            streamer.terrainMaterial = material;
            streamer.terrainLod1Material = material;
            streamer.terrainLod2Material = material;
            var root = new GameObject("Terrain render batching validation group");
            root.transform.SetParent(host.transform, false);
            var tiles = new Tile[2];
            for (var index = 0; index < tiles.Length; index++)
            {
                var tileObject = new GameObject($"Terrain validation tile {index}");
                tileObject.transform.SetParent(root.transform, false);
                var mesh = CreateBatchValidationMesh(index * 2f);
                tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                tiles[index] = new Tile(tileObject, mesh);
            }
            group = new TileGroup(root, tiles, 0);
            streamer.ConfigureTerrainBatch(group, 1);

            var renderers = root.GetComponentsInChildren<MeshRenderer>(true);
            if (renderers.Length != 1
                || renderers[0].sharedMaterial != material
                || group.batchMesh == null
                || group.batchMesh.vertexCount != 6
                || group.batchMesh.GetIndexCount(0) != 6
                || group.batchMesh.colors.Length != 6
                || group.batchMesh.uv2.Length != 6)
            {
                throw new InvalidOperationException(
                    "Terrain tiles were not merged into one attribute-complete render batch.");
            }
            if (tiles[0].edgeMesh != null || tiles[1].edgeMesh != null)
            {
                throw new InvalidOperationException(
                    "Terrain debug edge meshes were created before wireframe rendering requested them.");
            }

            streamer.meshEdgeMaterial = material;
            streamer.DrawGroupEdges(group);
            if (tiles[0].edgeMesh == null || tiles[1].edgeMesh == null)
            {
                throw new InvalidOperationException(
                    "Terrain debug edge meshes were not created lazily when requested.");
            }

            SetBatchedTileActive(group, 0, false);
            RebuildTerrainBatchIfDirty(group);
            if (group.batchMesh.GetIndexCount(0) != 3
                || !group.batchObject.activeSelf)
            {
                throw new InvalidOperationException(
                    "A refined terrain tile was not removed from its parent render batch.");
            }

            SetBatchedTileActive(group, 1, false);
            RebuildTerrainBatchIfDirty(group);
            if (group.batchMesh.GetIndexCount(0) != 0
                || group.batchObject.activeSelf)
            {
                throw new InvalidOperationException(
                    "An empty terrain render batch remained active.");
            }

            SetBatchedTileActive(group, 0, true);
            SetBatchedTileActive(group, 1, true);
            RebuildTerrainBatchIfDirty(group);
            if (group.batchMesh.GetIndexCount(0) != 6
                || !group.batchObject.activeSelf)
            {
                throw new InvalidOperationException(
                    "Terrain tiles were not restored to their parent render batch.");
            }
        }
        finally
        {
            DestroyGroup(group);
            DestroyUnityObject(host);
        }
    }

    private static Mesh CreateBatchValidationMesh(float xOffset)
    {
        var mesh = new Mesh
        {
            name = "Terrain render batching validation mesh",
            vertices = new[]
            {
                new Vector3(xOffset, 0f, 0f),
                new Vector3(xOffset + 1f, 0f, 0f),
                new Vector3(xOffset, 0f, 1f),
            },
            normals = new[] { Vector3.up, Vector3.up, Vector3.up },
            uv = new[] { Vector2.zero, Vector2.right, Vector2.up },
            uv2 = new[] { Vector2.zero, Vector2.one, Vector2.right },
            colors = new[] { Color.red, Color.green, Color.blue },
            triangles = new[] { 0, 1, 2 },
        };
        mesh.RecalculateBounds();
        mesh.UploadMeshData(false);
        return mesh;
    }

    internal void ValidateColliderStreaming(
        IslandPreparedColliderHeightMap preparedHeightMap,
        float terrainWorldSize)
    {
        colliderHeightMap = preparedHeightMap;
        worldSize = terrainWorldSize;
        colliderRoot = new GameObject("Terrain collider streaming validation");
        colliderRoot.transform.SetParent(transform, false);
        try
        {
            var firstCenter = new Vector2Int(31, 32);
            UpdateColliderNeighborhood(firstCenter);
            if (colliderTiles.Count != 9
                || !colliderTiles.TryGetValue(firstCenter, out var retainedCenter))
            {
                throw new InvalidOperationException(
                    "The initial terrain-collider neighbourhood is incomplete.");
            }

            var nextCenter = new Vector2Int(32, 32);
            requestedLod1 = nextCenter;
            var transition = UpdateColliderNeighborhoodIncremental(nextCenter);
            while (transition.MoveNext())
            {
                // Editor validation drains the same incremental iterator that
                // play mode advances one item per frame.
            }
            if (colliderTiles.Count != 9
                || !colliderTiles.ContainsKey(nextCenter)
                || !colliderTiles.TryGetValue(firstCenter, out var sharedTile)
                || !ReferenceEquals(retainedCenter, sharedTile))
            {
                throw new InvalidOperationException(
                    "Terrain-collider transition coverage or tile reuse is invalid.");
            }

            Physics.SyncTransforms();
            var tileSize = worldSize / Lod1Resolution;
            var point = new Vector3(
                -worldSize * 0.5f + (nextCenter.x + 0.5f) * tileSize,
                0f,
                -worldSize * 0.5f + (nextCenter.y + 0.5f) * tileSize);
            if (!TrySnapToCurrentCollider(point, out var hit))
            {
                throw new InvalidOperationException(
                    "The transitioned terrain-collider neighbourhood cannot be raycast.");
            }
            var intervals = preparedHeightMap.samplesPerTile - 1;
            var expectedHeight = preparedHeightMap.WorldHeightAt(
                nextCenter.x * intervals + intervals / 2,
                nextCenter.y * intervals + intervals / 2);
            if (Mathf.Abs(hit.y - expectedHeight) > 0.02f)
            {
                throw new InvalidOperationException(
                    $"Terrain collider height {hit.y:F3} does not match "
                    + $"the source height {expectedHeight:F3}.");
            }
        }
        finally
        {
            RemoveAllColliderTiles();
            DestroyUnityObject(colliderRoot);
            colliderRoot = null;
            colliderHeightMap = null;
        }
    }
#endif
}
