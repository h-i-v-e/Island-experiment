namespace Motu.World
{
    public enum TerrainSurfaceKind : byte { Dirt, Grass, Stone, Sand, Gravel, Mud, Snow }

    public readonly struct TerrainSurfaceSample
    {
        public readonly TerrainSurfaceKind Kind;
        public readonly float GroundHeight, SeaLevel;
        internal TerrainSurfaceSample(TerrainSurfaceKind kind, float groundHeight, float seaLevel)
        { Kind = kind; GroundHeight = groundHeight; SeaLevel = seaLevel; }
    }
}
