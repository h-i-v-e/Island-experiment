using System;
using Motu.World;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        /// <summary>
        /// Assess a world-XZ rectangle using the prepared CPU terrain heightfield.
        /// This does not request terrain, create colliders or change streaming focus.
        /// </summary>
        public TerrainFootprintQueryStatus QueryTerrainFootprint(
            TerrainFootprint footprint,
            out TerrainFootprintSample sample,
            int maximumCellTests = 262144)
        {
            Span<Vector2> polygon = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            var valid = TerrainFootprintGeometry.TryGetCorners(footprint, polygon, out _, out var area);
            var accumulator = new TerrainFootprintAccumulator(valid ? area : 0d, maximumCellTests);
            if (!valid || maximumCellTests <= 0)
                accumulator.MarkInvalid();
            else if (this == null)
                accumulator.AddMissing(area);
            else
                AccumulateTerrainFootprint(polygon.Slice(0, 4), transform.position.y, ref accumulator);
            sample = accumulator.Build();
            return sample.Status;
        }

        internal void AccumulateTerrainFootprint(
            ReadOnlySpan<Vector2> worldPolygon,
            float seaLevel,
            ref TerrainFootprintAccumulator accumulator)
        {
            if (this == null || terrainStreamer == null)
            {
                accumulator.AddMissing(TerrainFootprintGeometry.Area(worldPolygon));
                return;
            }
            if (!HasSupportedTransform())
            {
                accumulator.MarkUnsupported();
                return;
            }
            terrainStreamer.AccumulateTerrainFootprint(worldPolygon, seaLevel, ref accumulator);
        }
    }
}
