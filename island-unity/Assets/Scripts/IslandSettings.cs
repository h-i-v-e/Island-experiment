using System;
using UnityEngine;
using UnityEngine.Serialization;

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

    internal MotuNative.ReedOptions ToNativeReedOptions(IslandReedSettings reeds)
    {
        return new MotuNative.ReedOptions
        {
            bankWidthMetres = ToNativeForestMetres(reeds.BankWidthMetres),
            patchSizeMetres = ToNativeForestMetres(reeds.PatchSizeMetres),
            coverageThreshold = reeds.CoverageThreshold,
            spacingMetres = ToNativeForestMetres(reeds.SpacingMetres),
            rushRatio = reeds.RushRatio,
            minimumHeightMetres = ToNativeForestMetres(reeds.MinimumHeightMetres),
            maximumHeightMetres = ToNativeForestMetres(reeds.MaximumHeightMetres),
            maximumSlopeDegrees = reeds.MaximumSlopeDegrees,
        };
    }

    internal MotuNative.FernOptions ToNativeFernOptions(IslandFernSettings ferns)
    {
        return new MotuNative.FernOptions
        {
            barkClearanceMetres = ToNativeForestMetres(ferns.BarkClearanceMetres),
            outerRadiusMetres = ToNativeForestMetres(ferns.OuterRadiusMetres),
            spacingMetres = ToNativeForestMetres(ferns.SpacingMetres),
            patchSizeMetres = ToNativeForestMetres(ferns.PatchSizeMetres),
            coverageThreshold = ferns.CoverageThreshold,
            minimumLengthMetres = ToNativeForestMetres(ferns.MinimumLengthMetres),
            maximumLengthMetres = ToNativeForestMetres(ferns.MaximumLengthMetres),
            maximumSlopeDegrees = ferns.MaximumSlopeDegrees,
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

    internal float SourceCatchmentHectares => Mathf.Clamp(sourceCatchmentHectares, 0.01f, 10f);
    internal float SteepSourceMultiplier => Mathf.Clamp(steepSourceMultiplier, 1f, 8f);
    internal float SourceElevationBoost => Mathf.Clamp(sourceElevationBoost, 0f, 20f);
    internal float SourceWidthMetres => Mathf.Max(sourceWidthMetres, 0.25f);
    internal float MaximumWidthMetres => Mathf.Max(maximumWidthMetres, SourceWidthMetres);
    internal float SourceDepthMetres => Mathf.Max(sourceDepthMetres, 0.05f);
    internal float MaximumDepthMetres => Mathf.Max(maximumDepthMetres, SourceDepthMetres);
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
public sealed class IslandReedSettings
{
    [Tooltip("Show LOD 0 riverbank reeds and rushes without regenerating the island.")]
    [SerializeField] private bool showReeds = true;

    [Tooltip("Maximum distance across the immediate dry river bank. Regenerate to apply.")]
    [Range(0.25f, 10f)]
    [SerializeField] private float bankWidthMetres = 0.8f;

    [Tooltip("Physical size of coherent reed patches. Regenerate to apply.")]
    [Range(2f, 50f)]
    [SerializeField] private float patchSizeMetres = 8f;

    [Tooltip("Higher thresholds create fewer and more separated reed patches. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float coverageThreshold = 0.18f;

    [Tooltip("Minimum spacing between clump roots. Regenerate to apply.")]
    [Range(0.2f, 3f)]
    [SerializeField] private float spacingMetres = 0.36f;

    [Tooltip("Fraction of the outer bank strip occupied by shorter rushes. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float rushRatio = 0.45f;

    [Tooltip("Minimum clump height. Regenerate to apply.")]
    [Range(0.2f, 3f)]
    [SerializeField] private float minimumHeightMetres = 0.65f;

    [Tooltip("Maximum clump height. Regenerate to apply.")]
    [Range(0.2f, 4f)]
    [SerializeField] private float maximumHeightMetres = 2.1f;

    [Tooltip("Steepest bank on which reeds may be planted. Regenerate to apply.")]
    [Range(0f, 60f)]
    [SerializeField] private float maximumSlopeDegrees = 32f;

    [Tooltip("Dark green-brown used at the base and for rushes.")]
    [SerializeField] private Color baseColour = new Color(0.12f, 0.24f, 0.055f, 1f);

    [Tooltip("Sunlit yellow-green used toward reed tips.")]
    [SerializeField] private Color tipColour = new Color(0.38f, 0.48f, 0.09f, 1f);

    [Tooltip("Multiplier applied to the shared grass wind field.")]
    [Range(0f, 8f)]
    [SerializeField] private float windStrength = 3f;

    public bool ShowReeds { get => showReeds; set => showReeds = value; }
    internal float BankWidthMetres => Mathf.Clamp(bankWidthMetres, 0.25f, 10f);
    internal float PatchSizeMetres => Mathf.Clamp(patchSizeMetres, 2f, 50f);
    internal float CoverageThreshold => Mathf.Clamp01(coverageThreshold);
    internal float SpacingMetres => Mathf.Clamp(spacingMetres, 0.2f, 3f);
    internal float RushRatio => Mathf.Clamp01(rushRatio);
    internal float MinimumHeightMetres => Mathf.Clamp(minimumHeightMetres, 0.2f, 3f);
    internal float MaximumHeightMetres => Mathf.Max(maximumHeightMetres, MinimumHeightMetres);
    internal float MaximumSlopeDegrees => Mathf.Clamp(maximumSlopeDegrees, 0f, 60f);
    internal Color BaseColour => baseColour;
    internal Color TipColour => tipColour;
    internal float WindStrength => Mathf.Clamp(windStrength, 0f, 8f);
}

[Serializable]
public sealed class IslandFernSettings
{
    [Tooltip("Show LOD 0 ferns around tree trunks without regenerating the island.")]
    [SerializeField] private bool showFerns = true;

    [Tooltip("Clear ground retained between bark and the first fern roots. Regenerate to apply.")]
    [Range(0f, 2f)]
    [SerializeField] private float barkClearanceMetres = 0.18f;

    [Tooltip("Outer radius of each trunk's fern bed. Regenerate to apply.")]
    [Range(0.25f, 8f)]
    [SerializeField] private float outerRadiusMetres = 1.65f;

    [Tooltip("Minimum spacing between fern crowns. Regenerate to apply.")]
    [Range(0.2f, 4f)]
    [SerializeField] private float spacingMetres = 0.58f;

    [Tooltip("Physical size of coherent understory patches. Regenerate to apply.")]
    [Range(2f, 50f)]
    [SerializeField] private float patchSizeMetres = 12f;

    [Tooltip("Higher thresholds leave more tree trunks without ferns. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float coverageThreshold = 0.28f;

    [Tooltip("Minimum fern frond length. Regenerate to apply.")]
    [Range(0.2f, 2f)]
    [SerializeField] private float minimumLengthMetres = 0.45f;

    [Tooltip("Maximum fern frond length. Regenerate to apply.")]
    [Range(0.2f, 3f)]
    [SerializeField] private float maximumLengthMetres = 1.15f;

    [Tooltip("Steepest forest ground on which ferns may grow. Regenerate to apply.")]
    [Range(0f, 60f)]
    [SerializeField] private float maximumSlopeDegrees = 34f;

    [SerializeField] private Color baseColour = new Color(0.055f, 0.18f, 0.045f, 1f);
    [SerializeField] private Color tipColour = new Color(0.24f, 0.48f, 0.12f, 1f);

    [Tooltip("Multiplier applied to the shared grass wind field.")]
    [Range(0f, 8f)]
    [SerializeField] private float windStrength = 1.8f;

    public bool ShowFerns { get => showFerns; set => showFerns = value; }
    internal float BarkClearanceMetres => Mathf.Clamp(barkClearanceMetres, 0f, 2f);
    internal float OuterRadiusMetres => Mathf.Clamp(outerRadiusMetres, 0.25f, 8f);
    internal float SpacingMetres => Mathf.Clamp(spacingMetres, 0.2f, 4f);
    internal float PatchSizeMetres => Mathf.Clamp(patchSizeMetres, 2f, 50f);
    internal float CoverageThreshold => Mathf.Clamp01(coverageThreshold);
    internal float MinimumLengthMetres => Mathf.Clamp(minimumLengthMetres, 0.2f, 2f);
    internal float MaximumLengthMetres => Mathf.Max(maximumLengthMetres, MinimumLengthMetres);
    internal float MaximumSlopeDegrees => Mathf.Clamp(maximumSlopeDegrees, 0f, 60f);
    internal Color BaseColour => baseColour;
    internal Color TipColour => tipColour;
    internal float WindStrength => Mathf.Clamp(windStrength, 0f, 8f);
}

[Serializable]
public sealed class IslandStreamingSettings
{
    [Tooltip("Player or camera Transform that drives terrain detail, collision, rocks, grass, and river effects.")]
    [SerializeField] private Transform target;

    public Transform Target { get => target; set => target = value; }
}

[Serializable]
public sealed class IslandCloudSettings
{
    [SerializeField] private bool enabled = true;

    [Tooltip("Seed mixed with the island seed when generating the seamless weather field.")]
    [SerializeField] private int seed = 173;

    [Tooltip("Power-of-two resolution of the portable packed weather map.")]
    [Range(32, 1024)]
    [SerializeField] private int weatherMapResolution = 256;

    [Tooltip("Fraction of the weather field occupied by clouds.")]
    [Range(0f, 1f)]
    [SerializeField] private float coverage = 0.48f;

    [Tooltip("Optical thickness of formed clouds.")]
    [Range(0f, 8f)]
    [SerializeField] private float density = 2.1f;

    [Tooltip("Height of the cloud layer above sea level in metres.")]
    [Range(50f, 1800f)]
    [SerializeField] private float altitudeMetres = 650f;

    [Tooltip("World-space width and depth represented by one weather-map repeat.")]
    [Range(100f, 8000f)]
    [SerializeField] private float worldSizeMetres = 2200f;

    [Tooltip("Size of the broad, non-repeating cloud pattern relative to the weather map.")]
    [Range(2f, 16f)]
    [SerializeField] private float broadNoiseScale = 6f;

    [Tooltip("How strongly the broad cloud pattern breaks up repeated weather-map features.")]
    [Range(0f, 1f)]
    [SerializeField] private float broadNoiseStrength = 0.65f;

    [Tooltip("Influence of the medium-scale weather channel.")]
    [Range(0f, 1f)]
    [SerializeField] private float detailStrength = 0.52f;

    [Tooltip("Amount by which fine noise erodes cloud edges.")]
    [Range(0f, 1f)]
    [SerializeField] private float erosionStrength = 0.38f;

    [Tooltip("Horizontal direction in which the cloud field travels.")]
    [SerializeField] private Vector2 windDirection = new Vector2(1f, 0.25f);

    [Tooltip("Cloud travel speed in metres per second.")]
    [Range(0f, 100f)]
    [SerializeField] private float windSpeedMetresPerSecond = 9f;

    [SerializeField] private Color dayColour = new Color(0.92f, 0.94f, 0.96f, 1f);
    [SerializeField] private Color sunsetColour = new Color(0.92f, 0.48f, 0.24f, 1f);
    [SerializeField] private Color nightColour = new Color(0.035f, 0.055f, 0.10f, 1f);

    [Tooltip("Maximum attenuation of direct light beneath dense clouds.")]
    [Range(0f, 1f)]
    [SerializeField] private float shadowStrength = 0.72f;

    [Tooltip("Weaker attenuation applied to ambient illumination beneath dense clouds.")]
    [Range(0f, 0.5f)]
    [SerializeField] private float ambientShadowStrength = 0.12f;

    [Tooltip("Strength with which cloud density obscures celestial discs and halos.")]
    [Range(0f, 2f)]
    [SerializeField] private float celestialObscurationStrength = 1f;

    [Tooltip("Source elevation below which long cloud-shadow projections fade out.")]
    [Range(0.01f, 0.35f)]
    [SerializeField] private float lowElevationShadowFade = 0.09f;

    public bool Enabled { get => enabled; set => enabled = value; }
    public int Seed { get => seed; set => seed = value; }
    public int WeatherMapResolution
    {
        get => Mathf.Clamp(Mathf.ClosestPowerOfTwo(weatherMapResolution), 32, 1024);
        set => weatherMapResolution = Mathf.Clamp(Mathf.ClosestPowerOfTwo(value), 32, 1024);
    }
    public float Coverage { get => Mathf.Clamp01(coverage); set => coverage = Mathf.Clamp01(value); }
    public float Density { get => Mathf.Clamp(density, 0f, 8f); set => density = Mathf.Clamp(value, 0f, 8f); }
    public float AltitudeMetres { get => Mathf.Clamp(altitudeMetres, 50f, 1800f); set => altitudeMetres = Mathf.Clamp(value, 50f, 1800f); }
    public float WorldSizeMetres { get => Mathf.Clamp(worldSizeMetres, 100f, 8000f); set => worldSizeMetres = Mathf.Clamp(value, 100f, 8000f); }
    public float BroadNoiseScale { get => Mathf.Clamp(broadNoiseScale, 2f, 16f); set => broadNoiseScale = Mathf.Clamp(value, 2f, 16f); }
    public float BroadNoiseStrength { get => Mathf.Clamp01(broadNoiseStrength); set => broadNoiseStrength = Mathf.Clamp01(value); }
    public float DetailStrength { get => Mathf.Clamp01(detailStrength); set => detailStrength = Mathf.Clamp01(value); }
    public float ErosionStrength { get => Mathf.Clamp01(erosionStrength); set => erosionStrength = Mathf.Clamp01(value); }
    public Vector2 WindDirection { get => windDirection; set => windDirection = value; }
    public float WindSpeedMetresPerSecond { get => Mathf.Clamp(windSpeedMetresPerSecond, 0f, 100f); set => windSpeedMetresPerSecond = Mathf.Clamp(value, 0f, 100f); }
    public Color DayColour { get => dayColour; set => dayColour = value; }
    public Color SunsetColour { get => sunsetColour; set => sunsetColour = value; }
    public Color NightColour { get => nightColour; set => nightColour = value; }
    public float ShadowStrength { get => Mathf.Clamp01(shadowStrength); set => shadowStrength = Mathf.Clamp01(value); }
    public float AmbientShadowStrength { get => Mathf.Clamp(ambientShadowStrength, 0f, 0.5f); set => ambientShadowStrength = Mathf.Clamp(value, 0f, 0.5f); }
    public float CelestialObscurationStrength { get => Mathf.Clamp(celestialObscurationStrength, 0f, 2f); set => celestialObscurationStrength = Mathf.Clamp(value, 0f, 2f); }
    public float LowElevationShadowFade { get => Mathf.Clamp(lowElevationShadowFade, 0.01f, 0.35f); set => lowElevationShadowFade = Mathf.Clamp(value, 0.01f, 0.35f); }
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

    [Tooltip("Display authoritative waterfall-foot fog-volume markers.")]
    [FormerlySerializedAs("showRoughWaterEmitters")]
    [SerializeField] private bool showWaterfallFeet;

    public bool ShowMeshEdges { get => showMeshEdges; set => showMeshEdges = value; }
    public KeyCode ToggleMeshEdgesKey => toggleMeshEdgesKey;
    public bool ShowTreeMeshEdges { get => showTreeMeshEdges; set => showTreeMeshEdges = value; }
    public KeyCode ToggleTreeMeshEdgesKey => toggleTreeMeshEdgesKey;
    public bool ShowFrameRate { get => showFrameRate; set => showFrameRate = value; }
    public KeyCode ToggleFrameRateKey => toggleFrameRateKey;
    public bool ShowWaterfallFeet { get => showWaterfallFeet; set => showWaterfallFeet = value; }
}
