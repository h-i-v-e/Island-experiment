using System;
using UnityEngine;
using UnityEngine.Serialization;


[Serializable]
public sealed class IslandGenerationSettings
{
    private const float NativeIslandWorldMetres = 2000f;

    [Tooltip("Generate this island automatically when the level enters Play Mode.")]
    [SerializeField] private bool generateOnStart = true;

    [Tooltip("Restore previously generated islands from the persistent on-disk snapshot cache.")]
    [SerializeField] private bool useSnapshotCache = true;

    [Tooltip("Maximum shared generated-island snapshot cache size in GiB. Old snapshots are removed first.")]
    [Range(1, 64)]
    [SerializeField] private int snapshotCacheBudgetGiB = 8;

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

    [Tooltip("Spatial frequency of the broad noise shared by the original height field and hardness. Higher values can split the land into smaller island groups. Regenerate to apply.")]
    [Range(0.1f, 32f)]
    [SerializeField] private float continentalNoiseFrequency = 2.2f;

    [Tooltip("Strength of the broad noise relative to the radial island falloff. Regenerate to apply.")]
    [Range(0f, 4f)]
    [SerializeField] private float continentalNoiseStrength = 0.78f;

    [Tooltip("Spatial frequency of the fine noise shared by the original height field and hardness. Higher values produce smaller features. Regenerate to apply.")]
    [Range(0.1f, 64f)]
    [SerializeField] private float detailNoiseFrequency = 12f;

    [Tooltip("Strength of the fine noise relative to the broad layer and radial island falloff. Regenerate to apply.")]
    [Range(0f, 4f)]
    [SerializeField] private float detailNoiseStrength = 0.22f;

    [Tooltip("Signed offset applied after the water-ratio sea level is selected. Negative values submerge land and isolate high points into archipelagos. Regenerate to apply.")]
    [Range(-2f, 2f)]
    [SerializeField] private float landMassOffset;

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
    internal bool UseSnapshotCache => useSnapshotCache;
    internal long SnapshotCacheBudgetBytes =>
        (long)Mathf.Clamp(snapshotCacheBudgetGiB, 1, 64) * 1024L * 1024L * 1024L;
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
    internal float ContinentalNoiseFrequency => Mathf.Clamp(continentalNoiseFrequency, 0.1f, 128f);
    internal float ContinentalNoiseStrength => Mathf.Clamp(continentalNoiseStrength, 0f, 4f);
    internal float DetailNoiseFrequency => Mathf.Clamp(detailNoiseFrequency, 0.1f, 128f);
    internal float DetailNoiseStrength => Mathf.Clamp(detailNoiseStrength, 0f, 4f);
    internal float LandMassOffset => Mathf.Clamp(landMassOffset, -2f, 2f);
    internal float HydraulicErosionStrength => Mathf.Clamp(hydraulicErosionStrength, 0f, 8f);
    internal float SedimentDepositionStrength => Mathf.Clamp(sedimentDepositionStrength, 0f, 4f);
    internal float DepositionMaximumSlopeDegrees => Mathf.Clamp(
        depositionMaximumSlopeDegrees,
        1f,
        45f);

    internal void ApplyDeterministicVariation(
        int islandSeed,
        IslandParameterVariationSettings variation)
    {
        if (variation == null || !variation.Enabled)
        {
            return;
        }

        var random = new System.Random(unchecked(islandSeed * 1664525 + 1013904223));
        maximumHeightMetres *= Scale(random, variation.MaximumHeightVariation);
        waterRatio += Signed(random) * variation.WaterRatioVariation;
        inlandSlopeMultiplier *= Scale(random, variation.SlopeVariation);
        coastalSlopeMultiplier *= Scale(random, variation.SlopeVariation);
        continentalNoiseFrequency *= Scale(random, variation.NoiseFrequencyVariation);
        detailNoiseFrequency *= Scale(random, variation.NoiseFrequencyVariation);
        continentalNoiseStrength *= Scale(random, variation.NoiseStrengthVariation);
        detailNoiseStrength *= Scale(random, variation.NoiseStrengthVariation);
        landMassOffset += Signed(random) * variation.LandMassOffsetVariation;
        hydraulicErosionStrength *= Scale(random, variation.ErosionVariation);
        sedimentDepositionStrength *= Scale(random, variation.ErosionVariation);

        maximumHeightMetres = MaximumHeightMetres;
        waterRatio = WaterRatio;
        inlandSlopeMultiplier = InlandSlopeMultiplier;
        coastalSlopeMultiplier = CoastalSlopeMultiplier;
        continentalNoiseFrequency = ContinentalNoiseFrequency;
        detailNoiseFrequency = DetailNoiseFrequency;
        continentalNoiseStrength = ContinentalNoiseStrength;
        detailNoiseStrength = DetailNoiseStrength;
        landMassOffset = LandMassOffset;
        hydraulicErosionStrength = HydraulicErosionStrength;
        sedimentDepositionStrength = SedimentDepositionStrength;
    }

    private static float Scale(System.Random random, float variation)
    {
        return 1f + Signed(random) * Mathf.Clamp01(variation);
    }

    private static float Signed(System.Random random)
    {
        return (float)random.NextDouble() * 2f - 1f;
    }

    internal MotuNative.Options ToNativeOptions(IslandRiverSettings rivers)
    {
        return new MotuNative.Options
        {
            maxZ = MaximumHeightNormalized,
            waterRatio = WaterRatio,
            slopeMultiplier = InlandSlopeMultiplier,
            coastalSlopeMultiplier = CoastalSlopeMultiplier,
            continentalNoiseFrequency = ContinentalNoiseFrequency,
            detailNoiseFrequency = DetailNoiseFrequency,
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
            continentalNoiseStrength = ContinentalNoiseStrength,
            detailNoiseStrength = DetailNoiseStrength,
            landMassOffset = LandMassOffset,
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
