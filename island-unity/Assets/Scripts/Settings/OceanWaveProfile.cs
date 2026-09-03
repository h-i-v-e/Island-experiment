using System;
using UnityEngine;

[Serializable]
public struct OceanWaveComponent
{
    [Tooltip("Horizontal travel direction in world X/Z. It is normalized at runtime.")]
    [SerializeField] private Vector2 direction;
    [Tooltip("Distance between crests. Larger values make broader ocean swell.")]
    [Min(0.25f)] [SerializeField] private float wavelengthMetres;
    [Tooltip("Maximum vertical contribution of this wave before coherent height variation.")]
    [Min(0f)] [SerializeField] private float amplitudeMetres;
    [Tooltip("Speed at which this wave travels through the world.")]
    [Min(0f)] [SerializeField] private float speedMetresPerSecond;
    [Tooltip("Horizontal crest displacement. Keep this modest to avoid folded wave geometry.")]
    [Range(0f, 1f)] [SerializeField] private float choppiness;

    public Vector2 Direction => direction.sqrMagnitude > 1.0e-6f
        ? direction.normalized
        : Vector2.right;
    public float WavelengthMetres => Mathf.Max(wavelengthMetres, 0.25f);
    public float AmplitudeMetres => Mathf.Max(amplitudeMetres, 0f);
    public float SpeedMetresPerSecond => Mathf.Max(speedMetresPerSecond, 0f);
    public float Choppiness => Mathf.Clamp01(choppiness);

    public OceanWaveComponent(
        Vector2 direction,
        float wavelengthMetres,
        float amplitudeMetres,
        float speedMetresPerSecond,
        float choppiness = 0f)
    {
        this.direction = direction;
        this.wavelengthMetres = wavelengthMetres;
        this.amplitudeMetres = amplitudeMetres;
        this.speedMetresPerSecond = speedMetresPerSecond;
        this.choppiness = choppiness;
    }
}

public readonly struct OceanWaveRuntimeSettings
{
    public readonly bool Enabled;
    public readonly float FineVertexSpacingMetres;
    public readonly float FineRadiusMetres;
    public readonly int RingsPerSpacingLevel;
    public readonly float DisplacementFadeStartMetres;
    public readonly float DisplacementFadeEndMetres;
    public readonly float MaskCoverageMetres;
    public readonly int MaskResolution;
    public readonly float MaskAnchorSnapMetres;
    public readonly float DepthAllowancePower;
    public readonly float DistanceAllowancePower;
    public readonly float NoiseWorldSizeMetres;
    public readonly float DomainWarpMetres;
    public readonly float AmplitudeVariation;
    public readonly Color WhitecapColour;
    public readonly float WhitecapStrength;
    public readonly float WhitecapHeightThreshold;
    public readonly float WhitecapSlopeThreshold;
    public readonly float WhitecapCoverage;
    public readonly float WhitecapNoiseWorldSizeMetres;
    public readonly float WhitecapFineNoiseScale;
    public readonly float WhitecapCounterflowSpeed;
    public readonly bool OnshoreWaveEnabled;
    public readonly float OnshoreWaveWavelengthMetres;
    public readonly float OnshoreWaveAmplitudeMetres;
    public readonly float OnshoreWaveSpeedMetresPerSecond;
    public readonly float OnshoreWaveChoppiness;
    public readonly float OnshoreWaveLeadingEdgeSharpness;
    public readonly float OnshoreWaveSharpeningDistanceMetres;
    public readonly OceanWaveComponent Wave0;
    public readonly OceanWaveComponent Wave1;
    public readonly OceanWaveComponent Wave2;
    public readonly OceanWaveComponent Wave3;

    public float MaximumVerticalDisplacement =>
        (Wave0.AmplitudeMetres
            + Wave1.AmplitudeMetres
            + Wave2.AmplitudeMetres
            + Wave3.AmplitudeMetres
            + (OnshoreWaveEnabled ? OnshoreWaveAmplitudeMetres : 0f))
        * (1f + AmplitudeVariation);

    public float MaximumHorizontalDisplacement =>
        (Wave0.AmplitudeMetres * Wave0.Choppiness
            + Wave1.AmplitudeMetres * Wave1.Choppiness
            + Wave2.AmplitudeMetres * Wave2.Choppiness
            + Wave3.AmplitudeMetres * Wave3.Choppiness
            + (OnshoreWaveEnabled
                ? OnshoreWaveAmplitudeMetres * OnshoreWaveChoppiness
                : 0f))
        * (1f + AmplitudeVariation);

    public OceanWaveRuntimeSettings(
        bool enabled,
        float fineVertexSpacingMetres,
        float fineRadiusMetres,
        int ringsPerSpacingLevel,
        float displacementFadeStartMetres,
        float displacementFadeEndMetres,
        float maskCoverageMetres,
        int maskResolution,
        float maskAnchorSnapMetres,
        float depthAllowancePower,
        float distanceAllowancePower,
        float noiseWorldSizeMetres,
        float domainWarpMetres,
        float amplitudeVariation,
        Color whitecapColour,
        float whitecapStrength,
        float whitecapHeightThreshold,
        float whitecapSlopeThreshold,
        float whitecapCoverage,
        float whitecapNoiseWorldSizeMetres,
        float whitecapFineNoiseScale,
        float whitecapCounterflowSpeed,
        bool onshoreWaveEnabled,
        float onshoreWaveWavelengthMetres,
        float onshoreWaveAmplitudeMetres,
        float onshoreWaveSpeedMetresPerSecond,
        float onshoreWaveChoppiness,
        float onshoreWaveLeadingEdgeSharpness,
        float onshoreWaveSharpeningDistanceMetres,
        OceanWaveComponent wave0,
        OceanWaveComponent wave1,
        OceanWaveComponent wave2,
        OceanWaveComponent wave3)
    {
        Enabled = enabled;
        FineVertexSpacingMetres = Mathf.Clamp(fineVertexSpacingMetres, 0.5f, 16f);
        FineRadiusMetres = Mathf.Max(fineRadiusMetres, FineVertexSpacingMetres * 4f);
        RingsPerSpacingLevel = Mathf.Clamp(ringsPerSpacingLevel, 2, 32);
        DisplacementFadeEndMetres = Mathf.Clamp(
            displacementFadeEndMetres,
            FineVertexSpacingMetres * 2f,
            FineRadiusMetres);
        DisplacementFadeStartMetres = Mathf.Clamp(
            displacementFadeStartMetres,
            0f,
            DisplacementFadeEndMetres - FineVertexSpacingMetres);
        MaskCoverageMetres = Mathf.Max(maskCoverageMetres, DisplacementFadeEndMetres * 2f);
        MaskResolution = Mathf.Clamp(Mathf.ClosestPowerOfTwo(maskResolution), 64, 2048);
        var snapSteps = Mathf.Max(
            1,
            Mathf.RoundToInt(maskAnchorSnapMetres / FineVertexSpacingMetres));
        MaskAnchorSnapMetres = snapSteps * FineVertexSpacingMetres;
        DepthAllowancePower = Mathf.Clamp(depthAllowancePower, 0.25f, 8f);
        DistanceAllowancePower = Mathf.Clamp(distanceAllowancePower, 0.25f, 8f);
        NoiseWorldSizeMetres = Mathf.Clamp(noiseWorldSizeMetres, 256f, 16384f);
        DomainWarpMetres = Mathf.Clamp(domainWarpMetres, 0f, 32f);
        AmplitudeVariation = Mathf.Clamp(amplitudeVariation, 0f, 0.75f);
        WhitecapColour = whitecapColour;
        WhitecapStrength = Mathf.Clamp(whitecapStrength, 0f, 2f);
        WhitecapHeightThreshold = Mathf.Clamp(whitecapHeightThreshold, 0.5f, 0.98f);
        WhitecapSlopeThreshold = Mathf.Clamp01(whitecapSlopeThreshold);
        WhitecapCoverage = Mathf.Clamp01(whitecapCoverage);
        WhitecapNoiseWorldSizeMetres = Mathf.Clamp(
            whitecapNoiseWorldSizeMetres,
            0.5f,
            64f);
        WhitecapFineNoiseScale = Mathf.Clamp(whitecapFineNoiseScale, 0.1f, 1f);
        WhitecapCounterflowSpeed = Mathf.Clamp(
            whitecapCounterflowSpeed,
            0f,
            2f);
        OnshoreWaveEnabled = onshoreWaveEnabled;
        OnshoreWaveWavelengthMetres = Mathf.Clamp(
            onshoreWaveWavelengthMetres,
            1f,
            100f);
        OnshoreWaveAmplitudeMetres = Mathf.Clamp(
            onshoreWaveAmplitudeMetres,
            0f,
            4f);
        OnshoreWaveSpeedMetresPerSecond = Mathf.Clamp(
            onshoreWaveSpeedMetresPerSecond,
            0f,
            20f);
        OnshoreWaveChoppiness = Mathf.Clamp01(onshoreWaveChoppiness);
        OnshoreWaveLeadingEdgeSharpness = Mathf.Clamp01(
            onshoreWaveLeadingEdgeSharpness);
        OnshoreWaveSharpeningDistanceMetres = Mathf.Clamp(
            onshoreWaveSharpeningDistanceMetres,
            0.25f,
            16f);
        Wave0 = wave0;
        Wave1 = wave1;
        Wave2 = wave2;
        Wave3 = wave3;
    }

    public static OceanWaveRuntimeSettings Default => new OceanWaveRuntimeSettings(
        true,
        1f,
        256f,
        8,
        192f,
        256f,
        1024f,
        512,
        16f,
        1.35f,
        1.15f,
        2048f,
        9f,
        0.6f,
        new Color(0.90f, 0.96f, 1f, 1f),
        0.85f,
        0.68f,
        0.12f,
        0.58f,
        7f,
        0.32f,
        0.65f,
        true,
        12f,
        0.16f,
        2.2f,
        0.18f,
        0.95f,
        12f,
        new OceanWaveComponent(new Vector2(1f, 0.18f), 30f, 0.34f, 3.6f),
        new OceanWaveComponent(new Vector2(0.32f, 1f), 15f, 0.18f, 2.8f),
        new OceanWaveComponent(new Vector2(-0.82f, 0.55f), 7.5f, 0.09f, 2.1f),
        new OceanWaveComponent(new Vector2(0.58f, -0.81f), 4f, 0.04f, 1.5f));
}

[CreateAssetMenu(
    fileName = "OceanWaveProfile",
    menuName = "Motu/Ocean Wave Profile")]
public sealed class OceanWaveProfile : ScriptableObject
{
    [Header("Geometry")]
    [Tooltip("Move nearby ocean vertices vertically. Distant waves continue through normal perturbation.")]
    [SerializeField] private bool enableGeometricWaves = true;
    [Tooltip("Vertex spacing nearest the player. Smaller values produce smoother waves but cost substantially more geometry.")]
    [Range(0.5f, 16f)] [SerializeField] private float fineVertexSpacingMetres = 1f;
    [Tooltip("Radius around the player that retains the finest vertex spacing.")]
    [Min(16f)] [SerializeField] private float fineRadiusMetres = 256f;
    [Tooltip("Rows generated before the spacing doubles in the next outer band.")]
    [Range(2, 32)] [SerializeField] private int ringsPerSpacingLevel = 8;
    [Tooltip("Distance at which geometric displacement starts fading into normal-only waves. This must remain inside the fine-mesh radius.")]
    [Min(0f)] [SerializeField] private float displacementFadeStartMetres = 192f;
    [Tooltip("Distance at which geometric displacement reaches zero. It is limited to the fine-mesh radius to prevent undersampled waves from shearing the mesh.")]
    [Min(1f)] [SerializeField] private float displacementFadeEndMetres = 256f;

    [Header("Coastal Attenuation")]
    [Tooltip("World-space area around the player covered by the island depth mask.")]
    [Min(64f)] [SerializeField] private float maskCoverageMetres = 1024f;
    [Tooltip("Resolution of the combined coastal attenuation texture.")]
    [Range(64, 2048)] [SerializeField] private int maskResolution = 512;
    [Tooltip("Distance between mask recenter operations as the player moves.")]
    [Min(0.5f)] [SerializeField] private float maskAnchorSnapMetres = 16f;
    [Tooltip("Curve controlling how quickly waves flatten in shallow water.")]
    [Range(0.25f, 8f)] [SerializeField] private float depthAllowancePower = 1.35f;
    [Tooltip("Curve controlling how quickly waves flatten while approaching the coast.")]
    [Range(0.25f, 8f)] [SerializeField] private float distanceAllowancePower = 1.15f;

    [Header("Wave Shape Noise")]
    [Tooltip("World-space scale of wave grouping. Larger values create broader regions of calm and tall waves.")]
    [Min(256f)] [SerializeField] private float noiseWorldSizeMetres = 2048f;
    [Tooltip("Sideways displacement of the wave field. Increase this to bend otherwise straight crests.")]
    [Range(0f, 32f)] [SerializeField] private float domainWarpMetres = 9f;
    [Tooltip("Coherent variation in individual wave heights. At 0.6, local amplitudes range from roughly 40% to 160% of their base values.")]
    [Range(0f, 0.75f)] [SerializeField] private float amplitudeVariation = 0.6f;

    [Header("Whitecaps")]
    [Tooltip("Colour of broken foam before scene lighting and shadows are applied.")]
    [SerializeField] private Color whitecapColour = new Color(
        0.90f,
        0.96f,
        1f,
        1f);
    [Tooltip("Overall visibility of whitecaps. Set to zero to disable them.")]
    [Range(0f, 2f)] [SerializeField] private float whitecapStrength = 0.85f;
    [Tooltip("Normalized crest height at which foam starts forming. Lower values produce whitecaps on more waves.")]
    [Range(0.5f, 0.98f)] [SerializeField] private float whitecapHeightThreshold = 0.68f;
    [Tooltip("Surface slope needed for full breaking-wave foam. Crest height can still produce a smaller amount on a locally flat peak.")]
    [Range(0f, 1f)] [SerializeField] private float whitecapSlopeThreshold = 0.12f;
    [Tooltip("Fraction of eligible crests retained after coherent breakup noise.")]
    [Range(0f, 1f)] [SerializeField] private float whitecapCoverage = 0.58f;
    [Tooltip("World-space size of coherent gaps and clusters within the whitecaps.")]
    [Range(0.5f, 64f)] [SerializeField] private float whitecapNoiseWorldSizeMetres = 7f;
    [Tooltip("Size of the fine breakup noise relative to the broad whitecap noise. Smaller values create finer fragments.")]
    [Range(0.1f, 1f)] [SerializeField] private float whitecapFineNoiseScale = 0.32f;
    [Tooltip("Speed of the fine breakup layer travelling against the primary swell, relative to that swell's speed.")]
    [Range(0f, 2f)] [SerializeField] private float whitecapCounterflowSpeed = 0.65f;

    [Header("Onshore Wave")]
    [Tooltip("Add a shoreline wave guided by the average of water depth and distance to shore.")]
    [SerializeField] private bool onshoreWaveEnabled = true;
    [Tooltip("Crest spacing of the depth-guided onshore wave.")]
    [Range(1f, 100f)] [SerializeField] private float onshoreWaveWavelengthMetres = 12f;
    [Tooltip("Maximum height contribution inside the averaged depth-and-distance coastal band.")]
    [Range(0f, 4f)] [SerializeField] private float onshoreWaveAmplitudeMetres = 0.16f;
    [Tooltip("Speed at which the depth-guided wave approaches the coast.")]
    [Range(0f, 20f)] [SerializeField] private float onshoreWaveSpeedMetresPerSecond = 2.2f;
    [Tooltip("Crest sharpening and horizontal displacement of the depth-guided wave.")]
    [Range(0f, 1f)] [SerializeField] private float onshoreWaveChoppiness = 0.18f;
    [Tooltip("Compress only the shore-facing rise of each incoming wave as it approaches land. One produces a near-vertical leading face while preserving a rounded rear face.")]
    [Range(0f, 1f)] [SerializeField] private float onshoreWaveLeadingEdgeSharpness = 0.95f;
    [Tooltip("Distance from shore over which leading-edge sharpening grows from zero to its configured maximum.")]
    [Range(0.25f, 16f)] [SerializeField] private float onshoreWaveSharpeningDistanceMetres = 12f;

    [Header("Directional Waves")]
    [Tooltip("Primary broad swell. This should normally have the longest wavelength and largest amplitude.")]
    [SerializeField] private OceanWaveComponent wave0 = new OceanWaveComponent(
        new Vector2(1f, 0.18f), 30f, 0.34f, 3.6f);
    [Tooltip("Secondary swell crossing the primary direction.")]
    [SerializeField] private OceanWaveComponent wave1 = new OceanWaveComponent(
        new Vector2(0.32f, 1f), 15f, 0.18f, 2.8f);
    [Tooltip("Medium surface wave used to break up the two broad swells.")]
    [SerializeField] private OceanWaveComponent wave2 = new OceanWaveComponent(
        new Vector2(-0.82f, 0.55f), 7.5f, 0.09f, 2.1f);
    [Tooltip("Fine surface wave. Keep its amplitude low to avoid a uniformly busy surface.")]
    [SerializeField] private OceanWaveComponent wave3 = new OceanWaveComponent(
        new Vector2(0.58f, -0.81f), 4f, 0.04f, 1.5f);

    public OceanWaveRuntimeSettings ToRuntimeSettings()
    {
        return new OceanWaveRuntimeSettings(
            enableGeometricWaves,
            fineVertexSpacingMetres,
            fineRadiusMetres,
            ringsPerSpacingLevel,
            displacementFadeStartMetres,
            displacementFadeEndMetres,
            maskCoverageMetres,
            maskResolution,
            maskAnchorSnapMetres,
            depthAllowancePower,
            distanceAllowancePower,
            noiseWorldSizeMetres,
            domainWarpMetres,
            amplitudeVariation,
            whitecapColour,
            whitecapStrength,
            whitecapHeightThreshold,
            whitecapSlopeThreshold,
            whitecapCoverage,
            whitecapNoiseWorldSizeMetres,
            whitecapFineNoiseScale,
            whitecapCounterflowSpeed,
            onshoreWaveEnabled,
            onshoreWaveWavelengthMetres,
            onshoreWaveAmplitudeMetres,
            onshoreWaveSpeedMetresPerSecond,
            onshoreWaveChoppiness,
            onshoreWaveLeadingEdgeSharpness,
            onshoreWaveSharpeningDistanceMetres,
            wave0,
            wave1,
            wave2,
            wave3);
    }

    private void OnValidate()
    {
        fineVertexSpacingMetres = Mathf.Clamp(fineVertexSpacingMetres, 0.5f, 16f);
        fineRadiusMetres = Mathf.Max(
            fineRadiusMetres,
            fineVertexSpacingMetres * 4f);
        displacementFadeEndMetres = Mathf.Clamp(
            displacementFadeEndMetres,
            fineVertexSpacingMetres * 2f,
            fineRadiusMetres);
        displacementFadeStartMetres = Mathf.Clamp(
            displacementFadeStartMetres,
            0f,
            displacementFadeEndMetres - fineVertexSpacingMetres);
    }
}
