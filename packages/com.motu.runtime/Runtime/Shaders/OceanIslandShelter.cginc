#ifndef MOTU_OCEAN_ISLAND_SHELTER_INCLUDED
#define MOTU_OCEAN_ISLAND_SHELTER_INCLUDED

// Shelter stays in world space as the ocean grid recentres. Zero is open sea;
// one is a protected anchorage. The composer turns this field with the wind
// and blends it over time instead of scrolling the shadow off its island.
sampler2D _OceanShelterTex;
float4 _OceanShelterTex_TexelSize;
float4 _OceanShelterWorldRect;
// Retained wave fraction, reserved, enabled.
float4 _OceanShelterParameters;

float MotuOceanIslandShelter(float2 worldPosition)
{
    if (_OceanShelterParameters.z <= 0.0) return 0.0;
    float2 uv = (worldPosition - _OceanShelterWorldRect.xy)
        * _OceanShelterWorldRect.zw;
    float inside = step(0.0, uv.x) * step(0.0, uv.y)
        * step(uv.x, 1.0) * step(uv.y, 1.0);
    // Fade the outer two texels so leaving a finite player-centred map cannot
    // introduce a height discontinuity in the water or its buoyancy probes.
    float2 border = min(uv, 1.0 - uv)
        / max(abs(_OceanShelterTex_TexelSize.xy) * 2.0, 0.00001);
    float edge = smoothstep(0.0, 1.0, min(border.x, border.y));
    return saturate(tex2Dlod(_OceanShelterTex,
        float4(saturate(uv), 0, 0)).r) * inside * edge
        * saturate(_OceanShelterParameters.z);
}

float MotuOceanShelterWaveScale(float2 worldPosition)
{
    return lerp(1.0, saturate(_OceanShelterParameters.x), MotuOceanIslandShelter(worldPosition));
}

float2 MotuOceanShelterScaleGradient(float2 worldPosition)
{
    float2 texelMetres = max(abs(_OceanShelterTex_TexelSize.xy)
        / max(abs(_OceanShelterWorldRect.zw), 0.000001), 0.5);
    return float2(
        MotuOceanShelterWaveScale(worldPosition + float2(texelMetres.x, 0))
            - MotuOceanShelterWaveScale(worldPosition - float2(texelMetres.x, 0)),
        MotuOceanShelterWaveScale(worldPosition + float2(0, texelMetres.y))
            - MotuOceanShelterWaveScale(worldPosition - float2(0, texelMetres.y)))
        / (2.0 * texelMetres);
}

float MotuOceanShelterDetailScale(float2 worldPosition)
{
    return MotuOceanShelterWaveScale(worldPosition);
}

#endif
