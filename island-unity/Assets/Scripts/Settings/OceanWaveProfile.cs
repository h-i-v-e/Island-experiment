using System;
using UnityEngine;

[Serializable]
public struct OceanWaveComponent
{
    [Tooltip("Independent world X/Z wave direction. Global wind direction does not rotate this wave.")]
    [SerializeField] private Vector2 direction;
    [Tooltip("Distance between crests. Larger values make broader ocean swell.")]
    [Min(0.25f)] [SerializeField] private float wavelengthMetres;
    [Tooltip("Maximum vertical contribution at the reference global wind speed of 9 m/s.")]
    [Min(0f)] [SerializeField] private float amplitudeMetres;
    [Tooltip("Travel speed at the reference global wind speed of 9 m/s.")]
    [Min(0f)] [SerializeField] private float speedMetresPerSecond;
    [Tooltip("Horizontal crest displacement. Keep this modest to avoid folded wave geometry.")]
    [Range(0f, 1f)] [SerializeField] private float choppiness;

    public Vector2 Direction
    {
        get => direction.sqrMagnitude > 1.0e-6f ? direction.normalized : Vector2.right;
        set => direction = value;
    }
    public float WavelengthMetres
    {
        get => Mathf.Max(wavelengthMetres, 0.25f);
        set => wavelengthMetres = value;
    }
    public float AmplitudeMetres
    {
        get => Mathf.Max(amplitudeMetres, 0f);
        set => amplitudeMetres = value;
    }
    public float SpeedMetresPerSecond
    {
        get => Mathf.Max(speedMetresPerSecond, 0f);
        set => speedMetresPerSecond = value;
    }
    public float Choppiness
    {
        get => Mathf.Clamp01(choppiness);
        set => choppiness = value;
    }

    internal void ValidateFinite(string name)
    {
        WeatherValueValidation.RequireFinite(direction, name);
        WeatherValueValidation.RequireFinite(wavelengthMetres, name);
        WeatherValueValidation.RequireFinite(amplitudeMetres, name);
        WeatherValueValidation.RequireFinite(speedMetresPerSecond, name);
        WeatherValueValidation.RequireFinite(choppiness, name);
    }

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
    public readonly float OnshoreWaveBreakingStartDepthMetres;
    public readonly float OnshoreWaveBreakingFullDepthMetres;
    public readonly OceanWaveComponent Wave0;
    public readonly OceanWaveComponent Wave1;
    public readonly OceanWaveComponent Wave2;
    public readonly OceanWaveComponent Wave3;

    public float MaximumVerticalDisplacement =>
        (Wave0.AmplitudeMetres * (1f + 0.22f * Wave0.Choppiness)
            + Wave1.AmplitudeMetres * (1f + 0.22f * Wave1.Choppiness)
            + Wave2.AmplitudeMetres * (1f + 0.22f * Wave2.Choppiness)
            + Wave3.AmplitudeMetres * (1f + 0.22f * Wave3.Choppiness)
            + (OnshoreWaveEnabled
                ? OnshoreWaveAmplitudeMetres * (1f + 0.22f * OnshoreWaveChoppiness)
                : 0f))
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
        OceanWaveComponent wave3,
        float onshoreWaveBreakingStartDepthMetres = 5f,
        float onshoreWaveBreakingFullDepthMetres = 3.5f)
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
        OnshoreWaveAmplitudeMetres = Mathf.Max(onshoreWaveAmplitudeMetres, 0f);
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
            128f);
        OnshoreWaveBreakingStartDepthMetres = Mathf.Clamp(onshoreWaveBreakingStartDepthMetres, 0.01f, 5f);
        OnshoreWaveBreakingFullDepthMetres = Mathf.Clamp(onshoreWaveBreakingFullDepthMetres,
            0f, OnshoreWaveBreakingStartDepthMetres - 0.01f);
        Wave0 = wave0;
        Wave1 = wave1;
        Wave2 = wave2;
        Wave3 = wave3;
    }

    public OceanWaveWeatherSettings Weather => new OceanWaveWeatherSettings
    {
        Enabled = Enabled,
        DepthAllowancePower = DepthAllowancePower,
        DistanceAllowancePower = DistanceAllowancePower,
        NoiseWorldSizeMetres = NoiseWorldSizeMetres,
        DomainWarpMetres = DomainWarpMetres,
        AmplitudeVariation = AmplitudeVariation,
        WhitecapColour = WhitecapColour,
        WhitecapStrength = WhitecapStrength,
        WhitecapHeightThreshold = WhitecapHeightThreshold,
        WhitecapSlopeThreshold = WhitecapSlopeThreshold,
        WhitecapCoverage = WhitecapCoverage,
        WhitecapNoiseWorldSizeMetres = WhitecapNoiseWorldSizeMetres,
        WhitecapFineNoiseScale = WhitecapFineNoiseScale,
        WhitecapCounterflowSpeed = WhitecapCounterflowSpeed,
        OnshoreWaveEnabled = OnshoreWaveEnabled,
        OnshoreWaveWavelengthMetres = OnshoreWaveWavelengthMetres,
        OnshoreWaveAmplitudeMetres = OnshoreWaveAmplitudeMetres,
        OnshoreWaveSpeedMetresPerSecond = OnshoreWaveSpeedMetresPerSecond,
        OnshoreWaveChoppiness = OnshoreWaveChoppiness,
        OnshoreWaveLeadingEdgeSharpness = OnshoreWaveLeadingEdgeSharpness,
        OnshoreWaveSharpeningDistanceMetres = OnshoreWaveSharpeningDistanceMetres,
        OnshoreWaveBreakingStartDepthMetres = OnshoreWaveBreakingStartDepthMetres,
        OnshoreWaveBreakingFullDepthMetres = OnshoreWaveBreakingFullDepthMetres,
        Wave0 = Wave0,
        Wave1 = Wave1,
        Wave2 = Wave2,
        Wave3 = Wave3,
    };

    // Weather updates retain the installed mesh, fade distances and mask layout.
    public OceanWaveRuntimeSettings WithWeather(OceanWaveWeatherSettings weather)
    {
        weather.ValidateFinite();
        var updated = new OceanWaveRuntimeSettings(
            weather.Enabled,
            FineVertexSpacingMetres,
            FineRadiusMetres,
            RingsPerSpacingLevel,
            DisplacementFadeStartMetres,
            DisplacementFadeEndMetres,
            MaskCoverageMetres,
            MaskResolution,
            MaskAnchorSnapMetres,
            weather.DepthAllowancePower,
            weather.DistanceAllowancePower,
            weather.NoiseWorldSizeMetres,
            weather.DomainWarpMetres,
            weather.AmplitudeVariation,
            weather.WhitecapColour,
            weather.WhitecapStrength,
            weather.WhitecapHeightThreshold,
            weather.WhitecapSlopeThreshold,
            weather.WhitecapCoverage,
            weather.WhitecapNoiseWorldSizeMetres,
            weather.WhitecapFineNoiseScale,
            weather.WhitecapCounterflowSpeed,
            weather.OnshoreWaveEnabled,
            weather.OnshoreWaveWavelengthMetres,
            weather.OnshoreWaveAmplitudeMetres,
            weather.OnshoreWaveSpeedMetresPerSecond,
            weather.OnshoreWaveChoppiness,
            weather.OnshoreWaveLeadingEdgeSharpness,
            weather.OnshoreWaveSharpeningDistanceMetres,
            weather.Wave0,
            weather.Wave1,
            weather.Wave2,
            weather.Wave3,
            weather.OnshoreWaveBreakingStartDepthMetres,
            weather.OnshoreWaveBreakingFullDepthMetres);
        // Include the maximum weather scale (2.5) and bounds size conversion.
        // Finite inputs can still overflow when their displacements are added.
        WeatherValueValidation.RequireFinite(
            updated.MaximumVerticalDisplacement * 5f + 2f, nameof(weather));
        WeatherValueValidation.RequireFinite(
            updated.MaximumHorizontalDisplacement * 5f + 2f, nameof(weather));
        return updated;
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
        96f,
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
    [Tooltip("Noise texture repeat for the longest wave. Shorter waves sample the same noise at proportionally smaller scales. Larger values create broader regions of calm and tall waves.")]
    [Min(256f)] [SerializeField] private float noiseWorldSizeMetres = 2048f;
    [Tooltip("Sideways displacement of the wave field. Increase this to bend otherwise straight crests.")]
    [Range(0f, 32f)] [SerializeField] private float domainWarpMetres = 9f;
    [Tooltip("Height variation sampled separately at each wave's scale. At 0.6, local amplitudes range from roughly 40% to 160% of their base values. Zero disables height variation.")]
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
    [Tooltip("Add an incoming wave guided by distance to shore across the 128-metre coastal band.")]
    [SerializeField] private bool onshoreWaveEnabled = true;
    [Tooltip("Crest spacing of the onshore wave.")]
    [Range(1f, 100f)] [SerializeField] private float onshoreWaveWavelengthMetres = 12f;
    [Tooltip("Maximum height contribution inside the coastal band.")]
    [Min(0f)] [SerializeField] private float onshoreWaveAmplitudeMetres = 0.16f;
    [Tooltip("Speed at which the onshore wave approaches the coast.")]
    [Range(0f, 20f)] [SerializeField] private float onshoreWaveSpeedMetresPerSecond = 2.2f;
    [Tooltip("Crest sharpening and horizontal displacement of the onshore wave.")]
    [Range(0f, 1f)] [SerializeField] private float onshoreWaveChoppiness = 0.18f;
    [Tooltip("Compress the shore-facing rise over the configured breaking-depth range. One produces a near-vertical leading face while preserving a rounded rear face.")]
    [Range(0f, 1f)] [SerializeField] private float onshoreWaveLeadingEdgeSharpness = 0.95f;
    [Tooltip("Maximum distance from shore where depth-driven breaking can form, with a fade over the outer 20 percent. Actual depth controls sharpening and foam inside this band.")]
    [Range(0.25f, 128f)] [SerializeField] private float onshoreWaveSharpeningDistanceMetres = 96f;

    [Tooltip("Water depth where the face starts sharpening and making breaker foam. The depth map covers up to 5 metres.")]
    [Range(0.01f, 5f)] [SerializeField] private float onshoreWaveBreakingStartDepthMetres = 5f;
    [Tooltip("Water depth where sharpening and breaker foam reach full strength. Raise this to make waves break while they are taller. Must be shallower than the start depth.")]
    [Range(0f, 5f)] [SerializeField] private float onshoreWaveBreakingFullDepthMetres = 3.5f;

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
            wave3,
            onshoreWaveBreakingStartDepthMetres,
            onshoreWaveBreakingFullDepthMetres);
    }

    private void OnValidate()
    {
        onshoreWaveBreakingStartDepthMetres = Mathf.Clamp(onshoreWaveBreakingStartDepthMetres, 0.01f, 5f);
        onshoreWaveBreakingFullDepthMetres = Mathf.Clamp(onshoreWaveBreakingFullDepthMetres,
            0f, onshoreWaveBreakingStartDepthMetres - 0.01f);
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
