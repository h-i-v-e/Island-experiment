using System;

namespace Motu.World
{
    public enum TerrainFootprintQueryStatus
    {
        Incomplete,
        Complete,
        WorkLimitExceeded,
        InvalidInput,
        UnsupportedTransform,
    }

    /// <summary>Terrain bounds are conservative over intersecting collision-heightfield
    /// cells, rather than exact bounds of the native mesh between lattice samples.
    /// Areas are square metres. Unknown areas never count as sea or usable terrain.</summary>
    public readonly struct TerrainFootprintSample
    {
        public TerrainFootprintQueryStatus Status { get; }
        public bool IsComplete => Status == TerrainFootprintQueryStatus.Complete;
        public bool HasTerrain => TerrainArea > 0d;
        public double RequestedArea { get; }
        public double TerrainArea { get; }
        public double OpenSeaArea { get; }
        public double UnknownArea { get; }
        public float CoverageFraction { get; }
        public float MinimumHeight { get; }
        public float MaximumHeight { get; }
        public float HeightRange => MaximumHeight - MinimumHeight;
        public float MaximumSlopeDegrees { get; }
        public float MinimumSeaClearance { get; }
        public int TestedCellCount { get; }
        public bool IsDryLand => IsComplete && HasTerrain && OpenSeaArea == 0d
            && UnknownArea == 0d && MinimumSeaClearance >= 0f;

        internal TerrainFootprintSample(TerrainFootprintQueryStatus status, double requestedArea,
            double terrainArea, double openSeaArea, double unknownArea, float minimumHeight,
            float maximumHeight, float maximumSlope, float minimumClearance, int testedCells)
        {
            Status = status;
            RequestedArea = requestedArea;
            TerrainArea = terrainArea;
            OpenSeaArea = openSeaArea;
            UnknownArea = unknownArea;
            CoverageFraction = requestedArea > 0d
                ? (float)Math.Min(1d, (terrainArea + openSeaArea) / requestedArea) : 0f;
            MinimumHeight = minimumHeight;
            MaximumHeight = maximumHeight;
            MaximumSlopeDegrees = maximumSlope;
            MinimumSeaClearance = minimumClearance;
            TestedCellCount = testedCells;
        }
    }
}
