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
            CancellationToken cancellationToken,
            bool prepareNavigation = true)
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
                request.SnapshotCacheBudgetBytes,
                request.CaveOptions, request.CaveNetworkOptions, request.CaveWalkOptions,
                prepareNavigation && request.Navigation.Enabled && IslandNavigationIntegration.IsAvailable);
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
            long snapshotCacheBudgetBytes = 0L,
            CaveNative.Options? caveOptions = null,
            CaveNative.NetworkOptions? networkOptions = null,
            CaveNative.WalkOptions? walkOptions = null, bool prepareNavigation = true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var handle = LoadOrGenerateNative(islandSeed, options, forestOptions, reedOptions,
                fernOptions, snapshotPath, snapshotCacheBudgetBytes, caveOptions, networkOptions,
                walkOptions, out var generated);
            try
            {
                cancellationToken.ThrowIfCancellationRequested();
                var surfaceMaps = PrepareSurfaceMaps(handle, SurfaceMapDimension);
                cancellationToken.ThrowIfCancellationRequested();
                var lodSurfaceMaps = new IslandPreparedSurfaceMaps[2];
                lodSurfaceMaps[0] = PrepareSurfaceAtlas(handle, 1, 2048);
                cancellationToken.ThrowIfCancellationRequested();
                lodSurfaceMaps[1] = PrepareSurfaceAtlas(handle, 2, 1024);
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
                var navigationMesh = prepareNavigation ? PrepareNavigationMesh(handle, worldSize) : null;
                cancellationToken.ThrowIfCancellationRequested();
                var lod3 = PrepareLod3(handle, worldSize);
                IslandHorizonCache.TrySave(snapshotPath, lod3, snapshotCacheBudgetBytes);
                cancellationToken.ThrowIfCancellationRequested();
                var overviewTiles = TerrainTileStreamer.PrepareOverviewTiles(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var riverTiles = PrepareRiverTiles(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var riverRockTiles = PrepareRiverRockTiles(handle, worldSize);
                var boulderColliders = PrepareBoulderColliders(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var forest = PrepareForestData(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var reedTiles = PrepareReedMeshGrid(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var fernTiles = PrepareFernMeshGrid(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var waterfallFeet = PrepareWaterfallFeet(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var cavesPrepared = CavePreparation.Prepare(handle, worldSize, cancellationToken);
                var landBirdPerches = PrepareLandBirdPerches(handle, worldSize);
                cancellationToken.ThrowIfCancellationRequested();
                var result = new IslandPreparedData(
                    handle,
                    !generated,
                    surfaceMaps,
                    seaMask,
                    lod3,
                    overviewTiles,
                    riverTiles,
                    riverRockTiles,
                    forest,
                    reedTiles,
                    fernTiles,
                    waterfallFeet,
                    colliderHeightMap,
                    materialTextures,
                    cavesPrepared,
                    boulderColliders, navigationMesh, landBirdPerches,
                    lodSurfaceMaps: lodSurfaceMaps);
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

        internal static IslandPreparedMesh PrepareHorizon(IslandGenerationRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (IslandHorizonCache.TryLoad(request.SnapshotPath, out var cached))
            {
                cancellationToken.ThrowIfCancellationRequested();
                return cached;
            }
            cancellationToken.ThrowIfCancellationRequested();
            var handle = LoadOrGenerateNative(request.RandomSeed, request.Options, request.ForestOptions,
                request.ReedOptions, request.FernOptions, request.SnapshotPath,
                request.SnapshotCacheBudgetBytes, request.CaveOptions, request.CaveNetworkOptions,
                request.CaveWalkOptions, out _);
            try
            {
                cancellationToken.ThrowIfCancellationRequested();
                var mesh = PrepareLod3(handle, request.WorldSizeMetres);
                IslandHorizonCache.TrySave(request.SnapshotPath, mesh, request.SnapshotCacheBudgetBytes);
                cancellationToken.ThrowIfCancellationRequested();
                return mesh;
            }
            finally { MotuNative.ReleaseMotu(handle); }
        }

        private static IntPtr LoadOrGenerateNative(int islandSeed, MotuNative.Options options,
            MotuNative.ForestOptions forestOptions, MotuNative.ReedOptions reedOptions,
            MotuNative.FernOptions fernOptions, string snapshotPath, long snapshotCacheBudgetBytes,
            CaveNative.Options? caveOptions, CaveNative.NetworkOptions? networkOptions,
            CaveNative.WalkOptions? walkOptions, out bool generated)
        {
            var handle = IslandSnapshotCache.TryLoad(snapshotPath, out var loadStatus);
            generated = handle == IntPtr.Zero;
            if (!generated) return handle;
            var caves = caveOptions ?? new Motu.Settings.IslandCaveSettings().ToNative();
            var network = networkOptions ?? new Motu.Settings.IslandCaveSettings { MaximumBranches = 0 }.ToNativeNetwork();
            var walk = walkOptions ?? new Motu.Settings.IslandCaveSettings { RandomWalk = false }.ToNativeWalk();
            if (CaveNative.CaveAlgorithmRevision() != CaveNative.AlgorithmRevision)
                throw new InvalidOperationException("The native cave library revision does not match this Unity build.");
            handle = CaveNative.CreateMotuWithCaveWalks(islandSeed, ref options,
                ref forestOptions, ref reedOptions, ref fernOptions, ref caves, ref network, ref walk);
            if (handle == IntPtr.Zero)
            {
                // Copy on this worker before another native call replaces its error.
                var error = Marshal.PtrToStringUTF8(CaveNative.GetLastCaveGenerationError())
                    ?? $"island seed {islandSeed}: no native error was reported";
                throw new InvalidOperationException($"Rust CPU generation failed: {error}");
            }
            try
            {
                if (!string.IsNullOrEmpty(snapshotPath)
                    && !IslandSnapshotCache.TrySave(handle, snapshotPath, snapshotCacheBudgetBytes))
                    Debug.LogWarning($"Generated island {islandSeed} but could not cache its native snapshot "
                        + $"(previous load status {loadStatus}).");
                return handle;
            }
            catch { MotuNative.ReleaseMotu(handle); throw; }
        }

        internal static IslandPreparedMesh PrepareNavigationMesh(IntPtr handle, float worldSize)
        {
            // No spatial slice and no LOD transition morph. Cave openings and the
            // existing underwater render cutoff are retained by the native export.
            MotuNative.CreateMesh(handle, IntPtr.Zero, 0, 0, out var export);
            try
            {
                if (export.handle == IntPtr.Zero)
                    throw new InvalidOperationException("The native navigation terrain export failed.");
                return IslandMeshInterop.CopyNavigationMeshData(export, worldSize);
            }
            finally { MotuNative.ReleaseMesh(ref export); }
        }

        private static IslandPreparedMesh PrepareLod3(IntPtr handle, float worldSize)
        {
            MotuNative.CreateIslandLod3Mesh(handle, out var export);
            try
            {
                if (export.handle == IntPtr.Zero)
                    throw new InvalidOperationException("The native generator did not return an island LOD3 mesh.");
                return IslandMeshInterop.CopyOptionalGeneratedMeshData(export, worldSize);
            }
            finally { MotuNative.ReleaseMesh(ref export); }
        }

    }
}
