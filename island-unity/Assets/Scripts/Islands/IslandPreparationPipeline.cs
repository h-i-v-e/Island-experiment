using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using UnityEngine;
using Debug = UnityEngine.Debug;

internal static class IslandPreparationPipeline
{
    private const int SurfaceMapDimension = 2048;
    internal static IslandPreparedData PrepareIsland(
        int islandSeed,
        MotuNative.Options options,
        float worldSize,
        IslandMaterialColours materialColours,
        int materialTextureResolution,
        CancellationToken cancellationToken)
    {
        var forestOptions = new MotuNative.ForestOptions
        {
            patchSizeMetres = 200f * 2000f / worldSize,
            noiseThreshold = 0.62f,
            noiseOctaves = 4,
            snowlineMetres = 100f * 2000f / worldSize,
            prototypeCount = 8,
            minimumScale = 1f,
            maximumScale = 2f,
        };
        var reedOptions = new MotuNative.ReedOptions
        {
            bankWidthMetres = 0.8f * 2000f / worldSize,
            patchSizeMetres = 8f * 2000f / worldSize,
            coverageThreshold = 0.18f,
            spacingMetres = 0.36f * 2000f / worldSize,
            rushRatio = 0.45f,
            minimumHeightMetres = 0.65f * 2000f / worldSize,
            maximumHeightMetres = 2.1f * 2000f / worldSize,
            maximumSlopeDegrees = 32f,
        };
        var fernOptions = new MotuNative.FernOptions
        {
            barkClearanceMetres = 0.18f * 2000f / worldSize,
            outerRadiusMetres = 1.65f * 2000f / worldSize,
            spacingMetres = 0.58f * 2000f / worldSize,
            patchSizeMetres = 12f * 2000f / worldSize,
            coverageThreshold = 0.28f,
            minimumLengthMetres = 0.45f * 2000f / worldSize,
            maximumLengthMetres = 1.15f * 2000f / worldSize,
            maximumSlopeDegrees = 34f,
        };
        return PrepareIsland(
            islandSeed,
            options,
            forestOptions,
            reedOptions,
            fernOptions,
            worldSize,
            materialColours,
            materialTextureResolution,
            cancellationToken);
    }

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

    internal static IslandPreparedSurfaceMaps PrepareSurfaceMaps(
        IntPtr handle,
        int dimension)
    {
        MotuNative.CreateSurfaceMaps(handle, 0, dimension, out var surfaceMaps);
        try
        {
            if (surfaceMaps.handle == IntPtr.Zero
                || surfaceMaps.occlusion == IntPtr.Zero
                || surfaceMaps.normalRgb == IntPtr.Zero
                || surfaceMaps.width != dimension
                || surfaceMaps.height != dimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned invalid terrain surface maps.");
            }

            var pixelCount = checked(dimension * dimension);
            var occlusionBytes = new byte[pixelCount];
            Marshal.Copy(surfaceMaps.occlusion, occlusionBytes, 0, occlusionBytes.Length);
            var normalBytes = new byte[checked(pixelCount * 3)];
            Marshal.Copy(surfaceMaps.normalRgb, normalBytes, 0, normalBytes.Length);
            return new IslandPreparedSurfaceMaps(dimension, normalBytes, occlusionBytes);
        }
        finally
        {
            MotuNative.ReleaseSurfaceMaps(ref surfaceMaps);
        }
    }

    internal static IslandPreparedSeaMask PrepareSeaMask(IntPtr handle, int dimension)
    {
        MotuNative.CreateSeaMask(handle, dimension, out var seaMask);
        try
        {
            if (seaMask.handle == IntPtr.Zero
                || seaMask.rgba == IntPtr.Zero
                || seaMask.width != dimension
                || seaMask.height != dimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned an invalid coastal wave mask.");
            }

            var byteCount = checked(dimension * dimension * 4);
            var rgba = new byte[byteCount];
            Marshal.Copy(seaMask.rgba, rgba, 0, rgba.Length);
            return new IslandPreparedSeaMask(dimension, rgba);
        }
        finally
        {
            MotuNative.ReleaseSeaMask(ref seaMask);
        }
    }

    internal static IslandPreparedMesh PrepareSkyDome(float worldSize)
    {
        MotuNative.CreateSkyDome(out var export);
        try
        {
            if (export.handle == IntPtr.Zero
                || export.vertices.data == IntPtr.Zero
                || export.vertices.length == 0
                || export.normals.length != export.vertices.length
                || export.uv.length != export.vertices.length
                || export.triangles.data == IntPtr.Zero
                || export.triangles.length == 0)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned an invalid sky-dome mesh.");
            }
            return IslandMeshInterop.CopyGeneratedMeshData(export, worldSize);
        }
        finally
        {
            MotuNative.ReleaseMesh(ref export);
        }
    }

    internal static IslandPreparedMaterialTextures PrepareMaterialTextures(
        IslandMaterialColours colours,
        int resolution,
        long cacheBudgetBytes = 0L,
        string cacheDirectory = null)
    {
        var cached = IslandMaterialTextureCache.TryLoad(
            colours,
            resolution,
            cacheBudgetBytes,
            cacheDirectory);
        if (cached != null)
        {
            return cached;
        }
        var inputs = colours.ToNative();
        var options = new MotuNative.MaterialBakeOptions
        {
            width = checked((uint)resolution),
            height = checked((uint)resolution),
            normalConvention = 1,
            materialMask = 0x3f,
            reserved = 0,
        };
        var succeeded = MotuNative.BakeMotuMaterialTextures(
            ref inputs,
            ref options,
            out var textures);
        try
        {
            if (succeeded == 0 || textures.handle == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust procedural material library could not bake the requested textures.");
            }
            var result = new IslandPreparedMaterialTextures(
                colours,
                CopyMaterialTexture(textures.dirt, resolution, "dirt"),
                CopyMaterialTexture(textures.forestFloor, resolution, "forest floor"),
                CopyMaterialTexture(textures.rock, resolution, "rock"),
                CopyMaterialTexture(textures.riverBed, resolution, "river bed"),
                CopyMaterialTexture(textures.beach, resolution, "beach"),
                CopyMaterialTexture(textures.fallenStones, resolution, "fallen stones"));
            IslandMaterialTextureCache.TrySave(
                result,
                cacheBudgetBytes,
                cacheDirectory);
            return result;
        }
        finally
        {
            MotuNative.ReleaseMaterialTextureSet(ref textures);
        }
    }

    private static IslandPreparedMaterialTexture CopyMaterialTexture(
        MotuNative.ExportMaterialTexture source,
        int resolution,
        string label)
    {
        var pixels = checked(resolution * resolution);
        if (source.width != resolution
            || source.height != resolution
            || float.IsNaN(source.minimumHeight)
            || float.IsInfinity(source.minimumHeight)
            || float.IsNaN(source.maximumHeight)
            || float.IsInfinity(source.maximumHeight)
            || float.IsNaN(source.baseHeight)
            || float.IsInfinity(source.baseHeight)
            || source.maximumHeight <= source.minimumHeight
            || source.baseHeight < source.minimumHeight
            || source.baseHeight > source.maximumHeight
            || source.albedoRgb.data == IntPtr.Zero
            || source.albedoRgb.length != checked(pixels * 3)
            || source.normalRgb.data == IntPtr.Zero
            || source.normalRgb.length != checked(pixels * 3)
            || source.heightR16.data == IntPtr.Zero
            || source.heightR16.length != checked(pixels * 2)
            || source.occlusion.data == IntPtr.Zero
            || source.occlusion.length != pixels)
        {
            throw new InvalidOperationException(
                $"The Rust generator returned invalid {label} material textures.");
        }

        var albedo = new byte[source.albedoRgb.length];
        var normal = new byte[source.normalRgb.length];
        var height = new byte[source.heightR16.length];
        var occlusion = new byte[source.occlusion.length];
        Marshal.Copy(source.albedoRgb.data, albedo, 0, albedo.Length);
        Marshal.Copy(source.normalRgb.data, normal, 0, normal.Length);
        Marshal.Copy(source.heightR16.data, height, 0, height.Length);
        Marshal.Copy(source.occlusion.data, occlusion, 0, occlusion.Length);
        return new IslandPreparedMaterialTexture(
            resolution,
            resolution,
            source.minimumHeight,
            source.maximumHeight,
            source.baseHeight,
            albedo,
            normal,
            height,
            occlusion);
    }

    private static IslandPreparedColliderHeightMap PrepareColliderHeightMap(
        IntPtr handle,
        float terrainWorldSize)
    {
        var mapPointer = MotuNative.CreateTerrainColliderHeightMap(
            handle,
            TerrainTileStreamer.ColliderSamplesPerTile);
        try
        {
            if (mapPointer == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned a null terrain-collider height map.");
            }
            var native = Marshal.PtrToStructure<MotuNative.ExportHeightMapWithSeaLevel>(
                mapPointer);
            var expectedDimension = checked(
                TerrainTileStreamer.Lod1Resolution
                * (TerrainTileStreamer.ColliderSamplesPerTile - 1)
                + 1);
            if (native.data == IntPtr.Zero
                || native.width != expectedDimension
                || native.height != expectedDimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned invalid terrain-collider height-map data.");
            }

            var heights = new float[checked(native.width * native.height)];
            Marshal.Copy(native.data, heights, 0, heights.Length);
            return new IslandPreparedColliderHeightMap(
                native.width,
                TerrainTileStreamer.ColliderSamplesPerTile,
                heights,
                terrainWorldSize);
        }
        finally
        {
            MotuNative.ReleaseTerrainColliderHeightMap(mapPointer);
        }
    }

    private static IslandPreparedMesh[] PrepareRiverTiles(IntPtr handle, float worldSize)
    {
        var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
        MotuNative.CreateRiverMeshGrid(
            handle,
            ref area,
            TerrainTileStreamer.Lod1Resolution,
            out var export);
        try
        {
            var expectedLength = TerrainTileStreamer.Lod1Resolution
                * TerrainTileStreamer.Lod1Resolution;
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != expectedLength)
            {
                throw new InvalidOperationException(
                    "The Rust river slicer returned an invalid LOD 1 tile batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = IslandMeshInterop.CopyRiverMeshData(
                        nativeMesh,
                        worldSize);
                }
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private static IslandPreparedMesh[] PrepareRiverRockTiles(IntPtr handle, float worldSize)
    {
        var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
        MotuNative.CreateRiverRockMeshGrid(
            handle,
            ref area,
            TerrainTileStreamer.Lod1Resolution,
            out var export);
        try
        {
            var expectedLength = TerrainTileStreamer.Lod1Resolution
                * TerrainTileStreamer.Lod1Resolution;
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != expectedLength)
            {
                throw new InvalidOperationException(
                    "The Rust river-rock slicer returned an invalid LOD 1 tile batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = IslandMeshInterop.CopyRiverMeshData(
                        nativeMesh,
                        worldSize);
                }
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private static IslandPreparedForestData PrepareForestData(
        IntPtr handle,
        float worldSize)
    {
        return new IslandPreparedForestData(
            PrepareForestMeshGrid(handle, worldSize, 2, ForestTileStreamer.Lod2Resolution, false),
            PrepareForestMeshGrid(handle, worldSize, 2, ForestTileStreamer.Lod2Resolution, true),
            PrepareForestMeshGrid(handle, worldSize, 1, ForestTileStreamer.Lod1Resolution, false),
            PrepareForestMeshGrid(handle, worldSize, 1, ForestTileStreamer.Lod1Resolution, true),
            PrepareForestMeshGrid(handle, worldSize, 0, ForestTileStreamer.Lod1Resolution, false),
            PrepareForestMeshGrid(handle, worldSize, 0, ForestTileStreamer.Lod1Resolution, true),
            PrepareForestTrunkColliders(handle, worldSize));
    }

    private static IslandPreparedMesh[] PrepareReedMeshGrid(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateReedMeshGrid(handle, out var export);
        try
        {
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != ReedTileStreamer.TileCount)
            {
                throw new InvalidOperationException(
                    "The Rust reed owner grid returned an invalid batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var native = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (native.vertices.length == 0
                    && native.normals.length == 0
                    && native.triangles.length == 0)
                {
                    if (native.uv.length != 0
                        || native.material.length != 0
                        || native.environment.length != 0)
                    {
                        throw new InvalidOperationException(
                            $"The empty Rust reed tile {index} contains sidecar data.");
                    }
                    continue;
                }
                if (native.vertices.length <= 0
                    || native.vertices.data == IntPtr.Zero
                    || native.normals.length != native.vertices.length
                    || native.normals.data == IntPtr.Zero
                    || native.uv.length != native.vertices.length
                    || native.uv.data == IntPtr.Zero
                    || native.material.length != native.vertices.length
                    || native.material.data == IntPtr.Zero
                    || native.environment.length != native.vertices.length
                    || native.environment.data == IntPtr.Zero
                    || native.triangles.length <= 0
                    || native.triangles.length % 3 != 0
                    || native.triangles.data == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        $"The Rust reed tile {index} has invalid mesh attributes.");
                }
                result[index] = IslandMeshInterop.CopyGeneratedMeshData(
                    native,
                    worldSize);
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    internal static IslandPreparedMesh[] PrepareFernMeshGrid(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateFernMeshGrid(handle, out var export);
        try
        {
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != FernTileStreamer.TileCount)
            {
                throw new InvalidOperationException(
                    "The Rust fern owner grid returned an invalid batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var native = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (native.vertices.length == 0
                    && native.normals.length == 0
                    && native.triangles.length == 0)
                {
                    if (native.uv.length != 0
                        || native.material.length != 0
                        || native.environment.length != 0)
                    {
                        throw new InvalidOperationException(
                            $"The empty Rust fern tile {index} contains sidecar data.");
                    }
                    continue;
                }
                if (native.vertices.length <= 0
                    || native.vertices.data == IntPtr.Zero
                    || native.normals.length != native.vertices.length
                    || native.normals.data == IntPtr.Zero
                    || native.uv.length != native.vertices.length
                    || native.uv.data == IntPtr.Zero
                    || native.material.length != native.vertices.length
                    || native.material.data == IntPtr.Zero
                    || native.environment.length != native.vertices.length
                    || native.environment.data == IntPtr.Zero
                    || native.triangles.length <= 0
                    || native.triangles.length % 3 != 0
                    || native.triangles.data == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        $"The Rust fern tile {index} has invalid mesh attributes.");
                }
                result[index] = IslandMeshInterop.CopyGeneratedMeshData(
                    native,
                    worldSize);
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    internal static IslandPreparedTreeCollider[][] PrepareForestTrunkColliders(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateForestTrunkColliders(handle, out var export);
        try
        {
            if (export.handle == IntPtr.Zero
                || export.length < 0
                || (export.length > 0 && export.data == IntPtr.Zero))
            {
                throw new InvalidOperationException(
                    "The Rust forest trunk-collider export is invalid.");
            }

            var buckets = new List<IslandPreparedTreeCollider>[
                ForestTileStreamer.Lod1TileCount];
            var exportSize = Marshal.SizeOf<MotuNative.ForestTrunkColliderExport>();
            for (var index = 0; index < export.length; index++)
            {
                var native = Marshal.PtrToStructure<MotuNative.ForestTrunkColliderExport>(
                    IntPtr.Add(export.data, index * exportSize));
                if (!IsFiniteNative(native.bottom)
                    || !IsFiniteNative(native.top)
                    || !IsFinite(native.owner.x)
                    || !IsFinite(native.owner.y)
                    || !IsFinite(native.radius)
                    || native.owner.x < 0f
                    || native.owner.x > 1f
                    || native.owner.y < 0f
                    || native.owner.y > 1f
                    || native.radius <= 0f)
                {
                    throw new InvalidOperationException(
                        $"The Rust forest trunk collider {index} is invalid.");
                }
                var bottom = NativePositionToUnity(native.bottom, worldSize);
                var top = NativePositionToUnity(native.top, worldSize);
                var radius = native.radius * worldSize;
                if ((top - bottom).sqrMagnitude <= Mathf.Epsilon || !IsFinite(radius))
                {
                    throw new InvalidOperationException(
                        $"The copied forest trunk collider {index} is degenerate.");
                }
                var tileX = Mathf.Min(
                    Mathf.FloorToInt(native.owner.x * ForestTileStreamer.Lod1Resolution),
                    ForestTileStreamer.Lod1Resolution - 1);
                var tileY = Mathf.Min(
                    Mathf.FloorToInt(native.owner.y * ForestTileStreamer.Lod1Resolution),
                    ForestTileStreamer.Lod1Resolution - 1);
                var tileIndex = tileY * ForestTileStreamer.Lod1Resolution + tileX;
                buckets[tileIndex] ??= new List<IslandPreparedTreeCollider>();
                buckets[tileIndex].Add(new IslandPreparedTreeCollider(bottom, top, radius));
            }

            var result = new IslandPreparedTreeCollider[ForestTileStreamer.Lod1TileCount][];
            for (var tile = 0; tile < result.Length; tile++)
            {
                result[tile] = buckets[tile]?.ToArray()
                    ?? Array.Empty<IslandPreparedTreeCollider>();
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseForestTrunkColliders(ref export);
        }
    }

    private static Vector3 NativePositionToUnity(
        MotuNative.NativeVector3 position,
        float worldSize)
    {
        return new Vector3(
            (position.x - 0.5f) * worldSize,
            position.z * worldSize,
            (position.y - 0.5f) * worldSize);
    }

    private static bool IsFiniteNative(MotuNative.NativeVector3 value)
    {
        return IsFinite(value.x) && IsFinite(value.y) && IsFinite(value.z);
    }

    private static bool IsFinite(float value)
    {
        return !float.IsNaN(value) && !float.IsInfinity(value);
    }

    private static bool IsFinite(Vector3 value)
    {
        return IsFinite(value.x) && IsFinite(value.y) && IsFinite(value.z);
    }

    private static IslandPreparedMesh[] PrepareForestMeshGrid(
        IntPtr handle,
        float worldSize,
        int visualLod,
        int divisions,
        bool wood)
    {
        if (divisions <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(divisions));
        }

        var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
        MotuNative.ExportMeshGrid export;
        if (wood)
        {
            MotuNative.CreateForestWoodMeshGrid(
                handle,
                ref area,
                visualLod,
                divisions,
                out export);
        }
        else
        {
            MotuNative.CreateForestFoliageMeshGrid(
                handle,
                ref area,
                visualLod,
                divisions,
                out export);
        }

        try
        {
            var expectedLength = checked(divisions * divisions);
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != expectedLength)
            {
                throw new InvalidOperationException(
                    $"The Rust forest {(wood ? "wood" : "foliage")} grid returned an invalid "
                    + $"LOD {visualLod} batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle == IntPtr.Zero)
                {
                    ValidateEmptyForestMesh(nativeMesh, visualLod, index);
                    continue;
                }
                if (nativeMesh.vertices.length == 0
                    && nativeMesh.normals.length == 0
                    && nativeMesh.triangles.length == 0)
                {
                    continue;
                }
                ValidateForestNativeMesh(nativeMesh, visualLod, index, wood);
                var prepared = IslandMeshInterop.CopyGeneratedMeshData(
                    nativeMesh,
                    worldSize);
                ValidatePreparedForestMesh(prepared, visualLod, index, wood);
                result[index] = prepared;
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private static void ValidateEmptyForestMesh(
        MotuNative.ExportMesh mesh,
        int visualLod,
        int index)
    {
        if (mesh.vertices.length != 0
            || mesh.normals.length != 0
            || mesh.triangles.length != 0
            || mesh.uv.length != 0
            || mesh.material.length != 0
            || mesh.environment.length != 0)
        {
            throw new InvalidOperationException(
                $"The Rust forest tile {index} at LOD {visualLod} has data without ownership.");
        }
    }

    private static void ValidateForestNativeMesh(
        MotuNative.ExportMesh mesh,
        int visualLod,
        int index,
        bool wood)
    {
        if (mesh.vertices.length <= 0
            || mesh.vertices.data == IntPtr.Zero
            || mesh.normals.length != mesh.vertices.length
            || mesh.normals.data == IntPtr.Zero
            || mesh.triangles.length <= 0
            || mesh.triangles.length % 3 != 0
            || mesh.triangles.data == IntPtr.Zero
            || mesh.uv.length != mesh.vertices.length
            || mesh.uv.data == IntPtr.Zero
            || mesh.material.length != mesh.vertices.length
            || mesh.material.data == IntPtr.Zero
            || mesh.environment.length != 0)
        {
            throw new InvalidOperationException(
                $"The Rust forest {(wood ? "wood" : "foliage")} tile {index} at LOD "
                + $"{visualLod} has invalid mesh attributes.");
        }
    }

    private static void ValidatePreparedForestMesh(
        IslandPreparedMesh mesh,
        int visualLod,
        int index,
        bool wood)
    {
        if (mesh == null
            || mesh.vertices.Length == 0
            || mesh.normals.Length != mesh.vertices.Length
            || mesh.triangles.Length == 0
            || mesh.triangles.Length % 3 != 0
            || mesh.uv.Length != mesh.vertices.Length
            || mesh.environment.Length != 0
            || mesh.material.Length != mesh.vertices.Length)
        {
            throw new InvalidOperationException(
                $"The copied forest {(wood ? "wood" : "foliage")} tile {index} at LOD "
                + $"{visualLod} is invalid.");
        }
        for (var vertex = 0; vertex < mesh.vertices.Length; vertex++)
        {
            if (!IsFinite(mesh.vertices[vertex]) || !IsFinite(mesh.normals[vertex]))
            {
                throw new InvalidOperationException(
                    $"The copied forest tile {index} at LOD {visualLod} contains a non-finite value.");
            }
        }
        for (var triangle = 0; triangle < mesh.triangles.Length; triangle++)
        {
            if (mesh.triangles[triangle] < 0
                || mesh.triangles[triangle] >= mesh.vertices.Length)
            {
                throw new InvalidOperationException(
                    $"The copied forest tile {index} at LOD {visualLod} has an invalid triangle index.");
            }
        }
    }

    private static IslandPreparedWaterfallFoot[] PrepareWaterfallFeet(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateWaterfallFeet(handle, out var export);
        try
        {
            if (export.handle == IntPtr.Zero || export.length < 0)
            {
                throw new InvalidOperationException(
                    "The Rust waterfall-foot export is invalid.");
            }
            if (export.length == 0)
            {
                return Array.Empty<IslandPreparedWaterfallFoot>();
            }
            if (export.data == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust waterfall-foot data is missing.");
            }

            var result = new IslandPreparedWaterfallFoot[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.WaterfallFootExport>();
            for (var index = 0; index < export.length; index++)
            {
                var native = Marshal.PtrToStructure<MotuNative.WaterfallFootExport>(
                    IntPtr.Add(export.data, index * exportSize));
                var position = new Vector3(
                    (native.position.x - 0.5f) * worldSize,
                    native.position.z * worldSize,
                    (native.position.y - 0.5f) * worldSize);
                var direction = new Vector3(
                    native.direction.x,
                    native.direction.z,
                    native.direction.y).normalized;
                result[index] = new IslandPreparedWaterfallFoot(
                    position,
                    direction,
                    native.halfWidth * worldSize,
                    native.drop * worldSize);
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseWaterfallFeet(ref export);
        }
    }

    internal static void ValidateBorrowedArray(
        MotuNative.Vector3Array values,
        string label)
    {
        if (values.length < 0 || (values.length > 0 && values.data == IntPtr.Zero))
        {
            throw new InvalidOperationException(
                $"The native {label} decoration array is invalid.");
        }
    }

}
