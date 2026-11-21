#ifndef MOTU_TERRAIN_ROCK_COMMON_INCLUDED
#define MOTU_TERRAIN_ROCK_COMMON_INCLUDED

// Exposed soil, cliffs and cave walls share broad and fine 3D detail. The
// perturbation follows the surface even where top-down textures would stretch.
half3 MotuProceduralTerrainNormal(half3 normal, float3 fieldPosition, half3 broadNoise,
    float3 detailOffset, half strength)
{
    if (strength <= 0.0h) return normal;
    float3 noisePosition = fieldPosition / max(_CliffNoisePeriod, 1.0);
    half3 detail = tex3D(_CliffNoise3D,
        noisePosition * _CliffNoiseDetailScale + detailOffset).rgb * 2.0h - 1.0h;
    half3 perturbation = broadNoise * 0.45h + detail * 0.55h;
    perturbation -= normal * dot(perturbation, normal);
    return normalize(normal + perturbation * strength);
}

half3 MotuProceduralRockNormal(half3 normal, float3 fieldPosition, half3 broadNoise)
{
    return MotuProceduralTerrainNormal(normal, fieldPosition, broadNoise,
        float3(0.37, 0.61, 0.83), _CliffNormalStrength);
}

half3 MotuProceduralDirtNormal(half3 normal, float3 fieldPosition, half3 broadNoise, half strength)
{
    return MotuProceduralTerrainNormal(normal, fieldPosition, broadNoise,
        float3(0.79, 0.17, 0.53), strength);
}

#endif
