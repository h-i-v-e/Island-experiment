namespace Motu.World
{
    /// <summary>Read-only terrain geometry queries that never request streaming or collision.</summary>
    public interface ITerrainFootprintQuery
    {
        TerrainFootprintQueryStatus QueryTerrainFootprint(TerrainFootprint footprint,
            out TerrainFootprintSample sample, int maximumCellTests = 262144);
    }
}
