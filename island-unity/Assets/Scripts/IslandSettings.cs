using System;
using UnityEngine;

[Serializable]
public sealed class IslandGenerationSettings
{
    private const float NativeIslandWorldMetres = 2000f;

    [Tooltip("Generate this island automatically when the level enters Play Mode.")]
    [SerializeField] private bool generateOnStart = true;

    [Tooltip("Deterministic seed used by the native island generator.")]
    [SerializeField] private int seed = 666;

    [Tooltip("Width and length of the generated island square in metres.")]
    [Min(100f)]
    [SerializeField] private float worldSizeMetres = 2000f;

    [Tooltip("Maximum generated terrain height above sea level in metres.")]
    [Min(1f)]
    [SerializeField] private float maximumHeightMetres = 400f;

    [Tooltip("Normalized proportion of the generated map intended to be water. Regenerate to apply.")]
    [Range(0.60f, 0.95f)]
    [SerializeField] private float waterRatio = 0.95f;

    [Tooltip("Multiplier applied to inland terrain slopes. Regenerate to apply.")]
    [Range(0.2f, 4f)]
    [SerializeField] private float inlandSlopeMultiplier = 1.3f;

    [Tooltip("Multiplier applied to coastal terrain slopes. Regenerate to apply.")]
    [Range(0.1f, 4f)]
    [SerializeField] private float coastalSlopeMultiplier = 1f;

    [Tooltip("Hydraulic erosion strength. Regenerate to apply.")]
    [Range(0f, 8f)]
    [SerializeField] private float hydraulicErosionStrength = 1f;

    [Tooltip("Sediment deposition strength. Regenerate to apply.")]
    [Range(0f, 4f)]
    [SerializeField] private float sedimentDepositionStrength = 1.5f;

    [Tooltip("Maximum slope on which hydraulic sediment is deposited, in degrees.")]
    [Range(1f, 45f)]
    [SerializeField] private float depositionMaximumSlopeDegrees = 12f;

    public bool GenerateOnStart => generateOnStart;
    public int Seed { get => seed; set => seed = value; }
    public float WorldSizeMetres => Mathf.Max(worldSizeMetres, 100f);
    public float MaximumHeightMetres => Mathf.Clamp(
        maximumHeightMetres,
        1f,
        WorldSizeMetres * 0.5f);
    internal float MaximumHeightNormalized => MaximumHeightMetres / WorldSizeMetres;
    internal float WaterRatio => Mathf.Clamp(waterRatio, 0.60f, 0.95f);
    internal float InlandSlopeMultiplier => Mathf.Clamp(inlandSlopeMultiplier, 0.2f, 4f);
    internal float CoastalSlopeMultiplier => Mathf.Clamp(coastalSlopeMultiplier, 0.1f, 4f);
    internal float HydraulicErosionStrength => Mathf.Clamp(hydraulicErosionStrength, 0f, 8f);
    internal float SedimentDepositionStrength => Mathf.Clamp(sedimentDepositionStrength, 0f, 4f);
    internal float DepositionMaximumSlopeDegrees => Mathf.Clamp(
        depositionMaximumSlopeDegrees,
        1f,
        45f);

    internal MotuNative.Options ToNativeOptions(IslandRiverSettings rivers)
    {
        return new MotuNative.Options
        {
            maxZ = MaximumHeightNormalized,
            waterRatio = WaterRatio,
            slopeMultiplier = InlandSlopeMultiplier,
            coastalSlopeMultiplier = CoastalSlopeMultiplier,
            hydraulicErosionStrength = HydraulicErosionStrength,
            hydraulicDepositionStrength = SedimentDepositionStrength,
            hydraulicDepositionSlopeDegrees = DepositionMaximumSlopeDegrees,
            riverSourceCatchmentHectares = rivers.SourceCatchmentHectares,
            riverSourceSteepMultiplier = rivers.SteepSourceMultiplier,
            riverSourceElevationBoost = rivers.SourceElevationBoost,
            riverSourceWidthMetres = ToNativeRiverMetres(rivers.SourceWidthMetres),
            riverMaximumWidthMetres = ToNativeRiverMetres(rivers.MaximumWidthMetres),
            riverSourceDepthMetres = ToNativeRiverMetres(rivers.SourceDepthMetres),
            riverMaximumDepthMetres = ToNativeRiverMetres(rivers.MaximumDepthMetres),
        };
    }

    internal MotuNative.ForestOptions ToNativeForestOptions(IslandForestSettings forest)
    {
        return new MotuNative.ForestOptions
        {
            patchSizeMetres = ToNativeForestMetres(forest.ForestPatchSizeMetres),
            noiseThreshold = forest.ForestNoiseThreshold,
            noiseOctaves = 4,
            snowlineMetres = ToNativeForestMetres(forest.SnowlineMetres),
            prototypeCount = (byte)forest.ForestPrototypeCount,
            minimumScale = forest.MinimumTreeScale,
            maximumScale = forest.MaximumTreeScale,
        };
    }

    private float ToNativeRiverMetres(float metres)
    {
        return metres * NativeIslandWorldMetres / WorldSizeMetres;
    }

    private float ToNativeForestMetres(float metres)
    {
        return metres * NativeIslandWorldMetres / WorldSizeMetres;
    }
}

[Serializable]
public sealed class IslandRiverSettings
{
    [Tooltip("Minimum upstream catchment required for a river source, in hectares. Higher values create fewer rivers.")]
    [Range(0.01f, 10f)]
    [SerializeField] private float sourceCatchmentHectares = 0.05f;

    [Tooltip("Bias towards selecting steep river sources. Regenerate to apply.")]
    [Range(1f, 8f)]
    [SerializeField] private float steepSourceMultiplier = 4f;

    [Tooltip("Bias towards selecting high river sources. Regenerate to apply.")]
    [Range(0f, 20f)]
    [SerializeField] private float sourceElevationBoost = 9f;

    [Tooltip("Full river width at low-flow sources, in metres.")]
    [Min(0.25f)]
    [SerializeField] private float sourceWidthMetres = 2f;

    [Tooltip("Full river width at maximum accumulated flow, in metres.")]
    [Min(0.25f)]
    [SerializeField] private float maximumWidthMetres = 14f;

    [Tooltip("Nominal river depth at low-flow sources, in metres.")]
    [Min(0.05f)]
    [SerializeField] private float sourceDepthMetres = 0.35f;

    [Tooltip("Nominal river depth at maximum accumulated flow, in metres.")]
    [Min(0.05f)]
    [SerializeField] private float maximumDepthMetres = 2f;

    [Tooltip("Maximum change in river direction, in degrees, used for rough-water emitters.")]
    [Range(1f, 90f)]
    [SerializeField] private float roughWaterSharpnessDegrees = 35f;

    [Tooltip("Minimum spacing between rough-water emitters in metres.")]
    [Min(0.1f)]
    [SerializeField] private float roughWaterSpacingMetres = 1.5f;

    internal float SourceCatchmentHectares => Mathf.Clamp(sourceCatchmentHectares, 0.01f, 10f);
    internal float SteepSourceMultiplier => Mathf.Clamp(steepSourceMultiplier, 1f, 8f);
    internal float SourceElevationBoost => Mathf.Clamp(sourceElevationBoost, 0f, 20f);
    internal float SourceWidthMetres => Mathf.Max(sourceWidthMetres, 0.25f);
    internal float MaximumWidthMetres => Mathf.Max(maximumWidthMetres, SourceWidthMetres);
    internal float SourceDepthMetres => Mathf.Max(sourceDepthMetres, 0.05f);
    internal float MaximumDepthMetres => Mathf.Max(maximumDepthMetres, SourceDepthMetres);
    internal float RoughWaterSharpnessDegrees => Mathf.Clamp(
        roughWaterSharpnessDegrees,
        1f,
        90f);
    internal float RoughWaterSpacingMetres => Mathf.Max(roughWaterSpacingMetres, 0.1f);
}

[Serializable]
public sealed class IslandForestSettings
{
    [Tooltip("Show streamed forest foliage and wood without regenerating the island.")]
    [SerializeField] private bool showForests = true;

    [Tooltip("Physical size of the coherent forest noise patches. Regenerate to apply.")]
    [Min(32f)]
    [SerializeField] private float forestPatchSizeMetres = 200f;

    [Tooltip("Normalized coherent-noise coverage threshold. Higher values produce fewer trees. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float forestNoiseThreshold = 0.62f;

    [Tooltip("Physical height at and above which forest placement is rejected. Regenerate to apply.")]
    [Min(0.01f)]
    [SerializeField] private float snowlineMetres = 100f;

    [Tooltip("Number of deterministic tree prototypes generated by Rust. Regenerate to apply.")]
    [Range(1, 64)]
    [SerializeField] private int forestPrototypeCount = 8;

    [Tooltip("Minimum uniform scale for accepted trees. Regenerate to apply.")]
    [Min(0.01f)]
    [SerializeField] private float minimumTreeScale = 1f;

    [Tooltip("Maximum uniform scale for accepted trees. Regenerate to apply.")]
    [Min(0.01f)]
    [SerializeField] private float maximumTreeScale = 2f;

    public bool ShowForests { get => showForests; set => showForests = value; }
    internal float ForestPatchSizeMetres => Mathf.Max(forestPatchSizeMetres, 32f);
    internal float ForestNoiseThreshold => Mathf.Clamp01(forestNoiseThreshold);
    internal float SnowlineMetres => Mathf.Max(snowlineMetres, 0.01f);
    internal int ForestPrototypeCount => Mathf.Clamp(forestPrototypeCount, 1, 64);
    internal float MinimumTreeScale => Mathf.Max(minimumTreeScale, 0.01f);
    internal float MaximumTreeScale => Mathf.Max(maximumTreeScale, MinimumTreeScale);
}

[Serializable]
public sealed class IslandStreamingSettings
{
    [Tooltip("Player or camera Transform that drives terrain detail, collision, rocks, grass, and river effects.")]
    [SerializeField] private Transform target;

    public Transform Target { get => target; set => target = value; }
}

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

    [Tooltip("Optional level-owned sunlight used for grass shading.")]
    [SerializeField] private Light sunlight;

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
    public Light Sunlight { get => sunlight; internal set => sunlight = value; }
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

[Serializable]
public sealed class IslandDecorationSettings
{
    [Tooltip("Tree prefabs reserved for the vegetation placement phase.")]
    [SerializeField] private GameObject[] treePrefabs = Array.Empty<GameObject>();

    [Tooltip("Plant and shrub prefabs reserved for the vegetation placement phase.")]
    [SerializeField] private GameObject[] plantPrefabs = Array.Empty<GameObject>();

    public GameObject[] TreePrefabs => treePrefabs;
    public GameObject[] PlantPrefabs => plantPrefabs;
}

[Serializable]
public sealed class IslandDebugSettings
{
    [Tooltip("Draw the generated terrain mesh edges in black over its normal materials.")]
    [SerializeField] private bool showMeshEdges;

    [Tooltip(
        "Key used in Play Mode to toggle the terrain mesh-edge overlay. "
        + "Set to None to disable the shortcut.")]
    [SerializeField] private KeyCode toggleMeshEdgesKey = KeyCode.M;

    [Tooltip("Draw tree triangle edges over the normal wood and foliage materials.")]
    [SerializeField] private bool showTreeMeshEdges;

    [Tooltip(
        "Key used in Play Mode to toggle the tree-only mesh-edge overlay. "
        + "Set to None to disable the shortcut.")]
    [SerializeField] private KeyCode toggleTreeMeshEdgesKey = KeyCode.N;

    [Tooltip("Display a smoothed frame-rate counter in the top-right corner.")]
    [SerializeField] private bool showFrameRate;

    [Tooltip(
        "Key used in Play Mode to toggle the frame-rate counter. "
        + "Set to None to disable the shortcut.")]
    [SerializeField] private KeyCode toggleFrameRateKey = KeyCode.F;

    [Tooltip("Display rough-water emitter debug markers.")]
    [SerializeField] private bool showRoughWaterEmitters;

    public bool ShowMeshEdges { get => showMeshEdges; set => showMeshEdges = value; }
    public KeyCode ToggleMeshEdgesKey => toggleMeshEdgesKey;
    public bool ShowTreeMeshEdges { get => showTreeMeshEdges; set => showTreeMeshEdges = value; }
    public KeyCode ToggleTreeMeshEdgesKey => toggleTreeMeshEdgesKey;
    public bool ShowFrameRate { get => showFrameRate; set => showFrameRate = value; }
    public KeyCode ToggleFrameRateKey => toggleFrameRateKey;
    public bool ShowRoughWaterEmitters { get => showRoughWaterEmitters; set => showRoughWaterEmitters = value; }
}
