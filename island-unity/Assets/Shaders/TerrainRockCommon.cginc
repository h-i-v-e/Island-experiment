#ifndef MOTU_TERRAIN_ROCK_COMMON_INCLUDED
#define MOTU_TERRAIN_ROCK_COMMON_INCLUDED

// The exposed cliff and cave walls share the same island-local stone detail.
// Callers supply the island's noise sampler, period, detail scale and strength.
half3 MotuProceduralRockNormal(half3 normal, float3 localPosition, half3 broadNoise)
{
    if (_CliffNormalStrength <= 0.0h) return normal;
    float3 noisePosition = localPosition / max(_CliffNoisePeriod, 1.0);
    half3 detail = tex3D(_CliffNoise3D,
        noisePosition * _CliffNoiseDetailScale + float3(0.37, 0.61, 0.83)).rgb * 2.0h - 1.0h;
    half3 perturbation = broadNoise * 0.45h + detail * 0.55h;
    perturbation -= normal * dot(perturbation, normal);
    return normalize(normal + perturbation * _CliffNormalStrength);
}

#endif
