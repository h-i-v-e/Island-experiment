using System.Collections.Generic;
using Motu.Islands;
using UnityEngine;

namespace Motu.Streaming
{
    public sealed partial class TerrainTileStreamer
    {
        private readonly HashSet<Vector2Int> pinnedColliderCells = new();
        private readonly HashSet<Vector2Int> incomingPinCells = new();
        private readonly List<Vector2Int> retiredPinCells = new();
        private IslandRuntime pinRuntime;
        public int PinnedColliderCount => pinnedColliderCells.Count;
        public bool HasNavigationPins { get; private set; }

        internal void RegisterPinResidency()
        {
            if (disposed || colliderHeightMap == null || colliderRoot == null || worldSize <= 0) return;
            pinRuntime = GetComponentInParent<IslandRuntime>(true);
            // Visual Terrain Tiles may turn off at LOD3. Physics stays below
            // the resident island itself and is still disposed with that island.
            if (pinRuntime != null) colliderRoot.transform.SetParent(pinRuntime.transform, true);
            TerrainAreaPin.RegisterResident(this);
        }
        private void OnDestroy() => TerrainAreaPin.UnregisterResident(this);

        internal void RefreshPinnedColliders()
        {
            if (disposed || colliderHeightMap == null || colliderRoot == null) return;
            incomingPinCells.Clear();
            bool navigation = false;
            foreach (var pin in TerrainAreaPin.Sources)
            {
                if (pin == null || !pin.TryGetWorldBounds(out var worldBounds)) continue;
                var bounds = TerrainAreaPin.TransformBounds(transform.worldToLocalMatrix, worldBounds);
                float half = worldSize * .5f;
                if (bounds.max.x < -half || bounds.min.x > half || bounds.max.z < -half || bounds.min.z > half) continue;
                var minimum = LocalCell(bounds.min, Lod1Resolution);
                var maximum = LocalCell(bounds.max, Lod1Resolution);
                for (int y = minimum.y; y <= maximum.y; y++)
                    for (int x = minimum.x; x <= maximum.x; x++) incomingPinCells.Add(new Vector2Int(x, y));
                navigation |= pin.RequiresNavigation;
            }
            bool created = false;
            // If creation fails, leave the previous pins in place. No retiring
            // support is disabled until every incoming tile has been installed.
            foreach (var cell in incomingPinCells)
                if (!colliderTiles.ContainsKey(cell))
                { colliderTiles.Add(cell, CreateColliderTile(cell)); created = true; }
            if (created) Physics.SyncTransforms();
            retiredPinCells.Clear();
            foreach (var cell in pinnedColliderCells)
                if (!incomingPinCells.Contains(cell) && !InViewerNeighborhood(cell, currentLod1)
                    && !InViewerNeighborhood(cell, requestedLod1)) retiredPinCells.Add(cell);
            pinnedColliderCells.Clear(); pinnedColliderCells.UnionWith(incomingPinCells);
            foreach (var cell in retiredPinCells) RetireCollider(cell);
            if (navigation != HasNavigationPins)
            {
                HasNavigationPins = navigation;
                pinRuntime?.RefreshNavigationRegistration();
            }
        }

        private static bool InViewerNeighborhood(Vector2Int cell, Vector2Int center) => center != InvalidCell
            && Mathf.Abs(cell.x - center.x) <= NearbyRadius && Mathf.Abs(cell.y - center.y) <= NearbyRadius;

        private void RetireCollider(Vector2Int cell)
        {
            if (!colliderTiles.TryGetValue(cell, out var tile)) return;
            tile.collider.enabled = false;
            colliderTiles.Remove(cell); DestroyColliderTile(tile);
        }

        private void RetainPinnedColliders()
        {
            RefreshPinnedColliders();
            removalScratch.Clear();
            foreach (var cell in colliderTiles.Keys)
                if (!pinnedColliderCells.Contains(cell)) removalScratch.Add(cell);
            foreach (var cell in removalScratch) RetireCollider(cell);
        }
    }
}
