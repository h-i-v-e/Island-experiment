using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using Motu.Islands;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // Visual trees belong to one instanced renderer. Only the player's 3x3
    // finest terrain neighborhood creates trunk and fallen-log collision objects.
    internal sealed class ForestTileStreamer : IDisposable
    {
        internal const int Lod0Resolution = 64;
        internal const int Lod0TileCount = Lod0Resolution * Lod0Resolution;
        private const int NearbyRadius = 1;
        private const int MaximumCollidersPerSlice = 32;

        private sealed class ColliderTile
        {
            internal readonly GameObject root;
            internal int trunks, logs;
            internal ColliderTile(GameObject root) => this.root = root;
        }

        private readonly Dictionary<Vector2Int, ColliderTile> tiles = new Dictionary<Vector2Int, ColliderTile>();
        private readonly HashSet<ColliderTile> pending = new HashSet<ColliderTile>();
        private readonly List<StreamedMeshTile> fallenLogs = new List<StreamedMeshTile>(64);
        private GameObject forestRoot;
        private InstancedForestRenderer renderer;
        private IslandPreparedTreeCollider[][] trunks, logs;
        private double uploadBudgetMilliseconds;
        private int revision;
        private bool initialized;

        internal GameObject Root => forestRoot;
        internal int ColliderTileCount => tiles.Count;
        internal int ActiveTrunkColliderCount { get; private set; }
        internal int ActiveLogColliderCount { get; private set; }

        internal void Initialize(Transform parent, IslandPreparedForestData prepared, bool showForests,
            double uploadBudgetMilliseconds = 4.0, Texture detailNoise = null, Material fallenLogMaterial = null)
        {
            if (initialized) throw new InvalidOperationException("The forest streamer is already initialized.");
            if (parent == null) throw new ArgumentNullException(nameof(parent));
            if (prepared == null) throw new ArgumentNullException(nameof(prepared));
            if (prepared.hybrid == null) throw new ArgumentException("The instanced forest catalog is required.", nameof(prepared));
            if (double.IsNaN(uploadBudgetMilliseconds) || double.IsInfinity(uploadBudgetMilliseconds))
                throw new ArgumentOutOfRangeException(nameof(uploadBudgetMilliseconds));
            this.uploadBudgetMilliseconds = Math.Max(.1, uploadBudgetMilliseconds);
            trunks = prepared.lod0TrunkColliderTiles;
            logs = prepared.lod0LogColliderTiles;
            try
            {
                forestRoot = new GameObject("Instanced Forest") { layer = parent.gameObject.layer };
                forestRoot.transform.SetParent(parent, false);
                forestRoot.SetActive(showForests);
                renderer = forestRoot.AddComponent<InstancedForestRenderer>();
                renderer.Initialize(forestRoot.transform, prepared.hybrid, detailNoise);
                CreateFallenLogs(prepared.fallenLogTiles, fallenLogMaterial);
                initialized = true;
            }
            catch { Dispose(); throw; }
        }

        private void CreateFallenLogs(IslandPreparedMesh[] prepared, Material material)
        {
            if (prepared == null) return;
            for (var index = 0; index < prepared.Length; ++index)
            {
                var source = prepared[index];
                if (source == null || source.triangles.Length == 0) continue;
                if (material == null)
                    throw new ArgumentNullException(nameof(material), "Authored fallen logs require their wood material.");
                fallenLogs.Add(StreamedMeshTile.Create(source, forestRoot.transform, material,
                    $"Fallen logs tile {index % 8},{index / 8}"));
            }
        }

        internal void SetVisible(bool visible) => forestRoot?.SetActive(visible);

        internal bool Lod0NeighborhoodMatches(Vector2Int center)
        {
            if (!initialized) return false;
            if (!NeighborhoodBounds(center, out var minimum, out var maximum)) return tiles.Count == 0;
            var expectedCount = (maximum.x - minimum.x + 1) * (maximum.y - minimum.y + 1);
            if (tiles.Count != expectedCount) return false;
            for (var y = minimum.y; y <= maximum.y; ++y)
                for (var x = minimum.x; x <= maximum.x; ++x)
                    if (!tiles.ContainsKey(new Vector2Int(x, y))) return false;
            return true;
        }

        internal IEnumerator UpdateLod0NeighborhoodIncremental(Vector2Int center, Func<bool> stillWanted)
        {
            if (!initialized) yield break;
            if (stillWanted == null) throw new ArgumentNullException(nameof(stillWanted));
            var request = ++revision;
            DestroyPending();
            bool Wanted() => initialized && request == revision && stillWanted();
            var incoming = Neighborhood(center);
            var wantedKeys = new HashSet<Vector2Int>(incoming);
            var retired = new List<Vector2Int>();
            foreach (var key in tiles.Keys) if (!wantedKeys.Contains(key)) retired.Add(key);
            foreach (var key in retired)
            {
                if (!Wanted()) yield break;
                var tile = tiles[key];
                tiles.Remove(key);
                ActiveTrunkColliderCount -= tile.trunks;
                ActiveLogColliderCount -= tile.logs;
                DestroyTile(tile);
                yield return null;
            }
            foreach (var key in incoming)
            {
                if (!Wanted()) yield break;
                if (tiles.ContainsKey(key)) continue;
                var tile = CreatePendingTile(key);
                try
                {
                    var index = key.y * Lod0Resolution + key.x;
                    var timer = Stopwatch.StartNew();
                    var sliceCount = 0;
                    for (var kind = 0; kind < 2; ++kind)
                    {
                        if (!Wanted()) yield break;
                        var source = (kind == 0 ? trunks : logs)[index];
                        if (source == null) continue;
                        for (var collider = 0; collider < source.Length; ++collider)
                        {
                            if (!Wanted()) yield break;
                            CreateCollider(tile.root.transform, source[collider], key, collider, kind == 1);
                            if (kind == 0) ++tile.trunks; else ++tile.logs;
                            if (++sliceCount < MaximumCollidersPerSlice
                                && timer.Elapsed.TotalMilliseconds < uploadBudgetMilliseconds) continue;
                            yield return null;
                            if (!Wanted()) yield break;
                            timer.Restart();
                            sliceCount = 0;
                        }
                    }
                    if (!Wanted()) yield break;
                    tiles.Add(key, tile);
                    pending.Remove(tile);
                    ActiveTrunkColliderCount += tile.trunks;
                    ActiveLogColliderCount += tile.logs;
                    tile.root.SetActive(true);
                }
                finally
                {
                    // Disposing a coroutine, retargeting it, clearing focus, and
                    // exceptions all retire an unfinished tile without activation.
                    if (pending.Remove(tile)) DestroyTile(tile);
                }
                yield return null;
            }
        }

        internal void ClearPlayerFocus()
        {
            ++revision;
            DestroyPending();
            foreach (var tile in tiles.Values) DestroyTile(tile);
            tiles.Clear();
            ActiveTrunkColliderCount = ActiveLogColliderCount = 0;
        }

        private ColliderTile CreatePendingTile(Vector2Int key)
        {
            var root = new GameObject($"Forest colliders {key.x},{key.y}") { layer = forestRoot.layer };
            root.transform.SetParent(forestRoot.transform, false);
            root.SetActive(false);
            var tile = new ColliderTile(root);
            pending.Add(tile);
            return tile;
        }

        private static void CreateCollider(Transform parent, IslandPreparedTreeCollider source,
            Vector2Int tile, int index, bool fallenLog)
        {
            var axis = source.top - source.bottom;
            var length = axis.magnitude;
            if (!(length > Mathf.Epsilon) || float.IsInfinity(length)
                || !(source.radius > 0) || float.IsInfinity(source.radius))
                throw new InvalidOperationException($"Forest collider {tile.x},{tile.y}:{index} is degenerate.");
            var root = new GameObject($"Forest {(fallenLog ? "log" : "trunk")} collider {tile.x},{tile.y}:{index}")
                { layer = parent.gameObject.layer };
            root.transform.SetParent(parent, false);
            root.transform.localPosition = (source.bottom + source.top) * .5f;
            root.transform.localRotation = Quaternion.FromToRotation(Vector3.up, axis / length);
            if (fallenLog)
            {
                var box = root.AddComponent<BoxCollider>();
                box.size = new Vector3(source.radius * 2, length, source.radius * 2);
            }
            else
            {
                var capsule = root.AddComponent<CapsuleCollider>();
                capsule.direction = 1;
                capsule.center = Vector3.zero;
                capsule.radius = source.radius;
                capsule.height = Mathf.Max(length, source.radius * 2);
            }
        }

        private static List<Vector2Int> Neighborhood(Vector2Int center)
        {
            var result = new List<Vector2Int>(9);
            if (!NeighborhoodBounds(center, out var minimum, out var maximum)) return result;
            for (var y = minimum.y; y <= maximum.y; ++y)
                for (var x = minimum.x; x <= maximum.x; ++x)
                    result.Add(new Vector2Int(x, y));
            return result;
        }

        private static bool NeighborhoodBounds(Vector2Int center, out Vector2Int minimum, out Vector2Int maximum)
        {
            minimum = maximum = default;
            // Reject distant centres before adding/subtracting the radius, so
            // even extreme outside coordinates cannot overflow the bounds.
            if (center.x < -NearbyRadius || center.y < -NearbyRadius
                || center.x >= Lod0Resolution + NearbyRadius || center.y >= Lod0Resolution + NearbyRadius) return false;
            minimum = new Vector2Int(Mathf.Max(0, center.x - NearbyRadius), Mathf.Max(0, center.y - NearbyRadius));
            maximum = new Vector2Int(Mathf.Min(Lod0Resolution - 1, center.x + NearbyRadius), Mathf.Min(Lod0Resolution - 1, center.y + NearbyRadius));
            return true;
        }

        private void DestroyPending()
        {
            foreach (var tile in pending) DestroyTile(tile);
            pending.Clear();
        }

        private static void DestroyTile(ColliderTile tile)
        {
            if (tile.root == null) return;
            tile.root.SetActive(false);
            DestroyUnityObject(tile.root);
        }

        public void Dispose()
        {
            initialized = false;
            ClearPlayerFocus();
            renderer?.Release();
            renderer = null;
            foreach (var tile in fallenLogs) tile.Dispose();
            fallenLogs.Clear();
            DestroyUnityObject(forestRoot);
            forestRoot = null;
            trunks = logs = null;
        }
    }
}
