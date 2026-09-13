using Motu.Streaming;
using static Motu.Streaming.TerrainTileStreamer;
using System;
using UnityEngine;
using Motu.Islands;

namespace Motu.Editor
{
    internal static class TerrainStreamingValidation
    {
        internal static void ValidateTerrainRenderBatching(Material material, int unusedVertices = 0)
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
                    var mesh = CreateBatchValidationMesh(index * 2f, unusedVertices);
                    tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                    tiles[index] = new Tile(tileObject, mesh);
                }
                group = new TileGroup(root, tiles, 0);
                streamer.ConfigureTerrainBatch(group, 1);
                ValidateBatchTriangles(group);

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
                ValidateBatchTriangles(group);
                if (group.batchMesh.GetIndexCount(0) != 3
                    || !group.batchObject.activeSelf)
                {
                    throw new InvalidOperationException(
                        "A refined terrain tile was not removed from its parent render batch.");
                }

                SetBatchedTileActive(group, 1, false);
                RebuildTerrainBatchIfDirty(group);
                ValidateBatchTriangles(group);
                if (group.batchMesh.GetIndexCount(0) != 0
                    || group.batchObject.activeSelf)
                {
                    throw new InvalidOperationException(
                        "An empty terrain render batch remained active.");
                }

                SetBatchedTileActive(group, 0, true);
                SetBatchedTileActive(group, 1, true);
                RebuildTerrainBatchIfDirty(group);
                ValidateBatchTriangles(group);
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

        private static void ValidateBatchTriangles(TileGroup group)
        {
            var combinedVertices = group.batchMesh.vertices;
            var combinedIndices = group.batchMesh.GetIndices(0);
            var combinedColors = group.batchMesh.colors;
            var combinedEnvironment = group.batchMesh.uv2;
            var cursor = 0;
            foreach (var tile in group.tiles)
            {
                if (tile == null || !tile.gameObject.activeSelf) continue;
                var vertices = tile.mesh.vertices;
                var colors = tile.mesh.colors;
                var environment = tile.mesh.uv2;
                foreach (var sourceIndex in tile.mesh.GetIndices(0))
                {
                    NUnit.Framework.Assert.That(cursor, NUnit.Framework.Is.LessThan(combinedIndices.Length));
                    var index = combinedIndices[cursor++];
                    NUnit.Framework.Assert.That(index, NUnit.Framework.Is.InRange(0, combinedVertices.Length - 1));
                    NUnit.Framework.Assert.That(combinedVertices[index], NUnit.Framework.Is.EqualTo(vertices[sourceIndex]));
                    NUnit.Framework.Assert.That(combinedColors[index], NUnit.Framework.Is.EqualTo(colors[sourceIndex]));
                    NUnit.Framework.Assert.That(combinedEnvironment[index], NUnit.Framework.Is.EqualTo(environment[sourceIndex]));
                }
            }
            NUnit.Framework.Assert.That(cursor, NUnit.Framework.Is.EqualTo(combinedIndices.Length));
        }

        private static Mesh CreateBatchValidationMesh(float xOffset, int unusedVertices)
        {
            var offset = unusedVertices / 2;
            var vertices = new Vector3[unusedVertices + 3];
            var normals = new Vector3[vertices.Length];
            var uv = new Vector2[vertices.Length];
            var environment = new Vector2[vertices.Length];
            var colors = new Color[vertices.Length];
            vertices[offset] = new Vector3(xOffset, 0f, 0f);
            vertices[offset + 1] = new Vector3(xOffset + 1f, 0f, 0f);
            vertices[offset + 2] = new Vector3(xOffset, 0f, 1f);
            normals[offset] = normals[offset + 1] = normals[offset + 2] = Vector3.up;
            uv[offset + 1] = Vector2.right;
            uv[offset + 2] = Vector2.up;
            environment[offset + 1] = Vector2.one;
            environment[offset + 2] = Vector2.right;
            colors[offset] = Color.red;
            colors[offset + 1] = Color.green;
            colors[offset + 2] = Color.blue;
            var mesh = new Mesh
            {
                name = "Terrain render batching validation mesh",
                indexFormat = vertices.Length > ushort.MaxValue
                    ? UnityEngine.Rendering.IndexFormat.UInt32 : UnityEngine.Rendering.IndexFormat.UInt16,
                vertices = vertices,
                normals = normals,
                uv = uv,
                uv2 = environment,
                colors = colors,
                triangles = new[] { offset, offset + 1, offset + 2 },
            };
            mesh.RecalculateBounds();
            mesh.UploadMeshData(false);
            return mesh;
        }

        internal static void ValidateColliderStreaming(
            TerrainTileStreamer streamer,
            IslandPreparedColliderHeightMap preparedHeightMap,
            float terrainWorldSize)
        {
            streamer.colliderHeightMap = preparedHeightMap;
            streamer.worldSize = terrainWorldSize;
            streamer.colliderRoot = new GameObject("Terrain collider streaming validation");
            streamer.colliderRoot.transform.SetParent(streamer.transform, false);
            try
            {
                var firstCenter = new Vector2Int(31, 32);
                streamer.UpdateColliderNeighborhood(firstCenter);
                if (streamer.colliderTiles.Count != 9
                    || !streamer.colliderTiles.TryGetValue(firstCenter, out var retainedCenter))
                {
                    throw new InvalidOperationException(
                        "The initial terrain-collider neighbourhood is incomplete.");
                }

                var nextCenter = new Vector2Int(32, 32);
                streamer.requestedLod1 = nextCenter;
                var transition = streamer.UpdateColliderNeighborhoodIncremental(nextCenter);
                while (transition.MoveNext())
                {
                    // Editor validation drains the same incremental iterator that
                    // play mode advances one item per frame.
                }
                if (streamer.colliderTiles.Count != 9
                    || !streamer.colliderTiles.ContainsKey(nextCenter)
                    || !streamer.colliderTiles.TryGetValue(firstCenter, out var sharedTile)
                    || !ReferenceEquals(retainedCenter, sharedTile))
                {
                    throw new InvalidOperationException(
                        "Terrain-collider transition coverage or tile reuse is invalid.");
                }

                Physics.SyncTransforms();
                var tileSize = streamer.worldSize / Lod1Resolution;
                var point = new Vector3(
                    -streamer.worldSize * 0.5f + (nextCenter.x + 0.5f) * tileSize,
                    0f,
                    -streamer.worldSize * 0.5f + (nextCenter.y + 0.5f) * tileSize);
                if (!streamer.TrySnapToCurrentCollider(point, out var hit))
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
                streamer.RemoveAllColliderTiles();
                DestroyUnityObject(streamer.colliderRoot);
                streamer.colliderRoot = null;
                streamer.colliderHeightMap = null;
            }
        }
    }
}
