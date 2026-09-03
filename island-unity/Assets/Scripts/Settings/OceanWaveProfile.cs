using System;
using UnityEngine;

[Serializable]
public struct OceanWaveComponent
{
    [SerializeField] private Vector2 direction;
    [Min(0.25f)] [SerializeField] private float wavelengthMetres;
    [Min(0f)] [SerializeField] private float amplitudeMetres;
    [Min(0f)] [SerializeField] private float speedMetresPerSecond;
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
    public readonly OceanWaveComponent Wave0;
    public readonly OceanWaveComponent Wave1;
    public readonly OceanWaveComponent Wave2;
    public readonly OceanWaveComponent Wave3;

    public float MaximumVerticalDisplacement =>
        Wave0.AmplitudeMetres
        + Wave1.AmplitudeMetres
        + Wave2.AmplitudeMetres
        + Wave3.AmplitudeMetres;

    public float MaximumHorizontalDisplacement =>
        Wave0.AmplitudeMetres * Wave0.Choppiness
        + Wave1.AmplitudeMetres * Wave1.Choppiness
        + Wave2.AmplitudeMetres * Wave2.Choppiness
        + Wave3.AmplitudeMetres * Wave3.Choppiness;

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
        OceanWaveComponent wave0,
        OceanWaveComponent wave1,
        OceanWaveComponent wave2,
        OceanWaveComponent wave3)
    {
        Enabled = enabled;
        FineVertexSpacingMetres = Mathf.Clamp(fineVertexSpacingMetres, 0.5f, 16f);
        FineRadiusMetres = Mathf.Max(fineRadiusMetres, FineVertexSpacingMetres * 4f);
        RingsPerSpacingLevel = Mathf.Clamp(ringsPerSpacingLevel, 2, 32);
        DisplacementFadeStartMetres = Mathf.Max(displacementFadeStartMetres, FineRadiusMetres);
        DisplacementFadeEndMetres = Mathf.Max(
            displacementFadeEndMetres,
            DisplacementFadeStartMetres + FineVertexSpacingMetres);
        MaskCoverageMetres = Mathf.Max(maskCoverageMetres, DisplacementFadeEndMetres * 2f);
        MaskResolution = Mathf.Clamp(Mathf.ClosestPowerOfTwo(maskResolution), 64, 2048);
        var snapSteps = Mathf.Max(
            1,
            Mathf.RoundToInt(maskAnchorSnapMetres / FineVertexSpacingMetres));
        MaskAnchorSnapMetres = snapSteps * FineVertexSpacingMetres;
        DepthAllowancePower = Mathf.Clamp(depthAllowancePower, 0.25f, 8f);
        DistanceAllowancePower = Mathf.Clamp(distanceAllowancePower, 0.25f, 8f);
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
        320f,
        480f,
        1024f,
        512,
        16f,
        1.35f,
        1.15f,
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
    [SerializeField] private bool enableGeometricWaves = true;
    [Range(0.5f, 16f)] [SerializeField] private float fineVertexSpacingMetres = 1f;
    [Min(16f)] [SerializeField] private float fineRadiusMetres = 256f;
    [Range(2, 32)] [SerializeField] private int ringsPerSpacingLevel = 8;
    [Min(16f)] [SerializeField] private float displacementFadeStartMetres = 320f;
    [Min(17f)] [SerializeField] private float displacementFadeEndMetres = 480f;

    [Header("Coastal Attenuation")]
    [Min(64f)] [SerializeField] private float maskCoverageMetres = 1024f;
    [Range(64, 2048)] [SerializeField] private int maskResolution = 512;
    [Min(0.5f)] [SerializeField] private float maskAnchorSnapMetres = 16f;
    [Range(0.25f, 8f)] [SerializeField] private float depthAllowancePower = 1.35f;
    [Range(0.25f, 8f)] [SerializeField] private float distanceAllowancePower = 1.15f;

    [Header("Directional Waves")]
    [SerializeField] private OceanWaveComponent wave0 = new OceanWaveComponent(
        new Vector2(1f, 0.18f), 30f, 0.34f, 3.6f);
    [SerializeField] private OceanWaveComponent wave1 = new OceanWaveComponent(
        new Vector2(0.32f, 1f), 15f, 0.18f, 2.8f);
    [SerializeField] private OceanWaveComponent wave2 = new OceanWaveComponent(
        new Vector2(-0.82f, 0.55f), 7.5f, 0.09f, 2.1f);
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
            wave0,
            wave1,
            wave2,
            wave3);
    }
}
