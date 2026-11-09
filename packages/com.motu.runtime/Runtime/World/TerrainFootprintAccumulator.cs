using System;

namespace Motu.World
{
    internal struct TerrainFootprintAccumulator
    {
        private readonly double requestedArea;
        private readonly int maximumCellTests;
        private TerrainFootprintQueryStatus status;
        private double terrainArea, openSeaArea;
        private float minimumHeight, maximumHeight, maximumSlope, minimumClearance;
        private int testedCellCount;
        private bool missing;

        internal TerrainFootprintAccumulator(double requestedArea, int maximumCellTests)
        {
            this.requestedArea = requestedArea;
            this.maximumCellTests = maximumCellTests;
            status = requestedArea > 0d && !double.IsInfinity(requestedArea) && maximumCellTests > 0
                ? TerrainFootprintQueryStatus.Incomplete : TerrainFootprintQueryStatus.InvalidInput;
            terrainArea = openSeaArea = 0d;
            minimumHeight = minimumClearance = float.PositiveInfinity;
            maximumHeight = float.NegativeInfinity;
            maximumSlope = 0f;
            testedCellCount = 0;
            missing = false;
        }

        internal bool CanContinue => status == TerrainFootprintQueryStatus.Incomplete;
        internal int RemainingBudget => Math.Max(0, maximumCellTests - testedCellCount);

        internal bool TryVisitCandidate()
        {
            if (!CanContinue) return false;
            if (testedCellCount == maximumCellTests)
            {
                status = TerrainFootprintQueryStatus.WorkLimitExceeded;
                return false;
            }
            testedCellCount++;
            return true;
        }

        internal void AddOpenSea(double area) => openSeaArea += Math.Max(0d, area);
        internal void AddMissing(double area) { missing |= area > 0d; }
        internal void MarkUnsupported() => status = TerrainFootprintQueryStatus.UnsupportedTransform;
        internal void MarkInvalid() => status = TerrainFootprintQueryStatus.InvalidInput;
        internal void MarkWorkLimit() => status = TerrainFootprintQueryStatus.WorkLimitExceeded;

        internal void AddTerrain(double area, float minHeight, float maxHeight,
            float slopeDegrees, float seaLevel)
        {
            if (!(area > 0d)) return;
            var clearance = (double)minHeight - seaLevel;
            if (!float.IsFinite(minHeight) || !float.IsFinite(maxHeight)
                || !float.IsFinite(slopeDegrees) || !double.IsFinite(clearance)
                || Math.Abs(clearance) > float.MaxValue)
            {
                MarkInvalid();
                return;
            }
            terrainArea += area;
            minimumHeight = Math.Min(minimumHeight, minHeight);
            maximumHeight = Math.Max(maximumHeight, maxHeight);
            maximumSlope = Math.Max(maximumSlope, slopeDegrees);
            minimumClearance = Math.Min(minimumClearance, (float)clearance);
        }

        internal TerrainFootprintSample Build()
        {
            var unknown = Math.Max(0d, requestedArea - terrainArea - openSeaArea);
            // Polygon clipping and transformed float coordinates can differ by
            // a few ulps across section seams. Only numerical residue is closed.
            if (!missing && unknown <= requestedArea * .00001d) unknown = 0d;
            var resultStatus = status;
            if (CanContinue && !missing && unknown == 0d)
                resultStatus = TerrainFootprintQueryStatus.Complete;
            var hasTerrain = terrainArea > 0d;
            var clearance = hasTerrain ? minimumClearance : float.NaN;
            if (hasTerrain && openSeaArea > 0d) clearance = Math.Min(clearance, 0f);
            return new TerrainFootprintSample(resultStatus, requestedArea, terrainArea, openSeaArea,
                unknown, hasTerrain ? minimumHeight : float.NaN,
                hasTerrain ? maximumHeight : float.NaN, hasTerrain ? maximumSlope : float.NaN,
                clearance, testedCellCount);
        }
    }
}
