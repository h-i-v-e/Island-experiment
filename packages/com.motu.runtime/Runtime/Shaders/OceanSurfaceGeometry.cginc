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
    float hullHeight = MotuClampToConvexHulls(position);
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
#endif
