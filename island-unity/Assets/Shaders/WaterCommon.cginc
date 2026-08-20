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
    float3 viewDirection)
{
    float3 reflectionDirection = reflect(-viewDirection, worldNormal);
    half skyHeight = saturate(reflectionDirection.y);
    fixed3 skyReflection = lerp(
        _ReflectionHorizonColor.rgb,
        _ReflectionColor.rgb,
        skyHeight);
    half fresnel = pow(
        1.0h - saturate(dot(worldNormal, viewDirection)),
        _ReflectionFresnelPower);
    half reflectionWeight = saturate(
        _ReflectionStrength * lerp(0.08h, 1.0h, fresnel));
    fixed3 water = lerp(waterBody, skyReflection, reflectionWeight);
    half sunAlignment = saturate(dot(
        reflectionDirection,
        normalize(_WorldSpaceLightPos0.xyz)));
    half sunGlint = pow(sunAlignment, _SunGlintSharpness)
        * _SunGlintStrength;
    return water + _LightColor0.rgb * sunGlint;
}

#endif
