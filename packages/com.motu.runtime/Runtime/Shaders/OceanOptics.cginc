#ifndef MOTU_OCEAN_OPTICS_INCLUDED
#define MOTU_OCEAN_OPTICS_INCLUDED

#include "WaterOptics.cginc"

float _CoastalOpacity;

float MotuOceanCoastalTint(float2 worldXZ)
{
    // The composed mask carries depth / 10 m in B and shore distance / 128 m
    // in G. Tint now follows the displaced ocean rather than a flat overlay.
    float4 coastal = MotuOceanCoastalData(worldXZ);
    float shallow = 1 - saturate(coastal.b);
    float shoreWeight = lerp(1, .35, saturate(coastal.g * (128.0 / 16.0)));
    float2 uv = (worldXZ - _WaveAttenuationWorldRect.xy) * _WaveAttenuationWorldRect.zw;
    float2 edgeMetres = min(uv, 1 - uv) / max(_WaveAttenuationWorldRect.zw, .000001);
    float edgeFade = smoothstep(0, 24, min(edgeMetres.x, edgeMetres.y));
    return shallow * shoreWeight * edgeFade * saturate(_CoastalOpacity);
}

float _RippleStrength;
float _RippleWorldSize;
float _RippleSpeed;
float _SurfaceDistortionStrength;
float _WindRoughness;
sampler2D _OceanFoamHistory;
float4 _OceanFoamHistoryRect;
float _PersistentFoamStrength;

float MotuOceanSeabedVisibility(float sceneWorldY, float seaLevel)
{
    // Finish before the native terrain cutoff at 10 m, leaving a small margin
    // for rasterization/refraction at the mesh edge. Use mean sea level so
    // crests, troughs, and oblique views do not move the transition.
    return 1 - smoothstep(6, 9.5, max(seaLevel - sceneWorldY, 0));
}

float MotuOceanHistoryFoam(float2 worldXZ)
{
    float2 uv = (worldXZ - _OceanFoamHistoryRect.xy) * _OceanFoamHistoryRect.zw;
    // The history is a square following the viewer. Hide its boundary over
    // the outer half of an inscribed circle (64-128 m for the current map),
    // so whitecaps and old wakes dissolve before reaching any straight edge.
    float radius = length((uv - 0.5) * 2.0);
    return tex2D(_OceanFoamHistory, saturate(uv)).r
        * (1.0 - smoothstep(0.5, 1.0, radius)) * max(_PersistentFoamStrength, 0);
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

float2 MotuOceanSurfaceDistortion(float3 worldPosition)
{
    // Use accumulated wind travel so direction/speed changes do not restart
    // the pattern. Retain the old .20/.16 m/s drift rates at the 9 m/s reference wind.
    float2 position = MotuCloudWorldToLocal(worldPosition).xz;
    float2 firstUV = (position - _MotuWindOffset.xy * (0.20 / 9.0)) / 8.0;
    float2 secondUV = ((position - _MotuWindOffset.xy * (0.16 / 9.0)) / 8.0).yx;
    float2 noise = float2(
        tex2D(_NoiseTex, firstUV).r,
        tex2D(_NoiseTex, secondUV).g);
    return (noise - 0.5) * saturate(_SurfaceDistortionStrength);
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
        // Keep the pattern in fixed world axes. Rotating its coordinates with
        // each wind shift sweeps it around the origin, faster farther from it.
        float2 position = worldPosition.xz;
        float2 travel = _MotuWindOffset.xy * max(_RippleSpeed, 0);
        float span = wavelength * 64;
        float2 uv = (position - travel * .12) / float2(span, span * 1.3);
        float2 coarse = MotuOceanRippleSlope(uv);
        // A weaker oblique band travels faster and slightly across the wind.
        // The two patterns continually change their combined shape instead of
        // translating as a single stamped texture. Use integrated wind travel
        // so weather speed changes do not restart the animation.
        const float c = .9063078, s = .4226183;
        float2 secondPosition = position - travel * .20 - float2(-travel.y, travel.x) * .035;
        float2 fineUv = float2(c * secondPosition.x + s * secondPosition.y,
            -s * secondPosition.x + c * secondPosition.y) / (span * .62) + float2(.371, .619);
        float2 fine = MotuOceanRippleSlope(fineUv);
        fine = float2(c * fine.x - s * fine.y, s * fine.x + c * fine.y);
        float2 slope = (coarse + fine * .25) * strength * resolved * 1.5;
        normal = normalize(baseNormal + float3(-slope.x, 0, -slope.y));
    }
    // Filter unresolved detail into the highlight lobe rather than shimmer.
    float variance = max(dot(ddx(normal), ddx(normal)), dot(ddy(normal), ddy(normal)));
    roughness = clamp(sqrt(pow(max(_SurfaceRoughness, .04) + windResponse * _WindRoughness, 2)
        + strength * strength * (1-resolved * resolved) * .5 + min(variance, .2)), .04, .8);
    return normal;
}

#endif
