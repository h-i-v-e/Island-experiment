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
float3 MotuOceanHullNormalCorner(float3 side, float sideClamp, float3 end, float endClamp,
    float3 centre, float centreClamp, float seaPlaneHeight, out float clamped)
{
    // Reconstruct the local, unclamped corner from the existing axial samples.
    // Only its hull projection is new: do not evaluate the wave field again.
    float3 position = side + end - centre;
    position.y += sideClamp + endClamp - centreClamp;
    float height = MotuClampToConvexHulls(position, seaPlaneHeight);
    clamped = position.y - height;
    position.y = height;
    return position;
}

float MotuOceanHullNormalContact(float clamped) { return smoothstep(0, .05, clamped); }

// A weighted 3x3 neighbourhood borrows the bend from lowered neighbours,
// including diagonal contacts. Only the shading changes, never the cutout.
// Compute once per vertex and interpolate instead of using flat derivatives.
float4 MotuOceanHullSurfaceNormal(float3 basePosition, float localRadius,
    float3 centreSurface, float centreClamp)
{
    float4 unchanged = float4(0, 1, 0, 0);
    float spacing = max(.75, 1.5 * max(_OceanGeometryLod.y, MotuOceanGeometryFootprint(localRadius)));
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
    float backLeftClamp, backRightClamp, forwardLeftClamp, forwardRightClamp;
    float3 backLeft = MotuOceanHullNormalCorner(left, leftClamp, back, backClamp,
        centreSurface, centreClamp, basePosition.y, backLeftClamp);
    float3 backRight = MotuOceanHullNormalCorner(right, rightClamp, back, backClamp,
        centreSurface, centreClamp, basePosition.y, backRightClamp);
    float3 forwardLeft = MotuOceanHullNormalCorner(left, leftClamp, forward, forwardClamp,
        centreSurface, centreClamp, basePosition.y, forwardLeftClamp);
    float3 forwardRight = MotuOceanHullNormalCorner(right, rightClamp, forward, forwardClamp,
        centreSurface, centreClamp, basePosition.y, forwardRightClamp);
    float3 across = 2 * (right - left) + backRight - backLeft + forwardRight - forwardLeft;
    float3 along = 2 * (forward - back) + forwardLeft - backLeft + forwardRight - backRight;
    float3 normal = cross(along, across);
    float lengthSquared = dot(normal, normal);
    normal = lengthSquared > .000001 ? normal * rsqrt(lengthSquared) : float3(0, 1, 0);
    if (normal.y < 0) normal = -normal;
    // Feather by weighted contact coverage, so one distant/diagonal contact
    // does not abruptly replace the normal of an otherwise untouched wave.
    float coverage = (4 * MotuOceanHullNormalContact(centreClamp)
        + 2 * (MotuOceanHullNormalContact(leftClamp) + MotuOceanHullNormalContact(rightClamp)
            + MotuOceanHullNormalContact(backClamp) + MotuOceanHullNormalContact(forwardClamp))
        + MotuOceanHullNormalContact(backLeftClamp) + MotuOceanHullNormalContact(backRightClamp)
        + MotuOceanHullNormalContact(forwardLeftClamp) + MotuOceanHullNormalContact(forwardRightClamp)) / 16;
    return float4(normal, smoothstep(0, .5, coverage));
}

float3 MotuOceanBlendHullNormal(float3 waveNormal, float4 hullNormal)
{
    // Renormalise after interpolation, and blend continuously into analytic
    // wave lighting without a hard per-fragment clamped/unclamped switch.
    float3 normal = hullNormal.xyz * rsqrt(max(dot(hullNormal.xyz, hullNormal.xyz), .000001));
    return normalize(lerp(waveNormal, normal, saturate(hullNormal.w)));
}
#endif
