using System;
using UnityEngine;

internal sealed class IslandPreparedMesh
{
    internal readonly Vector3[] vertices;
    internal readonly Vector3[] normals;
    internal readonly int[] triangles;
    internal readonly Vector2[] uv;
    internal readonly Color[] material;

    internal IslandPreparedMesh(
        Vector3[] vertices,
        Vector3[] normals,
        int[] triangles,
        Vector2[] uv,
        Color[] material)
    {
        this.vertices = vertices;
        this.normals = normals;
        this.triangles = triangles;
        this.uv = uv;
        this.material = material;
    }
}

internal readonly struct IslandPreparedRiverEmitter
{
    internal readonly Vector3 position;
    internal readonly Vector3 direction;
    internal readonly float strength;

    internal IslandPreparedRiverEmitter(Vector3 position, Vector3 direction, float strength)
    {
        this.position = position;
        this.direction = direction;
        this.strength = strength;
    }
}

internal readonly struct IslandPreparedRockDecoration
{
    internal readonly int sourceIndex;
    internal readonly uint appearanceId;
    internal readonly Vector3 position;
    internal readonly Vector3 normal;
    internal readonly bool isBoulder;
    internal readonly int prototypeIndex;
    internal readonly Vector3 scale;
    internal readonly Quaternion rotation;
    internal readonly Color tint;
    internal readonly float embedDepth;

    internal IslandPreparedRockDecoration(
        int sourceIndex,
        uint appearanceId,
        Vector3 position,
        Vector3 normal,
        bool isBoulder,
        int prototypeIndex,
        Vector3 scale,
        Quaternion rotation,
        Color tint,
        float embedDepth)
    {
        this.sourceIndex = sourceIndex;
        this.appearanceId = appearanceId;
        this.position = position;
        this.normal = normal;
        this.isBoulder = isBoulder;
        this.prototypeIndex = prototypeIndex;
        this.scale = scale;
        this.rotation = rotation;
        this.tint = tint;
        this.embedDepth = embedDepth;
    }
}

internal sealed class IslandPreparedSurfaceMaps
{
    internal readonly int dimension;
    internal readonly byte[] normalRgb;
    internal readonly byte[] occlusion;

    internal IslandPreparedSurfaceMaps(int dimension, byte[] normalRgb, byte[] occlusion)
    {
        this.dimension = dimension;
        this.normalRgb = normalRgb;
        this.occlusion = occlusion;
    }
}

internal sealed class IslandPreparedSeaMask
{
    internal readonly int dimension;
    internal readonly byte[] rg;

    internal IslandPreparedSeaMask(int dimension, byte[] rg)
    {
        this.dimension = dimension;
        this.rg = rg;
    }
}

internal sealed class IslandPreparedColliderHeightMap
{
    private const float VerticalSafetyMarginMetres = 1f;
    private readonly float[] normalizedHeights;

    internal readonly int dimension;
    internal readonly int samplesPerTile;
    internal readonly float verticalOrigin;
    internal readonly float verticalSize;
    internal readonly float minimumHeight;
    internal readonly float maximumHeight;

    internal IslandPreparedColliderHeightMap(
        int mapDimension,
        int tileSamples,
        float[] heights,
        float terrainWorldSize)
    {
        var expectedDimension = checked(
            TerrainTileStreamer.Lod1Resolution * (tileSamples - 1) + 1);
        if (mapDimension != expectedDimension
            || heights == null
            || heights.Length != checked(mapDimension * mapDimension))
        {
            throw new InvalidOperationException(
                "The terrain-collider height map dimensions are invalid.");
        }

        dimension = mapDimension;
        samplesPerTile = tileSamples;
        normalizedHeights = heights;
        var minimum = float.PositiveInfinity;
        var maximum = float.NegativeInfinity;
        for (var index = 0; index < heights.Length; index++)
        {
            var height = heights[index] * terrainWorldSize;
            if (float.IsNaN(height) || float.IsInfinity(height))
            {
                throw new InvalidOperationException(
                    "The terrain-collider height map contains a non-finite sample.");
            }
            heights[index] = height;
            minimum = Math.Min(minimum, height);
            maximum = Math.Max(maximum, height);
        }

        minimumHeight = minimum;
        maximumHeight = maximum;
        verticalOrigin = minimum - VerticalSafetyMarginMetres;
        verticalSize = Math.Max(
            maximum - minimum + VerticalSafetyMarginMetres * 2f,
            VerticalSafetyMarginMetres * 2f);
        for (var index = 0; index < heights.Length; index++)
        {
            heights[index] = (heights[index] - verticalOrigin) / verticalSize;
        }
    }

    internal float[,] CopyTileHeights(Vector2Int tile)
    {
        if (tile.x < 0
            || tile.y < 0
            || tile.x >= TerrainTileStreamer.Lod1Resolution
            || tile.y >= TerrainTileStreamer.Lod1Resolution)
        {
            throw new ArgumentOutOfRangeException(nameof(tile));
        }

        var result = new float[samplesPerTile, samplesPerTile];
        var intervalsPerTile = samplesPerTile - 1;
        var sourceX = tile.x * intervalsPerTile;
        var sourceY = tile.y * intervalsPerTile;
        for (var localY = 0; localY < samplesPerTile; localY++)
        {
            var sourceOffset = (sourceY + localY) * dimension + sourceX;
            for (var localX = 0; localX < samplesPerTile; localX++)
            {
                result[localY, localX] = normalizedHeights[sourceOffset + localX];
            }
        }
        return result;
    }

    internal float WorldHeightAt(int sampleX, int sampleY)
    {
        if (sampleX < 0 || sampleY < 0 || sampleX >= dimension || sampleY >= dimension)
        {
            throw new ArgumentOutOfRangeException();
        }
        return verticalOrigin
            + normalizedHeights[sampleY * dimension + sampleX] * verticalSize;
    }
}

internal sealed class IslandPreparedData : IDisposable
{
    internal NativeIslandHandle handle;
    internal readonly IslandPreparedSurfaceMaps surfaceMaps;
    internal readonly IslandPreparedSeaMask seaMask;
    internal readonly IslandPreparedMesh[] overviewTiles;
    internal readonly IslandPreparedMesh[] riverTiles;
    internal readonly IslandPreparedMesh riverBedDebugMesh;
    internal readonly IslandPreparedMesh waterfallFaceTerrainDebugMesh;
    internal readonly IslandPreparedMesh waterfallPlaneDebugMesh;
    internal readonly IslandPreparedMesh waterfallLipPlaneDebugMesh;
    internal readonly IslandPreparedRiverEmitter[] riverEmitters;
    internal readonly IslandPreparedRockDecoration[] rocks;
    internal readonly IslandPreparedColliderHeightMap colliderHeightMap;

    internal IslandPreparedData(
        IntPtr handle,
        IslandPreparedSurfaceMaps surfaceMaps,
        IslandPreparedSeaMask seaMask,
        IslandPreparedMesh[] overviewTiles,
        IslandPreparedMesh[] riverTiles,
        IslandPreparedMesh riverBedDebugMesh,
        IslandPreparedMesh waterfallFaceTerrainDebugMesh,
        IslandPreparedMesh waterfallPlaneDebugMesh,
        IslandPreparedMesh waterfallLipPlaneDebugMesh,
        IslandPreparedRiverEmitter[] riverEmitters,
        IslandPreparedRockDecoration[] rocks,
        IslandPreparedColliderHeightMap colliderHeightMap)
    {
        this.handle = new NativeIslandHandle(handle);
        this.surfaceMaps = surfaceMaps;
        this.seaMask = seaMask;
        this.overviewTiles = overviewTiles;
        this.riverTiles = riverTiles;
        this.riverBedDebugMesh = riverBedDebugMesh;
        this.waterfallFaceTerrainDebugMesh = waterfallFaceTerrainDebugMesh;
        this.waterfallPlaneDebugMesh = waterfallPlaneDebugMesh;
        this.waterfallLipPlaneDebugMesh = waterfallLipPlaneDebugMesh;
        this.riverEmitters = riverEmitters;
        this.rocks = rocks;
        this.colliderHeightMap = colliderHeightMap;
    }

    internal NativeIslandHandle TakeHandle()
    {
        var result = handle;
        handle = null;
        return result;
    }

    public void Dispose()
    {
        handle?.Dispose();
        handle = null;
    }
}
