using static Motu.Rendering.UnityObjectLifetime;
using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Interop;
using Motu.Islands;

namespace Motu.Streaming
{
    // Fern rosettes are complete owner units in the same finest 64x64 grid as
    // terrain LOD0. At most the player's 3x3 neighborhood is uploaded.
    internal sealed class FernTileStreamer : IDisposable
    {
        internal const int Resolution = 64;
        internal const int TileCount = Resolution * Resolution;
        private const int NearbyRadius = 1;

        private readonly Dictionary<Vector2Int, StreamedMeshTile> active =
            new Dictionary<Vector2Int, StreamedMeshTile>();
        private readonly List<Vector2Int> removal = new List<Vector2Int>(9);
        private IslandPreparedMesh[] prepared;
        private Material material;
        private GameObject root;

        internal int ActiveTileCount => active.Count;

        internal void Initialize(
            Transform parent,
            Material sharedMaterial,
            IslandPreparedMesh[] preparedTiles,
            bool showFerns)
        {
            if (parent == null) throw new ArgumentNullException(nameof(parent));
            material = sharedMaterial
                ?? throw new ArgumentNullException(nameof(sharedMaterial));
            if (preparedTiles == null || preparedTiles.Length != TileCount)
            {
                throw new ArgumentException(
                    $"The fern owner grid must contain {TileCount} tiles.",
                    nameof(preparedTiles));
            }
            prepared = preparedTiles;
            root = new GameObject("Tree Trunk Ferns (LOD 0 Only)");
            root.transform.SetParent(parent, false);
            root.SetActive(showFerns);
        }

        internal void SetVisible(bool value)
        {
            root?.SetActive(value);
        }

        internal void UpdateLod0Neighborhood(Vector2Int center)
        {
            if (root == null) return;
            for (var y = center.y - NearbyRadius; y <= center.y + NearbyRadius; y++)
            {
                for (var x = center.x - NearbyRadius; x <= center.x + NearbyRadius; x++)
                {
                    if (x < 0 || y < 0 || x >= Resolution || y >= Resolution) continue;
                    var key = new Vector2Int(x, y);
                    if (!active.ContainsKey(key))
                    {
                        var tile = CreateTile(key);
                        if (tile != null) active.Add(key, tile);
                    }
                }
            }

            removal.Clear();
            foreach (var key in active.Keys)
            {
                if (!TileNeighborhood.Contains(key, center, NearbyRadius, Resolution)) removal.Add(key);
            }
            foreach (var key in removal)
            {
                active[key].Dispose();
                active.Remove(key);
            }
        }

        internal IEnumerator UpdateLod0NeighborhoodIncremental(
            Vector2Int center,
            Func<bool> stillWanted)
        {
            if (root == null) yield break;
            for (var y = center.y - NearbyRadius; y <= center.y + NearbyRadius; y++)
            {
                for (var x = center.x - NearbyRadius; x <= center.x + NearbyRadius; x++)
                {
                    if (x < 0 || y < 0 || x >= Resolution || y >= Resolution) continue;
                    var key = new Vector2Int(x, y);
                    if (!active.ContainsKey(key))
                    {
                        var tile = CreateTile(key);
                        if (tile != null) active.Add(key, tile);
                        yield return null;
                        if (!stillWanted()) yield break;
                    }
                }
            }

            removal.Clear();
            foreach (var key in active.Keys)
            {
                if (!TileNeighborhood.Contains(key, center, NearbyRadius, Resolution)) removal.Add(key);
            }
            foreach (var key in removal)
            {
                active[key].Dispose();
                active.Remove(key);
                yield return null;
                if (!stillWanted()) yield break;
            }
        }

        internal void ClearPlayerFocus()
        {
            foreach (var tile in active.Values) tile.Dispose();
            active.Clear();
        }

        private StreamedMeshTile CreateTile(Vector2Int key)
        {
            var source = prepared[key.y * Resolution + key.x];
            if (source == null || source.triangles.Length == 0) return null;
            return StreamedMeshTile.Create(source, root.transform, material, $"Tree trunk ferns {key.x},{key.y}");
        }

        public void Dispose()
        {
            ClearPlayerFocus();
            DestroyUnityObject(root);
            root = null;
            prepared = null;
            material = null;
        }
    }
}
