#ifndef MOTU_OCEAN_OPTICS_INCLUDED
#define MOTU_OCEAN_OPTICS_INCLUDED

#include "WaterOptics.cginc"

float _RippleStrength;
float _RippleWorldSize;
float _RippleSpeed;
float _WindRoughness;
sampler2D _OceanFoamHistory;
float4 _OceanFoamHistoryRect;
float _PersistentFoamStrength;

float MotuOceanHistoryFoam(float2 worldXZ)
{
    float2 uv = (worldXZ - _OceanFoamHistoryRect.xy) * _OceanFoamHistoryRect.zw;
    float edge = min(min(uv.x, uv.y), min(1-uv.x, 1-uv.y));
    return tex2D(_OceanFoamHistory, saturate(uv)).r
        * smoothstep(0, .04, edge) * max(_PersistentFoamStrength, 0);
}

float2 MotuOceanRippleSlope(float2 uv)
{
    // The weather texture has only four texels per noise cell. Start one mip
    // down to remove its tight cell corners before taking the height gradient.
    float2 dx = ddx(uv) * 256, dy = ddy(uv) * 256;
    float lod = max(1, .5 * log2(max(max(dot(dx, dx), dot(dy, dy)), 1)));
    const float stepUV = 2.0 / 256.0;
    return float2(
        tex2Dlod(_NoiseTex, float4(uv + float2(stepUV, 0), 0, lod)).r - tex2Dlod(_NoiseTex, float4(uv - float2(stepUV, 0), 0, lod)).r,
        tex2Dlod(_NoiseTex, float4(uv + float2(0, stepUV), 0, lod)).r - tex2Dlod(_NoiseTex, float4(uv - float2(0, stepUV), 0, lod)).r);
}

float3 MotuOceanRippleNormal(float3 worldPosition, float3 baseNormal, out float roughness)
{
    float windResponse = saturate(max(_MotuWeatherWind.z, 0) / 12);
    float strength = max(_RippleStrength, 0) * windResponse;
    float wavelength = max(_RippleWorldSize, .1);
    float footprint = max(length(ddx(worldPosition.xz)), length(ddy(worldPosition.xz)));
    float resolved = 1 - smoothstep(wavelength * .3, wavelength * 2, footprint);
    float3 normal = baseNormal;
    [branch]
    if (strength * resolved > .0001)
    {
        float2 wind = MotuWindDirection();
        float2 crossWind = float2(-wind.y, wind.x);
        float2 position = float2(dot(worldPosition.xz, wind), dot(worldPosition.xz, crossWind));
        float2 travel = float2(dot(_MotuWindOffset.xy, wind), dot(_MotuWindOffset.xy, crossWind))
            * max(_RippleSpeed, 0);
        float span = wavelength * 64;
        float2 uv = (position - travel * .12) / float2(span, span * 1.3);
        float2 coarse = MotuOceanRippleSlope(uv);
        // A weaker oblique band travels faster and slightly across the wind.
        // The two patterns continually change their combined shape instead of
        // translating as a single stamped texture. Use integrated wind travel
        // so weather speed changes do not restart the animation.
        const float c = .9063078, s = .4226183;
        float2 secondPosition = position - travel * .20 - float2(0, travel.x * .035);
        float2 fineUv = float2(c * secondPosition.x + s * secondPosition.y,
            -s * secondPosition.x + c * secondPosition.y) / (span * .62) + float2(.371, .619);
        float2 fine = MotuOceanRippleSlope(fineUv);
        fine = float2(c * fine.x - s * fine.y, s * fine.x + c * fine.y);
        float2 slope = (coarse + fine * .25) * strength * resolved * 1.5;
        float2 worldSlope = wind * slope.x + crossWind * slope.y;
        normal = normalize(baseNormal + float3(-worldSlope.x, 0, -worldSlope.y));
    }
    // Filter unresolved detail into the highlight lobe rather than shimmer.
    float variance = max(dot(ddx(normal), ddx(normal)), dot(ddy(normal), ddy(normal)));
    roughness = clamp(sqrt(pow(max(_SurfaceRoughness, .04) + windResponse * _WindRoughness, 2)
        + strength * strength * (1-resolved * resolved) * .5 + min(variance, .2)), .04, .8);
    return normal;
}

#endif
