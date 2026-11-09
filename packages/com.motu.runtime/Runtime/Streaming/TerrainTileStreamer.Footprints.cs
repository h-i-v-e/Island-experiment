using System;
using Motu.World;
using UnityEngine;

namespace Motu.Streaming
{
    public sealed partial class TerrainTileStreamer
    {
        /// <summary>Queries resident CPU terrain without activating colliders or changing streaming.</summary>
        public TerrainFootprintQueryStatus QueryTerrainFootprint(TerrainFootprint footprint,
            out TerrainFootprintSample sample, int maximumCellTests = 262144)
        {
            Span<Vector2> corners = stackalloc Vector2[4];
            var valid = TerrainFootprintGeometry.TryGetCorners(footprint, corners, out _, out var area);
            var accumulator = new TerrainFootprintAccumulator(area, maximumCellTests);
            if (!valid) accumulator.MarkInvalid();
            if (accumulator.CanContinue)
                AccumulateTerrainFootprint(corners, transform.position.y, ref accumulator);
            sample = accumulator.Build();
            return sample.Status;
        }

        internal void AccumulateTerrainFootprint(ReadOnlySpan<Vector2> worldPolygon,
            float seaLevel, ref TerrainFootprintAccumulator accumulator)
        {
            var requestedArea = TerrainFootprintGeometry.Area(worldPolygon);
            if (!accumulator.CanContinue || !(requestedArea > 0d)) return;
            if (!TerrainFootprintGeometry.HasSupportedTransform(transform))
            {
                accumulator.MarkUnsupported();
                return;
            }
            if (!float.IsFinite(worldSize) || !float.IsFinite(seaLevel))
            {
                accumulator.MarkInvalid();
                return;
            }
            if (disposed || colliderHeightMap == null || !(worldSize > 0f))
            {
                accumulator.AddMissing(requestedArea);
                return;
            }

            Span<Vector2> polygon = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            var localToWorld = transform.localToWorldMatrix;
            var worldToLocal = transform.worldToLocalMatrix;
            var metrics = new FootprintWorldMetrics(localToWorld, worldToLocal);
            var minimum = new Vector2(float.PositiveInfinity, float.PositiveInfinity);
            var maximum = new Vector2(float.NegativeInfinity, float.NegativeInfinity);
            for (var i = 0; i < worldPolygon.Length; i++)
            {
                var local = worldToLocal.MultiplyPoint3x4(new Vector3(worldPolygon[i].x,
                    localToWorld.m13, worldPolygon[i].y));
                polygon[i] = new Vector2(local.x, local.z);
                minimum = Vector2.Min(minimum, polygon[i]);
                maximum = Vector2.Max(maximum, polygon[i]);
            }
            AccumulateLocalCells(polygon.Slice(0, worldPolygon.Length), minimum, maximum,
                seaLevel, metrics, ref accumulator);
        }

        private void AccumulateLocalCells(ReadOnlySpan<Vector2> polygon, Vector2 minimum,
            Vector2 maximum, float seaLevel, FootprintWorldMetrics metrics,
            ref TerrainFootprintAccumulator accumulator)
        {
            var intervals = colliderHeightMap.dimension - 1;
            var spacing = (double)worldSize / intervals;
            var half = worldSize * .5d;
            if (maximum.x <= -half || minimum.x >= half || maximum.y <= -half || minimum.y >= half)
            {
                accumulator.AddMissing(TerrainFootprintGeometry.Area(polygon));
                return;
            }
            var firstX = Math.Max(0, (int)Math.Floor((Math.Max(minimum.x, -half) + half) / spacing));
            var lastX = Math.Min(intervals - 1, (int)Math.Ceiling((Math.Min(maximum.x, half) + half) / spacing) - 1);
            var firstZ = Math.Max(0, (int)Math.Floor((Math.Max(minimum.y, -half) + half) / spacing));
            var lastZ = Math.Min(intervals - 1, (int)Math.Ceiling((Math.Min(maximum.y, half) + half) / spacing) - 1);
            var candidates = (long)(lastX - firstX + 1) * (lastZ - firstZ + 1);
            if (candidates > accumulator.RemainingBudget)
            {
                accumulator.MarkWorkLimit();
                return;
            }
            if (minimum.x < -half || maximum.x > half || minimum.y < -half || maximum.y > half)
            {
                // Outside this map is unknown even if the sliver is smaller than
                // the numerical tolerance used when summing interior cell areas.
                accumulator.AddMissing(double.Epsilon);
            }
            Span<Vector2> clipped = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            Span<Vector2> scratch = stackalloc Vector2[TerrainFootprintGeometry.PolygonCapacity];
            var covered = 0d;
            for (var z = firstZ; z <= lastZ; z++)
            for (var x = firstX; x <= lastX; x++)
            {
                if (!accumulator.TryVisitCandidate()) return;
                var bounds = Rect.MinMaxRect((float)(x * spacing - half), (float)(z * spacing - half),
                    (float)((x + 1) * spacing - half), (float)((z + 1) * spacing - half));
                var count = TerrainFootprintGeometry.ClipToRect(polygon, bounds, clipped, scratch);
                var area = TerrainFootprintGeometry.Area(clipped.Slice(0, count));
                if (!(area > 0d)) continue;
                covered += area;
                AccumulateHeightCell(x, z, area * metrics.AreaScale, (float)spacing,
                    seaLevel, metrics, ref accumulator);
            }
            var missingArea = TerrainFootprintGeometry.Area(polygon) - covered;
            if (missingArea > TerrainFootprintGeometry.Area(polygon) * .00001d)
                accumulator.AddMissing(missingArea);
        }

        private void AccumulateHeightCell(int x, int z, double area, float spacing,
            float seaLevel, FootprintWorldMetrics metrics, ref TerrainFootprintAccumulator accumulator)
        {
            var h00 = colliderHeightMap.WorldHeightAt(x, z);
            var h10 = colliderHeightMap.WorldHeightAt(x + 1, z);
            var h01 = colliderHeightMap.WorldHeightAt(x, z + 1);
            var h11 = colliderHeightMap.WorldHeightAt(x + 1, z + 1);
            var minimum = (float)(Math.Min(Math.Min(h00, h10), Math.Min(h01, h11))
                * metrics.HeightScale + metrics.HeightOrigin);
            var maximum = (float)(Math.Max(Math.Max(h00, h10), Math.Max(h01, h11))
                * metrics.HeightScale + metrics.HeightOrigin);
            var gx = Math.Max(Math.Abs((double)h10 - h00), Math.Abs((double)h11 - h01)) / spacing;
            var gz = Math.Max(Math.Abs((double)h01 - h00), Math.Abs((double)h11 - h10)) / spacing;
            var slope = metrics.SlopeBound(gx, gz);
            accumulator.AddTerrain(area, minimum, maximum, slope, seaLevel);
        }

        private readonly struct FootprintWorldMetrics
        {
            internal readonly double HeightOrigin, HeightScale, AreaScale;
            private readonly double gradientXX, gradientZZ, gradientXZ;

            internal FootprintWorldMetrics(Matrix4x4 localToWorld, Matrix4x4 worldToLocal)
            {
                HeightOrigin = localToWorld.m13;
                HeightScale = localToWorld.m11;
                AreaScale = Math.Abs((double)localToWorld.m00 * localToWorld.m22
                    - (double)localToWorld.m02 * localToWorld.m20);
                // The inverse transpose maps local height gradients to world XZ.
                // Retain matrix roundoff in the bound instead of assuming exact
                // orthonormality after a hierarchy of floating point yaw rotations.
                double a = worldToLocal.m00, b = worldToLocal.m02;
                double c = worldToLocal.m20, d = worldToLocal.m22;
                gradientXX = a * a + b * b;
                gradientZZ = c * c + d * d;
                gradientXZ = Math.Abs(a * c + b * d);
            }

            internal float SlopeBound(double gx, double gz)
            {
                var squared = gx * gx * gradientXX + gz * gz * gradientZZ
                    + 2d * gx * gz * gradientXZ;
                return (float)(Math.Atan(HeightScale * Math.Sqrt(squared)) * Mathf.Rad2Deg);
            }
        }
    }
}
