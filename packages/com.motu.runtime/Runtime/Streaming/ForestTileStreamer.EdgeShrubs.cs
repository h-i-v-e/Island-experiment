using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Motu.Islands;
using Motu.Rendering;
using Motu.World;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    internal sealed partial class ForestTileStreamer
    {
        private const float EdgeShrubTileSize = 32f;
        private const float EdgeShrubCullHeight = .1f;

        private readonly List<StreamedMeshTile> edgeShrubTiles = new List<StreamedMeshTile>();
        private GameObject edgeShrubRoot;
        private Material edgeShrubMaterial;
        private SharedNoiseTexture edgeShrubNoise;

        internal async Task InstallEdgeShrubsAsync(IslandPreparedForestShrub[] shrubs, UnityFrameBudget frameBudget,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (shrubs.Length == 0) return;

            var groups = new Dictionary<Vector2Int, List<IslandPreparedForestShrub>>();
            foreach (var shrub in shrubs)
            {
                var key = new Vector2Int(Mathf.FloorToInt(shrub.position.x / EdgeShrubTileSize),
                    Mathf.FloorToInt(shrub.position.z / EdgeShrubTileSize));
                if (!groups.TryGetValue(key, out var group))
                {
                    group = new List<IslandPreparedForestShrub>();
                    groups.Add(key, group);
                }
                group.Add(shrub);
            }

            try
            {
                edgeShrubRoot = new GameObject("Forest edge shrubs") { layer = forestRoot.layer };
                edgeShrubRoot.transform.SetParent(forestRoot.transform, false);
                edgeShrubNoise = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.Cliff);
                edgeShrubMaterial = BushMeshBuilder.CreateMaterial("Forest edge shrubs", edgeShrubNoise.Texture);
                foreach (var pair in groups)
                {
                    InstallEdgeShrubTile(pair.Key, pair.Value);
                    await frameBudget.YieldIfExceededAsync(cancellationToken);
                }
            }
            catch
            {
                DisposeEdgeShrubs();
                throw;
            }
        }

        private void InstallEdgeShrubTile(Vector2Int key, List<IslandPreparedForestShrub> shrubs)
        {
            var origin = new Vector3((key.x + .5f) * EdgeShrubTileSize, shrubs[0].position.y,
                (key.y + .5f) * EdgeShrubTileSize);
            var builder = new BushMeshBuilder();
            foreach (var shrub in shrubs)
            {
                // Swapping the native vertical axis into Unity reflects the native yaw.
                builder.AddBush(shrub.position - origin, -shrub.yawRadians * Mathf.Rad2Deg, shrub.scale);
            }

            var name = $"Forest edge shrub tile {key.x},{key.y}";
            var mesh = builder.CreateMesh(name);
            var tile = StreamedMeshTile.Create(mesh, edgeShrubRoot.transform, edgeShrubMaterial, name, origin);
            edgeShrubTiles.Add(tile);

            var branches = builder.CreateBranchMesh("Forest edge shrub branches");
            var branchTile = StreamedMeshTile.Create(branches, tile.Renderer.transform, edgeShrubMaterial, branches.name);
            edgeShrubTiles.Add(branchTile);

            var group = tile.Renderer.gameObject.AddComponent<LODGroup>();
            group.SetLODs(new[]
            {
                new LOD(BushMeshBuilder.BranchLodHeight, new Renderer[] { tile.Renderer, branchTile.Renderer }),
                new LOD(EdgeShrubCullHeight, new Renderer[] { tile.Renderer })
            });
            group.localReferencePoint = mesh.bounds.center;
            // Sparse tiles and dense tiles use the same camera LOD distance.
            group.size = EdgeShrubTileSize;
            mesh.UploadMeshData(true);
            branches.UploadMeshData(true);
        }

        private void DisposeEdgeShrubs()
        {
            // Release the branch children before their tile parents.
            for (var i = edgeShrubTiles.Count - 1; i >= 0; --i) edgeShrubTiles[i].Dispose();
            edgeShrubTiles.Clear();
            DestroyUnityObject(edgeShrubRoot);
            edgeShrubRoot = null;
            DestroyUnityObject(edgeShrubMaterial);
            edgeShrubMaterial = null;
            edgeShrubNoise?.Dispose();
            edgeShrubNoise = null;
        }
    }
}
