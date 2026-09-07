using System;
using System.Globalization;
using Motu.Streaming;

namespace Motu.Islands
{
    internal static class IslandGenerationStatus
    {
        internal static string Installed(
            TerrainTileStreamer terrainStreamer,
            TimeSpan elapsed,
            bool cachedMaterials,
            float worldSize,
            bool cachedSnapshot,
            int islandSeed)
        {
            var status = string.Format(
                CultureInfo.InvariantCulture,
                "{0} | Seed {1} | 64 LOD 2 tiles | {2:N0} vertices | {3:N0} triangles | {4:F2}s",
                cachedSnapshot ? "Disk cache" : "CPU",
                islandSeed,
                terrainStreamer.BaseVertexCount,
                terrainStreamer.BaseTriangleCount,
                elapsed.TotalSeconds);
            status += " | shared 2048 terrain shading map";
            if (cachedMaterials)
            {
                status += " | cached material maps";
            }
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | {0:N0} waterfall feet / 32 pooled fog volumes",
                terrainStreamer.WaterfallFootCount);
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | 3x3 hidden LOD 1 terrain colliders (129x129 samples each) | {0:F1} km square",
                worldSize / 1000f);
            return status;
        }
    }
}
