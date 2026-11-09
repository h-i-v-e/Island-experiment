using System;
using UnityEngine;

namespace Motu.World
{
    public sealed partial class IslandWorldManager
    {
        /// <summary>
        /// Assess an oriented world-XZ footprint across all intersected sections.
        /// Missing expected terrain is incomplete; known open sea is reported separately.
        /// This read-only query never changes the streaming target or requests generation.
        /// </summary>
        public TerrainFootprintQueryStatus QueryTerrainFootprint(
            TerrainFootprint footprint,
            out TerrainFootprintSample sample,
            int maximumCellTests = 262144)
        {
            Span<Vector2> polygon = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            var valid = TerrainFootprintGeometry.TryGetCorners(footprint, polygon, out var bounds, out var area);
            var accumulator = new TerrainFootprintAccumulator(valid ? area : 0d, maximumCellTests);
            if (!valid || maximumCellTests <= 0
                || !TryGetFootprintCells(bounds, out var minimum, out var maximum, out var candidates))
                accumulator.MarkInvalid();
            else if (this == null)
                accumulator.AddMissing(area);
            else if (candidates > maximumCellTests)
                accumulator.MarkWorkLimit();
            else
                AccumulateFootprintSections(polygon.Slice(0, 4), minimum, maximum, ref accumulator);
            sample = accumulator.Build();
            return sample.Status;
        }

        private void AccumulateFootprintSections(
            ReadOnlySpan<Vector2> polygon,
            Vector2Int minimum,
            Vector2Int maximum,
            ref TerrainFootprintAccumulator accumulator)
        {
            Span<Vector2> clipped = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            Span<Vector2> scratch = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            var seaLevel = SeaLevelWorldY();
            if (!float.IsFinite(seaLevel))
            {
                accumulator.MarkInvalid();
                return;
            }
            for (long y = minimum.y; y <= maximum.y && accumulator.CanContinue; y++)
            for (long x = minimum.x; x <= maximum.x && accumulator.CanContinue; x++)
            {
                if (!accumulator.TryVisitCandidate()) return;
                var cell = new Vector2Int((int)x, (int)y);
                var centre = CellCentre(cell);
                var rect = new Rect(centre.x - IslandSizeMetres * .5f,
                    centre.z - IslandSizeMetres * .5f, IslandSizeMetres, IslandSizeMetres);
                var count = TerrainFootprintGeometry.ClipToRect(polygon, rect, clipped, scratch);
                if (count < 3) continue;
                var section = clipped.Slice(0, count);
                var area = TerrainFootprintGeometry.Area(section);
                if (area > 0d)
                    AccumulateFootprintSection(cell, section, area, seaLevel, ref accumulator);
            }
        }

        private void AccumulateFootprintSection(
            Vector2Int cell,
            ReadOnlySpan<Vector2> polygon,
            double area,
            float seaLevel,
            ref TerrainFootprintAccumulator accumulator)
        {
            if (managedIslands.TryGetValue(cell, out var entry) && entry.Generator != null)
                entry.Generator.AccumulateTerrainFootprint(polygon, seaLevel, ref accumulator);
            else if (IslandGenerationRequestFactory != null && !IslandGenerationRequestFactory.HasIsland(cell))
                accumulator.AddOpenSea(area);
            else
                accumulator.AddMissing(area);
        }

        private static bool TryGetFootprintCells(
            Rect bounds,
            out Vector2Int minimum,
            out Vector2Int maximum,
            out double candidates)
        {
            var half = IslandSizeMetres * .5d;
            var minX = Math.Floor(((double)bounds.xMin + half) / IslandSizeMetres);
            var minY = Math.Floor(((double)bounds.yMin + half) / IslandSizeMetres);
            var maxX = Math.Ceiling(((double)bounds.xMax + half) / IslandSizeMetres) - 1d;
            var maxY = Math.Ceiling(((double)bounds.yMax + half) / IslandSizeMetres) - 1d;
            minimum = maximum = default;
            candidates = 0d;
            if (minX < int.MinValue || minY < int.MinValue || maxX > int.MaxValue || maxY > int.MaxValue
                || maxX < minX || maxY < minY
                || !HasRepresentableCellBounds(minX, maxX) || !HasRepresentableCellBounds(minY, maxY))
                return false;
            minimum = new Vector2Int((int)minX, (int)minY);
            maximum = new Vector2Int((int)maxX, (int)maxY);
            candidates = (maxX - minX + 1d) * (maxY - minY + 1d);
            return true;
        }

        private static bool HasRepresentableCellBounds(double minimum, double maximum)
        {
            // At enormous coordinates, float rounding can overlap section
            // rectangles and hide missing terrain as known sea. Reject those
            // coordinates before any coverage is classified.
            var lower = minimum * IslandSizeMetres - IslandSizeMetres * .5d;
            var upper = maximum * IslandSizeMetres + IslandSizeMetres * .5d;
            return (double)(float)lower == lower && (double)(float)upper == upper;
        }
    }
}
