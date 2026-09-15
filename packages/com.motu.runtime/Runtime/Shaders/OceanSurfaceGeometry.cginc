#ifndef MOTU_OCEAN_SURFACE_GEOMETRY_INCLUDED
#define MOTU_OCEAN_SURFACE_GEOMETRY_INCLUDED
#include "OceanWaves.cginc"
#include "OceanDeckWaveClamp.cginc"
#include "OceanShipWaves.cginc"

// Shared by the visible ocean and the underwater interface-depth pass.
float3 MotuOceanSurfacePosition(float3 basePosition, float localRadius, out float4 deckData)
{
    float3 displacement;
    MotuEvaluateOceanWaveDisplacement(basePosition.xz, localRadius, displacement);
    float3 position = basePosition + displacement;
    float clampWeight = MotuDeckWaveClampWeight(position.xz);
    float height = MotuShipWaveHeight(displacement.y, MotuShipWaveField(position.xz),
        clampWeight, MotuOceanClampHeightEnvelope(basePosition.xz));
    position.y = basePosition.y + height;
    deckData = float4(displacement.y, clampWeight, height, abs(height - displacement.y));
    return position;
}
#endif
