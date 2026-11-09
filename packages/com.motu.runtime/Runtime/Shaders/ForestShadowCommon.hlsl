#ifndef MOTU_FOREST_SHADOW_INCLUDED
#define MOTU_FOREST_SHADOW_INCLUDED

sampler2D _MotuForestShadowMask;
float4 _MotuForestShadowUV;
half _MotuForestShadowStrength;

half MotuForestBakedShadow(float3 islandPosition)
{
    float2 uv = islandPosition.xz * _MotuForestShadowUV.xy + _MotuForestShadowUV.zw;
    half inside = all(uv >= 0) && all(uv <= 1);
    return 1.0h - tex2D(_MotuForestShadowMask, uv).r * inside * _MotuForestShadowStrength;
}

half MotuForestShadowFade(float3 positionWS)
{
#if defined(MAIN_LIGHT_CALCULATE_SHADOWS)
    half fade = GetMainLightShadowFade(positionWS);
    #if defined(_MAIN_LIGHT_SHADOWS_CASCADE)
        // A cascade can end before the spherical distance fade, especially at
        // grazing view angles. Do not leave a bright ring outside the last one.
        fade = max(fade, step(4.0h, ComputeCascadeIndex(positionWS)));
    #endif
    return fade;
#else
    return 1.0h;
#endif
}

half MotuForestGroundShadow(float3 islandPosition, float3 positionWS, half realtimeShadow)
{
    half coverage = 1.0h - MotuForestBakedShadow(islandPosition);
    // URP already fades its shadow towards white. Replace that white endpoint
    // with our bake: lerp(rawRealtime, baked, fade), without fading it twice.
    return saturate(realtimeShadow - coverage * MotuForestShadowFade(positionWS));
}

#endif
