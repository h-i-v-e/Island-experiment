using UnityEngine;

public sealed partial class IslandGenerator
{
    internal static Texture3D CreateCliffNoiseTexture()
    {
        var texture = new Texture3D(
            CliffNoiseDimension,
            CliffNoiseDimension,
            CliffNoiseDimension,
            TextureFormat.RGBA32,
            false)
        {
            name = "Cliff coherent noise",
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Repeat,
        };
        var pixels = new Color[CliffNoiseDimension * CliffNoiseDimension * CliffNoiseDimension];
        var latticeScale = CliffNoiseLatticePeriod / (float)CliffNoiseDimension;
        for (var z = 0; z < CliffNoiseDimension; z++)
        {
            for (var y = 0; y < CliffNoiseDimension; y++)
            {
                for (var x = 0; x < CliffNoiseDimension; x++)
                {
                    var sampleX = (x + 0.5f) * latticeScale;
                    var sampleY = (y + 0.5f) * latticeScale;
                    var sampleZ = (z + 0.5f) * latticeScale;
                    var index = x + CliffNoiseDimension * (y + CliffNoiseDimension * z);
                    pixels[index] = new Color(
                        PeriodicValueNoise(sampleX, sampleY, sampleZ, 0xA341316Cu),
                        PeriodicValueNoise(sampleX, sampleY, sampleZ, 0xC8013EA4u),
                        PeriodicValueNoise(sampleX, sampleY, sampleZ, 0xAD90777Du),
                        1f);
                }
            }
        }
        texture.SetPixels(pixels);
        texture.Apply(false, true);
        return texture;
    }

    internal static Texture2D CreateRiverNoiseTexture()
    {
        var texture = new Texture2D(
            RiverNoiseDimension,
            RiverNoiseDimension,
            TextureFormat.RGBA32,
            false,
            true)
        {
            name = "River coherent flow noise",
            filterMode = FilterMode.Bilinear,
            wrapMode = TextureWrapMode.Repeat,
        };
        var pixels = new Color[RiverNoiseDimension * RiverNoiseDimension];
        var latticeScale = RiverNoiseLatticePeriod / (float)RiverNoiseDimension;
        for (var y = 0; y < RiverNoiseDimension; y++)
        {
            for (var x = 0; x < RiverNoiseDimension; x++)
            {
                var sampleX = (x + 0.5f) * latticeScale;
                var sampleY = (y + 0.5f) * latticeScale;
                var index = x + RiverNoiseDimension * y;
                pixels[index] = new Color(
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0x9E3779B9u,
                        RiverNoiseLatticePeriod),
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0xD1B54A35u,
                        RiverNoiseLatticePeriod),
                    0f,
                    1f);
            }
        }
        texture.SetPixels(pixels);
        texture.Apply(false, true);
        return texture;
    }

    private static Texture2D CreateGrassPatchNoiseTexture()
    {
        var texture = new Texture2D(
            GrassPatchNoiseDimension,
            GrassPatchNoiseDimension,
            TextureFormat.RGBA32,
            true,
            true)
        {
            name = "Grass coverage and broad colour noise",
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Repeat,
            anisoLevel = 2,
        };
        var pixels = new Color[GrassPatchNoiseDimension * GrassPatchNoiseDimension];
        var latticeScale = GrassPatchNoiseLatticePeriod
            / (float)GrassPatchNoiseDimension;
        var colourLatticeScale = GrassColourNoiseLatticePeriod
            / (float)GrassPatchNoiseDimension;
        for (var y = 0; y < GrassPatchNoiseDimension; y++)
        {
            for (var x = 0; x < GrassPatchNoiseDimension; x++)
            {
                var sampleX = (x + 0.5f) * latticeScale;
                var sampleY = (y + 0.5f) * latticeScale;
                var colourSampleX = (x + 0.5f) * colourLatticeScale;
                var colourSampleY = (y + 0.5f) * colourLatticeScale;
                var index = x + GrassPatchNoiseDimension * y;
                pixels[index] = new Color(
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0xB5297A4Du,
                        GrassPatchNoiseLatticePeriod),
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0x68E31DA4u,
                        GrassPatchNoiseLatticePeriod),
                    PeriodicNoise2D(
                        colourSampleX,
                        colourSampleY,
                        0x1B56C4E9u,
                        GrassColourNoiseLatticePeriod),
                    1f);
            }
        }
        texture.SetPixels(pixels);
        texture.Apply(true, true);
        return texture;
    }

    private static float PeriodicNoise2D(float x, float y, uint seed, int period)
    {
        var latticeX = Mathf.FloorToInt(x);
        var latticeY = Mathf.FloorToInt(y);
        var x0 = latticeX % period;
        var y0 = latticeY % period;
        var x1 = (x0 + 1) % period;
        var y1 = (y0 + 1) % period;
        var fadeX = QuinticFade(x - latticeX);
        var fadeY = QuinticFade(y - latticeY);
        var near = Mathf.Lerp(
            LatticeNoise(x0, y0, 0, seed),
            LatticeNoise(x1, y0, 0, seed),
            fadeX);
        var far = Mathf.Lerp(
            LatticeNoise(x0, y1, 0, seed),
            LatticeNoise(x1, y1, 0, seed),
            fadeX);
        return Mathf.Lerp(near, far, fadeY);
    }

    private static float PeriodicValueNoise(float x, float y, float z, uint seed)
    {
        var latticeX = Mathf.FloorToInt(x);
        var latticeY = Mathf.FloorToInt(y);
        var latticeZ = Mathf.FloorToInt(z);
        var x0 = latticeX % CliffNoiseLatticePeriod;
        var y0 = latticeY % CliffNoiseLatticePeriod;
        var z0 = latticeZ % CliffNoiseLatticePeriod;
        var x1 = (x0 + 1) % CliffNoiseLatticePeriod;
        var y1 = (y0 + 1) % CliffNoiseLatticePeriod;
        var z1 = (z0 + 1) % CliffNoiseLatticePeriod;

        var fadeX = QuinticFade(x - latticeX);
        var fadeY = QuinticFade(y - latticeY);
        var fadeZ = QuinticFade(z - latticeZ);
        var lowerNear = Mathf.Lerp(
            LatticeNoise(x0, y0, z0, seed),
            LatticeNoise(x1, y0, z0, seed),
            fadeX);
        var lowerFar = Mathf.Lerp(
            LatticeNoise(x0, y1, z0, seed),
            LatticeNoise(x1, y1, z0, seed),
            fadeX);
        var upperNear = Mathf.Lerp(
            LatticeNoise(x0, y0, z1, seed),
            LatticeNoise(x1, y0, z1, seed),
            fadeX);
        var upperFar = Mathf.Lerp(
            LatticeNoise(x0, y1, z1, seed),
            LatticeNoise(x1, y1, z1, seed),
            fadeX);
        return Mathf.Lerp(
            Mathf.Lerp(lowerNear, lowerFar, fadeY),
            Mathf.Lerp(upperNear, upperFar, fadeY),
            fadeZ);
    }

    private static float LatticeNoise(int x, int y, int z, uint seed)
    {
        unchecked
        {
            var value = (uint)x * 0x8DA6B343u;
            value ^= (uint)y * 0xD8163841u;
            value ^= (uint)z * 0xCB1AB31Fu;
            return HashNoise(value ^ seed);
        }
    }

    private static float QuinticFade(float value)
    {
        return value * value * value * (value * (value * 6f - 15f) + 10f);
    }

    private static float HashNoise(uint value)
    {
        value ^= value >> 16;
        value *= 0x7FEB352Du;
        value ^= value >> 15;
        value *= 0x846CA68Bu;
        value ^= value >> 16;
        return (value & 0x00FFFFFFu) / 16777215f;
    }

}
