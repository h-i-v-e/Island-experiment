#ifndef MOTU_TREE_WIND_COMMON_INCLUDED
#define MOTU_TREE_WIND_COMMON_INCLUDED

sampler2D _GrassPatchNoise;
float4 _GrassWindDirection;
float _GrassWindStrength;
float _GrassWindSpeed;
float _GrassWindWorldSize;
float _WorldSize;
float _TreeWindStrengthMultiplier;
float _TreeWindBasePinHeight;
float _TreeWindFullBendHeight;

#include "GrassWindCommon.cginc"

float MotuHasTreeRoot(float4 treeData)
{
    return 1.0 - step(0.01, abs(treeData.w - 0.5));
}

float3 MotuDecodeTreeRoot(float4 treeData)
{
    return float3(
        (treeData.x - 0.5) * _WorldSize,
        treeData.z * _WorldSize,
        (treeData.y - 0.5) * _WorldSize);
}

float MotuTreeWindBendWeight(float heightAboveGround)
{
    float bendWeight = smoothstep(
        max(_TreeWindBasePinHeight, 0.0),
        max(_TreeWindFullBendHeight, _TreeWindBasePinHeight + 0.01),
        max(heightAboveGround, 0.0));
    return bendWeight * bendWeight;
}

float3 MotuTreeWindOffsetAtHeight(
    float3 worldPosition,
    float3 islandLocalPosition,
    float4 treeData,
    float heightAboveGround)
{
    float hasTreeRoot = MotuHasTreeRoot(treeData);
    float3 treeRoot = MotuDecodeTreeRoot(treeData) * hasTreeRoot;

    // Recover a stable world-space root so every vertex belonging to a tree or
    // its nearest canopy region samples exactly the same moving field.
    float3 rootWorldPosition = worldPosition - mul(
        (float3x3)unity_ObjectToWorld,
        islandLocalPosition - treeRoot);
    float3 wind = MotuGrassWindSample(rootWorldPosition.xz);
    return float3(wind.x, 0.0, wind.z)
        * (_GrassWindStrength
            * max(_TreeWindStrengthMultiplier, 0.0)
            * wind.y
            * MotuTreeWindBendWeight(heightAboveGround));
}

float3 MotuTreeWindOffset(
    float3 worldPosition,
    float3 islandLocalPosition,
    float4 treeData)
{
    float hasTreeRoot = MotuHasTreeRoot(treeData);
    float3 treeRoot = MotuDecodeTreeRoot(treeData) * hasTreeRoot;
    return MotuTreeWindOffsetAtHeight(
        worldPosition,
        islandLocalPosition,
        treeData,
        islandLocalPosition.y - treeRoot.y);
}

half3 MotuTreeWindNormal(half3 worldNormal, float3 windOffset)
{
    return normalize(worldNormal - windOffset * 0.18);
}

#endif
