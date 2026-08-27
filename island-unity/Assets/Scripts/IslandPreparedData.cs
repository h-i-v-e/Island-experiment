using System;
using UnityEngine;

internal sealed class IslandPreparedMesh
{
    internal readonly Vector3[] vertices;
    internal readonly Vector3[] normals;
    internal readonly int[] triangles;
    internal readonly Vector2[] uv;
    internal readonly Color[] material;
    internal readonly Vector2[] environment;

    internal IslandPreparedMesh(
        Vector3[] vertices,
        Vector3[] normals,
        int[] triangles,
        Vector2[] uv,
        Color[] material,
        Vector2[] environment)
    {
        this.vertices = vertices;
        this.normals = normals;
        this.triangles = triangles;
        this.uv = uv;
        this.material = material;
        this.environment = environment;
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

internal sealed class IslandPreparedForestData
{
    internal readonly IslandPreparedMesh[] lod2FoliageTiles;
    internal readonly IslandPreparedMesh[] lod1FoliageTiles;
    internal readonly IslandPreparedMesh[] lod1WoodTiles;
    internal readonly IslandPreparedMesh[] lod0FoliageTiles;
    internal readonly IslandPreparedMesh[] lod0WoodTiles;

    internal IslandPreparedForestData(
        IslandPreparedMesh[] lod2FoliageTiles,
        IslandPreparedMesh[] lod1FoliageTiles,
        IslandPreparedMesh[] lod1WoodTiles,
        IslandPreparedMesh[] lod0FoliageTiles,
        IslandPreparedMesh[] lod0WoodTiles)
    {
        ValidateLength(lod2FoliageTiles, ForestTileStreamer.Lod2TileCount);
        ValidateLength(lod1FoliageTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod1WoodTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod0FoliageTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod0WoodTiles, ForestTileStreamer.Lod1TileCount);
        this.lod2FoliageTiles = lod2FoliageTiles;
        this.lod1FoliageTiles = lod1FoliageTiles;
        this.lod1WoodTiles = lod1WoodTiles;
        this.lod0FoliageTiles = lod0FoliageTiles;
        this.lod0WoodTiles = lod0WoodTiles;
    }

    private static void ValidateLength(IslandPreparedMesh[] tiles, int expectedLength)
    {
        if (tiles == null || tiles.Length != expectedLength)
        {
            throw new InvalidOperationException(
                $"The prepared forest batch must contain {expectedLength} tiles.");
        }
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
    internal readonly IslandPreparedMesh[] riverRockTiles;
    internal readonly IslandPreparedForestData forest;
    internal readonly IslandPreparedRiverEmitter[] riverEmitters;
    internal readonly IslandPreparedColliderHeightMap colliderHeightMap;

    internal IslandPreparedData(
        IntPtr handle,
        IslandPreparedSurfaceMaps surfaceMaps,
        IslandPreparedSeaMask seaMask,
        IslandPreparedMesh[] overviewTiles,
        IslandPreparedMesh[] riverTiles,
        IslandPreparedMesh[] riverRockTiles,
        IslandPreparedForestData forest,
        IslandPreparedRiverEmitter[] riverEmitters,
        IslandPreparedColliderHeightMap colliderHeightMap)
    {
        this.handle = new NativeIslandHandle(handle);
        this.surfaceMaps = surfaceMaps;
        this.seaMask = seaMask;
        this.overviewTiles = overviewTiles;
        this.riverTiles = riverTiles;
        this.riverRockTiles = riverRockTiles;
        this.forest = forest ?? throw new ArgumentNullException(nameof(forest));
        this.riverEmitters = riverEmitters;
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
