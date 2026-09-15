using System;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using UnityEngine;

namespace Motu.Streaming
{
    // A single-use transfer: the uploader clears each tile after creating its mesh.
    internal sealed class ForestMeshRegion
    {
        internal readonly IslandPreparedMesh[] foliage;
        internal readonly IslandPreparedMesh[] wood;

        internal ForestMeshRegion(int tileCount)
        {
            foliage = new IslandPreparedMesh[tileCount];
            wood = new IslandPreparedMesh[tileCount];
        }
    }

    internal static class ForestGridWorker
    {
        // Bound forest export/copy across all resident islands. No detailed managed
        // forest grids are retained on an island that has no player focus.
        private static readonly SemaphoreSlim Gate = new SemaphoreSlim(1, 1);

        internal static async Task<ForestMeshRegion> PrepareAsync(NativeIslandHandle owner,
            float worldSize, int lod, Vector2Int key, CancellationToken cancellation)
        {
            await Gate.WaitAsync(cancellation).ConfigureAwait(false);
            try
            {
                cancellation.ThrowIfCancellationRequested();
                using var lease = owner.Acquire();
                return await Task.Run(() => Prepare(lease.Value, worldSize, lod, key, cancellation),
                    cancellation).ConfigureAwait(false);
            }
            finally { Gate.Release(); }
        }

        internal static ForestMeshRegion Prepare(IntPtr handle, float worldSize, int lod,
            Vector2Int key, CancellationToken cancellation)
        {
            if (lod != 0 && lod != 1) throw new ArgumentOutOfRangeException(nameof(lod));
            var divisions = lod == 1 ? ForestTileStreamer.Lod1Resolution / ForestTileStreamer.Lod2Resolution : 1;
            var resolution = ForestTileStreamer.Lod1Resolution / divisions;
            if (key.x < 0 || key.y < 0 || key.x >= resolution || key.y >= resolution)
                throw new ArgumentOutOfRangeException(nameof(key));
            var result = new ForestMeshRegion(divisions * divisions);
            for (var index = 0; index < result.foliage.Length; index++)
            {
                cancellation.ThrowIfCancellationRequested();
                var x = key.x * divisions + index % divisions;
                var y = key.y * divisions + index / divisions;
                var area = TileArea(x, y);
                // Export leaves separately to keep native temporary buffers small.
                // Their half-open bounds also preserve whole-grid owner assignment
                // for anchors exactly on a tile edge, including island corners.
                result.foliage[index] = VegetationPreparation.PrepareForestMeshGrid(
                    handle, worldSize, lod, 1, false, area, cancellation)[0];
                result.wood[index] = VegetationPreparation.PrepareForestMeshGrid(
                    handle, worldSize, lod, 1, true, area, cancellation)[0];
            }
            cancellation.ThrowIfCancellationRequested();
            return result;
        }

        internal static MotuNative.ExportArea TileArea(int x, int y)
        {
            const int resolution = ForestTileStreamer.Lod1Resolution;
            return new MotuNative.ExportArea(x / (float)resolution, y / (float)resolution,
                InclusiveUpperBound((x + 1) / (float)resolution),
                InclusiveUpperBound((y + 1) / (float)resolution));
        }

        private static float InclusiveUpperBound(float boundary)
        {
            // Native bounds are inclusive. The previous representable float is
            // an exact half-open bound for these positive, power-of-two cells.
            return boundary == 1f ? 1f
                : BitConverter.Int32BitsToSingle(BitConverter.SingleToInt32Bits(boundary) - 1);
        }
    }
}
