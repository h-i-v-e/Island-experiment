using static Motu.Rendering.UnityObjectLifetime;
using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Islands;

namespace Motu.Streaming
{
    // Only the terrain LOD0 3x3 owner grid holds draw batches. All clumps share
    // one mesh and no placement creates a GameObject, Mesh, or Material.
    internal sealed class ReedTileStreamer : IDisposable
    {
        internal const int Resolution = 64;
        internal const int TileCount = Resolution * Resolution;
        private const int NearbyRadius = 1;
        private static readonly int WindMaterialId = Shader.PropertyToID("_MotuWindMaterial");
        private static readonly int WeatherWindId = Shader.PropertyToID("_MotuWeatherWind");
        private readonly Dictionary<Vector2Int, ReedClumpBatch[]> active =
            new Dictionary<Vector2Int, ReedClumpBatch[]>();
        private readonly List<Vector2Int> removal = new List<Vector2Int>(9);
        private IslandPreparedReedTile[] prepared;
        private Material material;
        private Mesh sharedMesh;
        private GameObject root;
        private Matrix4x4 previousMatrix;
        private bool transformDirty;

        internal int ActiveTileCount => active.Count;
        internal int ActiveInstanceCount { get; private set; }
        internal int SubmittedBatchCount { get; private set; }
        internal int SharedVertexCount => sharedMesh != null ? sharedMesh.vertexCount : 0;
        internal Mesh SharedMesh => sharedMesh;

        internal void Initialize(Transform parent, Material sharedMaterial,
            IslandPreparedReedTile[] preparedTiles, bool showReeds)
        {
            if (root != null) throw new InvalidOperationException("Reeds are already initialized.");
            if (parent == null) throw new ArgumentNullException(nameof(parent));
            if (sharedMaterial == null) throw new ArgumentNullException(nameof(sharedMaterial));
            if (preparedTiles == null || preparedTiles.Length != TileCount)
                throw new ArgumentException($"The reed owner grid must contain {TileCount} tiles.", nameof(preparedTiles));
            material = sharedMaterial;
            material.enableInstancing = true;
            material.EnableKeyword("MOTU_REED_CLUMPS");
            prepared = preparedTiles;
            sharedMesh = ReedClumpMesh.Acquire();
            root = new GameObject("Riverbank Reeds and Rushes (LOD 0 Only)") { layer = parent.gameObject.layer };
            root.transform.SetParent(parent, false);
            root.SetActive(showReeds);
            transformDirty = true;
            RenderPipelineManager.beginCameraRendering += RenderCamera;
        }

        internal void SetVisible(bool value) => root?.SetActive(value);

        internal void UpdateLod0Neighborhood(Vector2Int center)
        {
            if (root == null) return;
            for (var y = center.y - NearbyRadius; y <= center.y + NearbyRadius; ++y)
                for (var x = center.x - NearbyRadius; x <= center.x + NearbyRadius; ++x)
                    AddTile(new Vector2Int(x, y));
            FindRemovals(center);
            foreach (var key in removal) RemoveTile(key);
        }

        internal IEnumerator UpdateLod0NeighborhoodIncremental(Vector2Int center, Func<bool> stillWanted)
        {
            if (root == null) yield break;
            for (var y = center.y - NearbyRadius; y <= center.y + NearbyRadius; ++y)
                for (var x = center.x - NearbyRadius; x <= center.x + NearbyRadius; ++x)
                {
                    if (!AddTile(new Vector2Int(x, y))) continue;
                    yield return null;
                    if (!stillWanted()) yield break;
                }
            FindRemovals(center);
            foreach (var key in removal)
            {
                RemoveTile(key);
                yield return null;
                if (!stillWanted()) yield break;
            }
        }

        internal void ClearPlayerFocus()
        {
            active.Clear();
            ActiveInstanceCount = SubmittedBatchCount = 0;
        }

        private bool AddTile(Vector2Int key)
        {
            if (key.x < 0 || key.y < 0 || key.x >= Resolution || key.y >= Resolution || active.ContainsKey(key)) return false;
            var placements = prepared[key.y * Resolution + key.x]?.placements;
            if (placements == null || placements.Length == 0) return false;
            var batches = new ReedClumpBatch[(placements.Length + ReedClumpBatch.Limit - 1) / ReedClumpBatch.Limit];
            for (var i = 0; i < batches.Length; ++i)
            {
                var offset = i * ReedClumpBatch.Limit;
                batches[i] = new ReedClumpBatch(placements, offset, Math.Min(ReedClumpBatch.Limit, placements.Length - offset));
            }
            active.Add(key, batches);
            ActiveInstanceCount += placements.Length;
            transformDirty = true;
            return true;
        }

        private void FindRemovals(Vector2Int center)
        {
            removal.Clear();
            foreach (var key in active.Keys)
                if (!TileNeighborhood.Contains(key, center, NearbyRadius, Resolution)) removal.Add(key);
        }

        private void RemoveTile(Vector2Int key)
        {
            foreach (var batch in active[key]) ActiveInstanceCount -= batch.Count;
            active.Remove(key);
        }

        private void RenderCamera(ScriptableRenderContext context, Camera camera) => Submit(camera);

        internal void Submit(Camera camera)
        {
            SubmittedBatchCount = 0;
            if (camera == null || root == null || !root.activeInHierarchy
                || (camera.cullingMask & (1 << root.layer)) == 0) return;
            var matrix = root.transform.localToWorldMatrix;
            if (transformDirty || previousMatrix != matrix)
            {
                foreach (var tile in active.Values)
                    foreach (var batch in tile) batch.SetTransform(matrix);
                previousMatrix = matrix;
                transformDirty = false;
            }
            var windPadding = Mathf.Max(2f, Mathf.Max(0, Shader.GetGlobalVector(WindMaterialId).x)
                * Mathf.Max(0, Shader.GetGlobalVector(WeatherWindId).w));
            // Unity culls each small batch for the view and shadow maps. Do not
            // remove offscreen casters here: they can shadow the visible bank.
            foreach (var tile in active.Values)
                foreach (var batch in tile)
                {
                    batch.Submit(camera, sharedMesh, material, root.layer, windPadding);
                    ++SubmittedBatchCount;
                }
        }

        public void Dispose()
        {
            RenderPipelineManager.beginCameraRendering -= RenderCamera;
            ClearPlayerFocus();
            if (sharedMesh != null) ReedClumpMesh.Release();
            sharedMesh = null;
            DestroyUnityObject(root);
            root = null;
            prepared = null;
            material = null;
        }
    }
}
