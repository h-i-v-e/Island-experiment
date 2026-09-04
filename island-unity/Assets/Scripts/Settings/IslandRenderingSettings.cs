using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandRenderingSettings
{
    [Tooltip("Base dirt colour shared by recipe textures and terrain shader fallbacks.")]
    [SerializeField] private Color dirtColour = new Color(0.09f, 0.055f, 0.026f, 1f);

    [Tooltip("Base stone colour shared by recipe textures and terrain shader fallbacks.")]
    [SerializeField] private Color stoneColour = new Color(0.30f, 0.32f, 0.29f, 1f);

    [Tooltip("Base sand colour shared by the beach recipe and terrain shader fallback.")]
    [SerializeField] private Color sandColour = new Color(0.62f, 0.57f, 0.34f, 1f);

    [Tooltip("Derive deterministic dirt, stone, and sand variations from the island seed before requesting textures.")]
    [SerializeField] private bool randomizeMaterialColours = true;

    [Tooltip("Maximum engine-side colour variation applied per island.")]
    [Range(0f, 0.35f)]
    [SerializeField] private float materialColourVariation = 0.14f;

    [Tooltip("Runtime resolution requested from the Rust procedural material library.")]
    [Range(128, 2048)]
    [SerializeField] private int materialTextureResolution = 1024;

    [Tooltip("Optional terrain material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material terrainMaterial;

    [Tooltip("Optional grass material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material grassMaterial;

    [Tooltip("Optional river material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material riverMaterial;

    [Tooltip("Optional sea material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material seaMaterial;

    [Tooltip("Optional reusable geometric-wave profile for the global deep ocean.")]
    [SerializeField] private OceanWaveProfile oceanWaveProfile;

    [Tooltip("Optional stone and boulder material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material rockMaterial;

    [Tooltip("Optional generated-tree wood material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material treeWoodMaterial;

    [Tooltip("Optional generated-tree foliage material template. A per-island copy is created at runtime.")]
    [SerializeField] private Material treeFoliageMaterial;

    [Tooltip("Optional authored replacement for the generated 3D cliff-detail noise.")]
    [SerializeField] private Texture3D cliffDetailNoise;

    [Tooltip("Optional authored replacement for the generated river and shoreline noise.")]
    [SerializeField] private Texture2D riverNoise;

    [Tooltip("Optional authored replacement for the generated grass patch noise. Red and green control coverage; blue controls broad grass colour variation.")]
    [SerializeField] private Texture2D grassPatchNoise;

    [Tooltip("First colour used by broad grass variation.")]
    [SerializeField] private Color grassColourA = new Color(0.18f, 0.46f, 0.14f, 1f);

    [Tooltip("Second colour used by broad grass variation.")]
    [SerializeField] private Color grassColourB = new Color(0.34f, 0.50f, 0.14f, 1f);

    [Tooltip("World-space repeat size of the broad grass colour noise, in metres. The generated texture produces roughly eight broad patches per repeat.")]
    [Min(1f)]
    [SerializeField] private float grassColourNoiseWorldSizeMetres = 2048f;

    [Tooltip("Brightness multiplier applied to grass rendering.")]
    [Range(0.25f, 3f)]
    [SerializeField] private float grassBrightness = 1.35f;

    [Tooltip("Horizontal world-space direction used by the animated grass wind.")]
    [SerializeField] private Vector2 grassWindDirection = new Vector2(1f, 0.35f);

    [Tooltip("Maximum horizontal bend at the tips of the fur grass, in metres.")]
    [Range(0f, 0.25f)]
    [SerializeField] private float grassWindStrengthMetres = 0.07f;

    [Tooltip("Speed at which coherent gusts travel across the grass, in metres per second.")]
    [Range(0f, 10f)]
    [SerializeField] private float grassWindSpeedMetresPerSecond = 1.8f;

    [Tooltip("World-space size of the broad moving grass gusts, in metres.")]
    [Range(1f, 64f)]
    [SerializeField] private float grassWindGustSizeMetres = 12f;

    [Tooltip("How strongly the moving wind field perturbs fur-grass lighting normals.")]
    [Range(0f, 1f)]
    [SerializeField] private float grassWindNormalStrength = 0.35f;

    [Tooltip("World-space size of coherent sand patches, in metres.")]
    [Min(0.1f)]
    [SerializeField] private float sandPatchSizeMetres = 32f;

    [Tooltip("World-space size of coherent grass patches, in metres.")]
    [Min(0.1f)]
    [SerializeField] private float grassPatchSizeMetres = 32f;

    [Tooltip("Height over which river colour blends into the sea near estuaries, in metres.")]
    [Min(0f)]
    [SerializeField] private float estuaryBlendHeightMetres = 2f;

    [Tooltip("Enable atmospheric haze while walking in first-person mode.")]
    [SerializeField] private bool showDistanceHaze = true;

    [Tooltip("Warm atmospheric colour accumulated by distant first-person views.")]
    [SerializeField] private Color distanceHazeColour = new Color(0.62f, 0.60f, 0.54f, 1f);

    [Tooltip("Density of the exponential-squared first-person haze.")]
    [Range(0.00005f, 0.003f)]
    [SerializeField] private float distanceHazeDensity = 0.00055f;


    [Tooltip("Real-time minutes taken for one complete sunrise-to-sunrise solar cycle.")]
    [Range(0.25f, 240f)]
    [SerializeField] private float sunCycleDurationMinutes = 20f;

    [Tooltip("How many times faster solar time passes at midnight than at noon. One gives a uniform clock while preserving the configured full-cycle duration.")]
    [Range(1f, 20f)]
    [SerializeField] private float midnightToNoonClockRateRatio = 10f;

    [Tooltip("Solar latitude in degrees. Higher absolute values produce a lower noon sun; the sign selects which side of the island it crosses.")]
    [Range(-80f, 80f)]
    [SerializeField] private float sunLatitudeDegrees = -36f;

    [Tooltip("Solar time used when play begins: 6 is sunrise, 12 is noon, and 18 is sunset.")]
    [Range(0f, 24f)]
    [SerializeField] private float startingSolarTimeHours = 8f;

    [Tooltip("Directional-light intensity when the sun is high in the sky.")]
    [Range(0f, 4f)]
    [SerializeField] private float middaySunIntensity = 1.25f;

    [Tooltip("Moon-orbit tilt toward the equator relative to the solar path.")]
    [Range(0f, 45f)]
    [SerializeField] private float moonEquatorOffsetDegrees = 22f;

    [Tooltip("Moon phase when play begins: 0 is new, 0.25 first quarter, 0.5 full, and 0.75 last quarter.")]
    [Range(0f, 1f)]
    [SerializeField] private float startingMoonPhase = 0.5f;

    [Tooltip("Directional-light intensity produced by a full moon after sunset.")]
    [Range(0f, 1f)]
    [SerializeField] private float fullMoonLightIntensity = 0.14f;

    [Tooltip("Fraction of sky cells containing a visible star at night.")]
    [Range(0f, 1f)]
    [SerializeField] private float starDensity = 0.18f;

    [Tooltip("Brightness of the procedural night stars.")]
    [Range(0f, 4f)]
    [SerializeField] private float starBrightness = 1.35f;

    [Tooltip("Apparent radius of procedural stars within their sky cells.")]
    [Range(0.02f, 0.12f)]
    [SerializeField] private float starSize = 0.052f;

    [Tooltip("Show the carved river surface.")]
    [SerializeField] private bool showRivers = true;

    [Tooltip("Show the sea surface.")]
    [SerializeField] private bool showSea = true;

    [Tooltip("Show generated grass shells.")]
    [SerializeField] private bool showGrass = true;

    [Tooltip("Show streamed stones and boulders.")]
    [SerializeField] private bool showRocks = true;

    public Material TerrainMaterial => terrainMaterial;
    public Material GrassMaterial => grassMaterial;
    public Material RiverMaterial => riverMaterial;
    public Material SeaMaterial => seaMaterial;
    public OceanWaveProfile OceanWaveProfile => oceanWaveProfile;
    public Material RockMaterial => rockMaterial;
    public Material TreeWoodMaterial => treeWoodMaterial;
    public Material TreeFoliageMaterial => treeFoliageMaterial;
    public Texture3D CliffDetailNoise => cliffDetailNoise;
    public Texture2D RiverNoise => riverNoise;
    public Texture2D GrassPatchNoise => grassPatchNoise;
    public Color GrassColourA { get => grassColourA; set => grassColourA = value; }
    public Color GrassColourB { get => grassColourB; set => grassColourB = value; }
    public float GrassColourNoiseWorldSizeMetres
    {
        get => Mathf.Max(grassColourNoiseWorldSizeMetres, 1f);
        set => grassColourNoiseWorldSizeMetres = Mathf.Max(value, 1f);
    }
    public float GrassBrightness { get => grassBrightness; set => grassBrightness = Mathf.Clamp(value, 0.25f, 3f); }
    public Vector2 GrassWindDirection
    {
        get => grassWindDirection;
        set => grassWindDirection = value;
    }
    public float GrassWindStrengthMetres
    {
        get => Mathf.Clamp(grassWindStrengthMetres, 0f, 0.25f);
        set => grassWindStrengthMetres = Mathf.Clamp(value, 0f, 0.25f);
    }
    public float GrassWindSpeedMetresPerSecond
    {
        get => Mathf.Clamp(grassWindSpeedMetresPerSecond, 0f, 10f);
        set => grassWindSpeedMetresPerSecond = Mathf.Clamp(value, 0f, 10f);
    }
    public float GrassWindGustSizeMetres
    {
        get => Mathf.Clamp(grassWindGustSizeMetres, 1f, 64f);
        set => grassWindGustSizeMetres = Mathf.Clamp(value, 1f, 64f);
    }
    public float GrassWindNormalStrength
    {
        get => Mathf.Clamp01(grassWindNormalStrength);
        set => grassWindNormalStrength = Mathf.Clamp01(value);
    }
    internal float SandPatchSizeMetres => Mathf.Max(sandPatchSizeMetres, 0.1f);
    internal float GrassPatchSizeMetres => Mathf.Max(grassPatchSizeMetres, 0.1f);
    internal float EstuaryBlendHeightMetres => Mathf.Max(estuaryBlendHeightMetres, 0f);
    public bool ShowDistanceHaze
    {
        get => showDistanceHaze;
        set => showDistanceHaze = value;
    }
    public Color DistanceHazeColour
    {
        get => distanceHazeColour;
        set => distanceHazeColour = value;
    }
    public float DistanceHazeDensity
    {
        get => Mathf.Clamp(distanceHazeDensity, 0.00005f, 0.003f);
        set => distanceHazeDensity = Mathf.Clamp(value, 0.00005f, 0.003f);
    }
    public float SunCycleDurationMinutes
    {
        get => Mathf.Clamp(sunCycleDurationMinutes, 0.25f, 240f);
        set => sunCycleDurationMinutes = Mathf.Clamp(value, 0.25f, 240f);
    }
    public float MidnightToNoonClockRateRatio
    {
        get => midnightToNoonClockRateRatio > 0f
            ? Mathf.Clamp(midnightToNoonClockRateRatio, 1f, 20f)
            : 10f;
        set => midnightToNoonClockRateRatio = Mathf.Clamp(value, 1f, 20f);
    }
    public float SunLatitudeDegrees
    {
        get => Mathf.Clamp(sunLatitudeDegrees, -80f, 80f);
        set => sunLatitudeDegrees = Mathf.Clamp(value, -80f, 80f);
    }
    public float StartingSolarTimeHours
    {
        get => Mathf.Repeat(startingSolarTimeHours, 24f);
        set => startingSolarTimeHours = Mathf.Repeat(value, 24f);
    }
    public float MiddaySunIntensity
    {
        get => Mathf.Clamp(middaySunIntensity, 0f, 4f);
        set => middaySunIntensity = Mathf.Clamp(value, 0f, 4f);
    }
    public float MoonEquatorOffsetDegrees
    {
        get => Mathf.Clamp(moonEquatorOffsetDegrees, 0f, 45f);
        set => moonEquatorOffsetDegrees = Mathf.Clamp(value, 0f, 45f);
    }
    public float StartingMoonPhase
    {
        get => Mathf.Repeat(startingMoonPhase, 1f);
        set => startingMoonPhase = Mathf.Repeat(value, 1f);
    }
    public float FullMoonLightIntensity
    {
        get => Mathf.Clamp01(fullMoonLightIntensity);
        set => fullMoonLightIntensity = Mathf.Clamp01(value);
    }
    public float StarDensity
    {
        get => Mathf.Clamp01(starDensity);
        set => starDensity = Mathf.Clamp01(value);
    }
    public float StarBrightness
    {
        get => Mathf.Clamp(starBrightness, 0f, 4f);
        set => starBrightness = Mathf.Clamp(value, 0f, 4f);
    }
    public float StarSize
    {
        get => starSize > 0f ? Mathf.Clamp(starSize, 0.02f, 0.12f) : 0.052f;
        set => starSize = Mathf.Clamp(value, 0.02f, 0.12f);
    }
    public bool ShowRivers { get => showRivers; set => showRivers = value; }
    public bool ShowSea { get => showSea; set => showSea = value; }
    public bool ShowGrass { get => showGrass; set => showGrass = value; }
    public bool ShowRocks { get => showRocks; set => showRocks = value; }
    internal int MaterialTextureResolution => Mathf.Clamp(
        Mathf.ClosestPowerOfTwo(materialTextureResolution),
        128,
        2048);

    internal IslandMaterialColours SelectMaterialColours(int islandSeed)
    {
        var dirt = ClampLinearColour(dirtColour);
        var stone = ClampLinearColour(stoneColour);
        var sand = ClampLinearColour(sandColour);
        if (!randomizeMaterialColours || materialColourVariation <= 0f)
        {
            return new IslandMaterialColours(dirt, stone, sand);
        }

        var random = new System.Random(unchecked(islandSeed * 1103515245 + 12345));
        dirt = VaryLinearColour(dirt, random, materialColourVariation, 0.45f);
        stone = VaryLinearColour(stone, random, materialColourVariation * 0.72f, 0.18f);
        sand = VaryLinearColour(sand, random, materialColourVariation * 0.65f, 0.30f);
        return new IslandMaterialColours(dirt, stone, sand);
    }

    private static Color VaryLinearColour(
        Color colour,
        System.Random random,
        float amount,
        float warmth)
    {
        var brightness = 1f + ((float)random.NextDouble() * 2f - 1f) * amount;
        var temperature = ((float)random.NextDouble() * 2f - 1f) * amount * warmth;
        var greenShift = ((float)random.NextDouble() * 2f - 1f) * amount * 0.18f;
        return ClampLinearColour(new Color(
            colour.r * brightness * (1f + temperature),
            colour.g * brightness * (1f + greenShift),
            colour.b * brightness * (1f - temperature),
            1f));
    }

    private static Color ClampLinearColour(Color colour)
    {
        return new Color(
            Mathf.Clamp01(colour.r),
            Mathf.Clamp01(colour.g),
            Mathf.Clamp01(colour.b),
            1f);
    }

    internal void AssignMaterialTemplates(
        Material terrain,
        Material grass,
        Material river,
        Material sea,
        Material rock,
        Material treeWood = null,
        Material treeFoliage = null)
    {
        terrainMaterial = terrain;
        grassMaterial = grass;
        riverMaterial = river;
        seaMaterial = sea;
        rockMaterial = rock;
        treeWoodMaterial = treeWood;
        treeFoliageMaterial = treeFoliage;
    }
}
