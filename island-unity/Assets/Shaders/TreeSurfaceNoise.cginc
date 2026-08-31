#ifndef MOTU_TREE_SURFACE_NOISE_INCLUDED
#define MOTU_TREE_SURFACE_NOISE_INCLUDED

sampler3D _CliffNoise3D;
float _TreeNoisePeriod;
half _TreeNoiseDetailScale;
half _TreeNoiseFineScale;
half _TreeNormalStrength;
half _TreeHueVariationDegrees;
float4x4 _IslandWorldToLocal;

#include "CloudCommon.cginc"

struct MotuTreeNoiseSample
{
    half3 broad;
    half3 detail;
    half3 fine;
    half hue;
};

MotuTreeNoiseSample MotuSampleTreeNoise(float3 islandLocalPosition)
{
    MotuTreeNoiseSample sample;
    float3 noisePosition = islandLocalPosition / max(_TreeNoisePeriod, 0.1);
    sample.broad = tex3D(
        _CliffNoise3D,
        noisePosition + float3(0.13, 0.47, 0.71)).rgb * 2.0 - 1.0;
    sample.detail = tex3D(
        _CliffNoise3D,
        noisePosition * max(_TreeNoiseDetailScale, 1.0)
            + float3(0.61, 0.19, 0.83)).rgb * 2.0 - 1.0;
    sample.fine = tex3D(
        _CliffNoise3D,
        noisePosition * max(_TreeNoiseFineScale, 1.0)
            + float3(0.37, 0.89, 0.29)).rgb * 2.0 - 1.0;
    sample.hue = dot(sample.broad, half3(0.46, 0.34, 0.20)) * 0.60
        + dot(sample.detail, half3(0.21, 0.51, 0.28)) * 0.30
        + sample.fine.b * 0.10;
    return sample;
}

half3 MotuPerturbTreeNormal(half3 normal, MotuTreeNoiseSample sample)
{
    half3 perturbation = sample.broad * 0.20
        + sample.detail * 0.45
        + sample.fine * 0.35;
    perturbation -= normal * dot(perturbation, normal);
    return normalize(normal + perturbation * _TreeNormalStrength);
}

half3 MotuRotateTreeHue(half3 color, half hueSignal)
{
    half angle = hueSignal * _TreeHueVariationDegrees * 0.0174532925;
    half sine;
    half cosine;
    sincos(angle, sine, cosine);
    const half3 greyAxis = half3(0.577350269, 0.577350269, 0.577350269);
    return saturate(
        color * cosine
            + cross(greyAxis, color) * sine
            + greyAxis * dot(greyAxis, color) * (1.0 - cosine));
}

half3 MotuShadeFoliage(
    half3 albedo,
    half3 normal,
    float3 worldPosition,
    half3 lightDirection,
    half3 lightColor,
    half attenuation,
    half3 transmissionColor,
    half translucency,
    half ambientFloor)
{
    MotuCloudLighting cloud = MotuCloudSurfaceLighting(worldPosition);
    half facing = dot(normal, lightDirection);
    half wrappedDiffuse = saturate((facing + 0.28h) / 1.28h);
    half backLighting = pow(saturate(-facing), 2.0h) * translucency;
    half3 ambient = max(
        ShadeSH9(half4(normal, 1.0h)) * cloud.ambientTransmittance,
        half3(ambientFloor, ambientFloor, ambientFloor));
    half3 directLight = lightColor * attenuation * cloud.directTransmittance;
    return albedo * (ambient + directLight * wrappedDiffuse)
        + transmissionColor * directLight * backLighting;
}

half MotuTreeCanopyAlpha(
    float3 islandLocalPosition,
    half canopyCoverage,
    half edgeSoftness,
    half materialAlpha)
{
    // Hold the vertical coordinate constant so upper and lower canopy faces
    // cut out together, producing genuine openings through the draped shell.
    float3 columnPosition = float3(
        islandLocalPosition.x,
        0.0,
        islandLocalPosition.z) / max(_TreeNoisePeriod, 0.1);
    half3 broad = tex3D(
        _CliffNoise3D,
        columnPosition + float3(0.29, 0.43, 0.67)).rgb * 2.0 - 1.0;
    half3 detail = tex3D(
        _CliffNoise3D,
        columnPosition * max(_TreeNoiseDetailScale, 1.0)
            + float3(0.73, 0.11, 0.53)).rgb * 2.0 - 1.0;
    half3 fine = tex3D(
        _CliffNoise3D,
        columnPosition * max(_TreeNoiseFineScale, 1.0)
            + float3(0.17, 0.79, 0.31)).rgb * 2.0 - 1.0;
    half holeNoise = saturate(
        0.5
            + (broad.r * 0.35
                + detail.g * 0.45
                + fine.b * 0.20) * 0.5);
    half threshold = 1.0 - saturate(canopyCoverage);
    half softness = max(edgeSoftness, 0.001);
    return smoothstep(
        threshold - softness,
        threshold + softness,
        holeNoise) * saturate(materialAlpha);
}

#endif
