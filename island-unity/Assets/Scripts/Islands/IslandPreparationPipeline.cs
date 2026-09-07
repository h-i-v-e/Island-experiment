using static Motu.Interop.VegetationPreparation;
using static Motu.Interop.MaterialPreparation;
using static Motu.Interop.WaterPreparation;
using static Motu.Interop.TerrainPreparation;
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using UnityEngine;
using Debug = UnityEngine.Debug;
using Motu.Interop;
using Motu.Rendering;
using Motu.Streaming;

namespace Motu.Islands
{
    internal static class IslandPreparationPipeline
    {
        private const int SurfaceMapDimension = 2048;

        internal static IslandPreparedData PrepareIsland(
            IslandGenerationRequest request,
            CancellationToken cancellationToken)
        {
            if (request == null) throw new ArgumentNullException(nameof(request));
            return PrepareIsland(
                request.Descriptor.Seed,
                request.Options,
                request.ForestOptions,
                request.ReedOptions,
                request.FernOptions,
                request.WorldSizeMetres,
                request.MaterialColours,
                request.MaterialTextureResolution,
                cancellationToken,
                request.SnapshotPath,
                request.SnapshotCacheBudgetBytes);
        }

        internal static IslandPreparedData PrepareIsland(
            int islandSeed,
            MotuNative.Options options,
            MotuNative.ForestOptions forestOptions,
            MotuNative.ReedOptions reedOptions,
            MotuNative.FernOptions fernOptions,
            float worldSize,
            IslandMaterialColours materialColours,
            int materialTextureResolution,
            CancellationToken cancellationToken,
            string snapshotPath = null,
            long snapshotCacheBudgetBytes = 0L)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var handle = IslandSnapshotCache.TryLoad(snapshotPath, out var loadStatus);
            var generated = handle == IntPtr.Zero;
            if (generated)
            {
                handle = MotuNative.CreateMotuWithForestReedsAndFerns(
                    islandSeed,
                    ref options,
                    ref forestOptions,
                    ref reedOptions,
                    ref fernOptions);
            }
            if (handle == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust CPU generator returned a null island handle.");
            }

            try
            {
                if (generated
                    && !string.IsNullOrEmpty(snapshotPath)
                    && !IslandSnapshotCache.TrySave(
                        handle,
                        snapshotPath,
                        snapshotCacheBudgetBytes))
                {
                    Debug.LogWarning(
                        $"Generated island {islandSeed} but could not cache its native snapshot "
                        + $"(previous load status {loadStatus}).");
                }
                cancellationToken.ThrowIfCancellationRequested();
                var surfaceMaps = PrepareSurfaceMaps(handle, SurfaceMapDimension);
                cancellationToken.ThrowIfCancellationRequested();
                var seaMask = PrepareSeaMask(handle, SurfaceMapDimension);
                cancellationToken.ThrowIfCancellationRequested();
                var materialTextures = PrepareMaterialTextures(
                    materialColours,
                    materialTextureResolution,
                    snapshotCacheBudgetBytes,
                    string.IsNullOrEmpty(snapshotPath)
                        ? null
                        : Path.GetDirectoryName(snapshotPath));
                cancellationToken.ThrowIfCancellationRequested();
                var colliderHeightMap = PrepareColliderHeightMap(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var overviewTiles = TerrainTileStreamer.PrepareOverviewTiles(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var riverTiles = PrepareRiverTiles(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var riverRockTiles = PrepareRiverRockTiles(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var forest = PrepareForestData(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var reedTiles = PrepareReedMeshGrid(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var fernTiles = PrepareFernMeshGrid(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var waterfallFeet = PrepareWaterfallFeet(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var result = new IslandPreparedData(
                    handle,
                    !generated,
                    surfaceMaps,
                    seaMask,
                    overviewTiles,
                    riverTiles,
                    riverRockTiles,
                    forest,
                    reedTiles,
                    fernTiles,
                    waterfallFeet,
                    colliderHeightMap,
                    materialTextures);
                handle = IntPtr.Zero;
                return result;
            }
            finally
            {
                if (handle != IntPtr.Zero)
                {
                    MotuNative.ReleaseMotu(handle);
                }
            }
        }

    }
}
