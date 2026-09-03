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

internal readonly struct IslandPreparedWaterfallFoot
{
    internal readonly Vector3 position;
    internal readonly Vector3 direction;
    internal readonly float halfWidth;
    internal readonly float drop;

    internal IslandPreparedWaterfallFoot(
        Vector3 position,
        Vector3 direction,
        float halfWidth,
        float drop)
    {
        this.position = position;
        this.direction = direction;
        this.halfWidth = halfWidth;
        this.drop = drop;
    }
}

internal readonly struct IslandPreparedTreeCollider
{
    internal readonly Vector3 bottom;
    internal readonly Vector3 top;
    internal readonly float radius;

    internal IslandPreparedTreeCollider(Vector3 bottom, Vector3 top, float radius)
    {
        this.bottom = bottom;
        this.top = top;
        this.radius = radius;
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

internal readonly struct IslandMaterialColours
{
    internal readonly Color dirt;
    internal readonly Color stone;
    internal readonly Color sand;

    internal IslandMaterialColours(Color dirt, Color stone, Color sand)
    {
        this.dirt = new Color(dirt.r, dirt.g, dirt.b, 1f);
        this.stone = new Color(stone.r, stone.g, stone.b, 1f);
        this.sand = new Color(sand.r, sand.g, sand.b, 1f);
    }

    internal MotuNative.MaterialInputs ToNative()
    {
        // Unity's authored material colours are display-space values in this
        // Gamma colour-space project. The procedural texture library accepts
        // linear RGB and performs the sRGB encoding when it produces albedo
        // bytes, so decode exactly once at the engine/library boundary. This
        // keeps the baked texture colour equal to the shader fallback colour.
        var linearDirt = dirt.linear;
        var linearStone = stone.linear;
        var linearSand = sand.linear;
        return new MotuNative.MaterialInputs
        {
            dirtRed = linearDirt.r,
            dirtGreen = linearDirt.g,
            dirtBlue = linearDirt.b,
            stoneRed = linearStone.r,
            stoneGreen = linearStone.g,
            stoneBlue = linearStone.b,
            sandRed = linearSand.r,
            sandGreen = linearSand.g,
            sandBlue = linearSand.b,
        };
    }
}

internal sealed class IslandPreparedMaterialTexture
{
    internal readonly int width;
    internal readonly int height;
    internal readonly float minimumHeight;
    internal readonly float maximumHeight;
    internal readonly float baseHeight;
    internal readonly byte[] albedoRgb;
    internal readonly byte[] normalRgb;
    internal readonly byte[] heightR16;
    internal readonly byte[] occlusion;

    internal IslandPreparedMaterialTexture(
        int width,
        int height,
        float minimumHeight,
        float maximumHeight,
        float baseHeight,
        byte[] albedoRgb,
        byte[] normalRgb,
        byte[] heightR16,
        byte[] occlusion)
    {
        var pixels = checked(width * height);
        if (width <= 0
            || height <= 0
            || float.IsNaN(minimumHeight)
            || float.IsInfinity(minimumHeight)
            || float.IsNaN(maximumHeight)
            || float.IsInfinity(maximumHeight)
            || float.IsNaN(baseHeight)
            || float.IsInfinity(baseHeight)
            || maximumHeight <= minimumHeight
            || baseHeight < minimumHeight
            || baseHeight > maximumHeight
            || albedoRgb == null
            || albedoRgb.Length != checked(pixels * 3)
            || normalRgb == null
            || normalRgb.Length != checked(pixels * 3)
            || heightR16 == null
            || heightR16.Length != checked(pixels * 2)
            || occlusion == null
            || occlusion.Length != pixels)
        {
            throw new InvalidOperationException(
                "The native procedural material texture data is invalid.");
        }
        this.width = width;
        this.height = height;
        this.minimumHeight = minimumHeight;
        this.maximumHeight = maximumHeight;
        this.baseHeight = baseHeight;
        this.albedoRgb = albedoRgb;
        this.normalRgb = normalRgb;
        this.heightR16 = heightR16;
        this.occlusion = occlusion;
    }

    internal float NormalizedBaseHeight =>
        Mathf.Clamp01((baseHeight - minimumHeight) / (maximumHeight - minimumHeight));
}

internal sealed class IslandPreparedMaterialTextures
{
    internal readonly IslandMaterialColours colours;
    internal readonly IslandPreparedMaterialTexture dirt;
    internal readonly IslandPreparedMaterialTexture forestFloor;
    internal readonly IslandPreparedMaterialTexture rock;
    internal readonly IslandPreparedMaterialTexture riverBed;
    internal readonly IslandPreparedMaterialTexture beach;
    internal readonly IslandPreparedMaterialTexture fallenStones;

    internal IslandPreparedMaterialTextures(
        IslandMaterialColours colours,
        IslandPreparedMaterialTexture dirt,
        IslandPreparedMaterialTexture forestFloor,
        IslandPreparedMaterialTexture rock,
        IslandPreparedMaterialTexture riverBed,
        IslandPreparedMaterialTexture beach,
        IslandPreparedMaterialTexture fallenStones)
    {
        this.colours = colours;
        this.dirt = dirt ?? throw new ArgumentNullException(nameof(dirt));
        this.forestFloor = forestFloor ?? throw new ArgumentNullException(nameof(forestFloor));
        this.rock = rock ?? throw new ArgumentNullException(nameof(rock));
        this.riverBed = riverBed ?? throw new ArgumentNullException(nameof(riverBed));
        this.beach = beach ?? throw new ArgumentNullException(nameof(beach));
        this.fallenStones = fallenStones ?? throw new ArgumentNullException(nameof(fallenStones));
    }
}

internal sealed class IslandPreparedForestData
{
    internal readonly IslandPreparedMesh[] lod2FoliageTiles;
    internal readonly IslandPreparedMesh[] lod2WoodTiles;
    internal readonly IslandPreparedMesh[] lod1FoliageTiles;
    internal readonly IslandPreparedMesh[] lod1WoodTiles;
    internal readonly IslandPreparedMesh[] lod0FoliageTiles;
    internal readonly IslandPreparedMesh[] lod0WoodTiles;
    internal readonly IslandPreparedTreeCollider[][] lod0TrunkColliderTiles;

    internal IslandPreparedForestData(
        IslandPreparedMesh[] lod2FoliageTiles,
        IslandPreparedMesh[] lod2WoodTiles,
        IslandPreparedMesh[] lod1FoliageTiles,
        IslandPreparedMesh[] lod1WoodTiles,
        IslandPreparedMesh[] lod0FoliageTiles,
        IslandPreparedMesh[] lod0WoodTiles,
        IslandPreparedTreeCollider[][] lod0TrunkColliderTiles)
    {
        ValidateLength(lod2FoliageTiles, ForestTileStreamer.Lod2TileCount);
        ValidateLength(lod2WoodTiles, ForestTileStreamer.Lod2TileCount);
        ValidateLength(lod1FoliageTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod1WoodTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod0FoliageTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod0WoodTiles, ForestTileStreamer.Lod1TileCount);
        ValidateLength(lod0TrunkColliderTiles, ForestTileStreamer.Lod1TileCount);
        this.lod2FoliageTiles = lod2FoliageTiles;
        this.lod2WoodTiles = lod2WoodTiles;
        this.lod1FoliageTiles = lod1FoliageTiles;
        this.lod1WoodTiles = lod1WoodTiles;
        this.lod0FoliageTiles = lod0FoliageTiles;
        this.lod0WoodTiles = lod0WoodTiles;
        this.lod0TrunkColliderTiles = lod0TrunkColliderTiles;
    }

    private static void ValidateLength<T>(T[] tiles, int expectedLength)
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
    internal readonly bool loadedFromSnapshot;
    internal readonly IslandPreparedSurfaceMaps surfaceMaps;
    internal readonly IslandPreparedSeaMask seaMask;
    internal readonly IslandPreparedMesh[] overviewTiles;
    internal readonly IslandPreparedMesh[] riverTiles;
    internal readonly IslandPreparedMesh[] riverRockTiles;
    internal readonly IslandPreparedForestData forest;
    internal readonly IslandPreparedMesh[] reedTiles;
    internal readonly IslandPreparedMesh[] fernTiles;
    internal readonly IslandPreparedWaterfallFoot[] waterfallFeet;
    internal readonly IslandPreparedColliderHeightMap colliderHeightMap;
    internal readonly IslandPreparedMaterialTextures materialTextures;

    internal IslandPreparedData(
        IntPtr handle,
        bool loadedFromSnapshot,
        IslandPreparedSurfaceMaps surfaceMaps,
        IslandPreparedSeaMask seaMask,
        IslandPreparedMesh[] overviewTiles,
        IslandPreparedMesh[] riverTiles,
        IslandPreparedMesh[] riverRockTiles,
        IslandPreparedForestData forest,
        IslandPreparedMesh[] reedTiles,
        IslandPreparedMesh[] fernTiles,
        IslandPreparedWaterfallFoot[] waterfallFeet,
        IslandPreparedColliderHeightMap colliderHeightMap,
        IslandPreparedMaterialTextures materialTextures)
    {
        this.handle = new NativeIslandHandle(handle);
        this.loadedFromSnapshot = loadedFromSnapshot;
        this.surfaceMaps = surfaceMaps;
        this.seaMask = seaMask;
        this.overviewTiles = overviewTiles;
        this.riverTiles = riverTiles;
        this.riverRockTiles = riverRockTiles;
        this.forest = forest ?? throw new ArgumentNullException(nameof(forest));
        if (reedTiles == null || reedTiles.Length != ReedTileStreamer.TileCount)
        {
            throw new ArgumentException("The prepared reed owner grid is invalid.", nameof(reedTiles));
        }
        this.reedTiles = reedTiles;
        if (fernTiles == null || fernTiles.Length != FernTileStreamer.TileCount)
        {
            throw new ArgumentException("The prepared fern owner grid is invalid.", nameof(fernTiles));
        }
        this.fernTiles = fernTiles;
        this.waterfallFeet = waterfallFeet;
        this.colliderHeightMap = colliderHeightMap;
        this.materialTextures = materialTextures
            ?? throw new ArgumentNullException(nameof(materialTextures));
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
