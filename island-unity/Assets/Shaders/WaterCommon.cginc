#ifndef MOTU_WATER_COMMON_INCLUDED
#define MOTU_WATER_COMMON_INCLUDED

#include "UnityCG.cginc"
#include "Lighting.cginc"

fixed4 _Color;
half _ShallowOpacity;
float _OpacityDepth;
fixed4 _ReflectionColor;
fixed4 _ReflectionHorizonColor;
half _ReflectionStrength;
half _ReflectionFresnelPower;
half _SunGlintStrength;
half _SunGlintSharpness;
half _PlanarReflectionWeight;
half _PlanarReflectionDistortion;
sampler2D _PlanarReflectionTexture;
float4x4 _PlanarReflectionMatrix;
half _PlanarReflectionAvailable;
float4x4 _IslandWorldToLocal;
UNITY_DECLARE_DEPTH_TEXTURE(_CameraDepthTexture);

float3 MotuFacingWaterNormal(float3 worldNormal, float3 viewDirection)
{
    float3 normal = normalize(worldNormal);
    return normal * (dot(normal, viewDirection) >= 0.0 ? 1.0 : -1.0);
}

float MotuWaterDepth(float4 screenPosition, float surfaceEyeDepth)
{
    float sceneDepth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE_PROJ(
        _CameraDepthTexture,
        UNITY_PROJ_COORD(screenPosition)));
    return max(sceneDepth - surfaceEyeDepth, 0.0);
}

half MotuWaterOpacity(float waterDepth, float opacityDepth)
{
    half depthOpacity = saturate(waterDepth / max(opacityDepth, 0.001));
    return lerp(_ShallowOpacity, _Color.a, depthOpacity);
}

fixed3 MotuShadeWater(
    fixed3 waterBody,
    float3 worldNormal,
    float3 viewDirection,
    float3 worldPosition,
    half2 ripple,
    half planarWeight)
{
    float3 reflectionDirection = reflect(-viewDirection, worldNormal);
    half skyHeight = saturate(reflectionDirection.y);
    fixed3 skyReflection = lerp(
        _ReflectionHorizonColor.rgb,
        _ReflectionColor.rgb,
        skyHeight);
    float4 reflectionPosition = mul(
        _PlanarReflectionMatrix,
        float4(worldPosition, 1.0));
    float inverseW = rcp(max(reflectionPosition.w, 0.0001));
    float2 reflectionUv = reflectionPosition.xy * inverseW
        + ripple * _PlanarReflectionDistortion;
    half reflectionInBounds = step(0.0001, reflectionPosition.w)
        * step(0.0, reflectionUv.x)
        * step(reflectionUv.x, 1.0)
        * step(0.0, reflectionUv.y)
        * step(reflectionUv.y, 1.0);
    fixed3 planarReflection = tex2D(
        _PlanarReflectionTexture,
        saturate(reflectionUv)).rgb;
    half planarBlend = saturate(
        _PlanarReflectionAvailable
        * _PlanarReflectionWeight
        * planarWeight
        * reflectionInBounds);
    fixed3 reflectedScene = lerp(
        skyReflection,
        planarReflection,
        planarBlend);
    half fresnel = pow(
        1.0h - saturate(dot(worldNormal, viewDirection)),
        _ReflectionFresnelPower);
    half reflectionWeight = saturate(
        _ReflectionStrength * lerp(0.08h, 1.0h, fresnel));
    fixed3 water = lerp(waterBody, reflectedScene, reflectionWeight);
    half sunAlignment = saturate(dot(
        reflectionDirection,
        normalize(_WorldSpaceLightPos0.xyz)));
    half sunGlint = pow(sunAlignment, _SunGlintSharpness)
        * _SunGlintStrength;
    return water + _LightColor0.rgb * sunGlint;
}

#endif
