#ifndef MOTU_OCEAN_SURFACE_GEOMETRY_INCLUDED
#define MOTU_OCEAN_SURFACE_GEOMETRY_INCLUDED
#include "OceanWaves.cginc"
#include "OceanDeckWaveClamp.cginc"
#include "OceanShipWaves.cginc"
#include "OceanHullVolume.cginc"

// Shared by the visible ocean and the underwater interface-depth pass.
float3 MotuOceanSurfacePosition(float3 basePosition, float localRadius, out float4 deckData, out float hullClamped)
{
    float3 displacement;
    MotuEvaluateOceanWaveDisplacement(basePosition.xz, localRadius, displacement);
    float3 position = basePosition + displacement;
    float clampWeight = MotuDeckWaveClampWeight(position.xz);
    float height = MotuShipWaveHeight(displacement.y, MotuShipWaveField(position.xz),
        clampWeight, MotuOceanClampHeightEnvelope(basePosition.xz));
    position.y = basePosition.y + height;
    deckData = float4(displacement.y, clampWeight, height, abs(height - displacement.y));
    float hullHeight = MotuClampToConvexHulls(position, basePosition.y);
    hullClamped = position.y - hullHeight;
    position.y = hullHeight;
    deckData.z = hullHeight - basePosition.y;
    return position;
}

float3 MotuOceanSurfacePosition(float3 basePosition, float localRadius, out float4 deckData)
{
    float hullClamped;
    return MotuOceanSurfacePosition(basePosition, localRadius, deckData, hullClamped);
}
// Sample the same displaced surface at neighbouring grid positions. These
// normals are computed once per vertex and interpolated across shared edges;
// fragment derivatives would produce a different flat normal per triangle.
float4 MotuOceanHullSurfaceNormal(float3 basePosition, float localRadius,
    float3 centreSurface, float centreClamp)
{
    float4 unchanged = float4(0, 1, 0, 0);
    float spacing = max(.5, max(_OceanGeometryLod.y, MotuOceanGeometryFootprint(localRadius)));
    bool nearHull = false;
    [loop]
    for (int hull = 0; hull < _MotuHullCount; hull++)
    {
        if (all(centreSurface.xz >= _MotuHullMinimum[hull].xz - spacing * 2)
            && all(centreSurface.xz <= _MotuHullMaximum[hull].xz + spacing * 2))
            nearHull = true;
    }
    // Open ocean keeps its analytic normals and pays no extra wave samples.
    [branch]
    if (!nearHull) return unchanged;

    float4 unused;
    float leftClamp, rightClamp, backClamp, forwardClamp;
    float3 left = MotuOceanSurfacePosition(basePosition - float3(spacing, 0, 0), localRadius, unused, leftClamp);
    float3 right = MotuOceanSurfacePosition(basePosition + float3(spacing, 0, 0), localRadius, unused, rightClamp);
    float3 back = MotuOceanSurfacePosition(basePosition - float3(0, 0, spacing), localRadius, unused, backClamp);
    float3 forward = MotuOceanSurfacePosition(basePosition + float3(0, 0, spacing), localRadius, unused, forwardClamp);
    float clampAmount = max(centreClamp, max(max(leftClamp, rightClamp), max(backClamp, forwardClamp)));
    float3 normal = cross(forward - back, right - left);
    float lengthSquared = dot(normal, normal);
    normal = lengthSquared > .000001 ? normal * rsqrt(lengthSquared) : float3(0, 1, 0);
    if (normal.y < 0) normal = -normal;
    return float4(normal, smoothstep(0, .05, clampAmount));
}

float3 MotuOceanBlendHullNormal(float3 waveNormal, float4 hullNormal)
{
    // Renormalise after interpolation, and blend continuously into analytic
    // wave lighting without a hard per-fragment clamped/unclamped switch.
    float3 normal = hullNormal.xyz * rsqrt(max(dot(hullNormal.xyz, hullNormal.xyz), .000001));
    return normalize(lerp(waveNormal, normal, saturate(hullNormal.w)));
}
#endif
