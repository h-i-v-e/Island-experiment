using System;
using UnityEngine;

namespace Motu.World
{
    public sealed partial class IslandWorldManager
    {
        // Only resident or detailed-island generators matter to distant AI.
        // This never discovers islands, moves the streaming target, or starts work.
        internal bool IsShipFootprintClear(TerrainFootprint footprint, float seaLevel, float safeDraft)
        {
            Span<Vector2> polygon = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            Span<Vector2> clipped = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            Span<Vector2> scratch = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            if (!TerrainFootprintGeometry.TryGetCorners(footprint, polygon, out var bounds, out _)
                || !TryGetFootprintCells(bounds, out var minimum, out var maximum, out var candidates)
                || candidates > 256) return false;

            for (long z = minimum.y; z <= maximum.y; z++)
            for (long x = minimum.x; x <= maximum.x; x++)
            {
                var cell = new Vector2Int((int)x, (int)z);
                // A generating entry without a generator is building only a
                // horizon silhouette, so its detailed terrain is still unloaded.
                if (!managedIslands.TryGetValue(cell, out var entry)
                    || entry.Generator == null) continue;
                var centre = CellCentre(cell);
                var rect = new Rect(centre.x - IslandSizeMetres * .5f,
                    centre.z - IslandSizeMetres * .5f, IslandSizeMetres, IslandSizeMetres);
                var count = TerrainFootprintGeometry.ClipToRect(polygon.Slice(0, 4), rect, clipped, scratch);
                var section = clipped.Slice(0, count);
                var area = TerrainFootprintGeometry.Area(section);
                if (area <= 0d) continue;
                var accumulator = new TerrainFootprintAccumulator(area, 262144);
                entry.Generator.AccumulateTerrainFootprint(section, seaLevel, ref accumulator);
                var sample = accumulator.Build();
                if (!sample.IsComplete || !sample.HasTerrain
                    || sample.MaximumHeight >= seaLevel - safeDraft) return false;
            }
            return true;
        }
    }
}
