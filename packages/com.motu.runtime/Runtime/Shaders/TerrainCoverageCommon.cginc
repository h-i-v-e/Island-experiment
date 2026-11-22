#ifndef MOTU_TERRAIN_COVERAGE_COMMON_INCLUDED
#define MOTU_TERRAIN_COVERAGE_COMMON_INCLUDED

#include "WheatGroundCommon.hlsl"

#define MOTU_LAYER_DIRT 0
#define MOTU_LAYER_FOREST_FLOOR 1
#define MOTU_LAYER_ROCK 2
#define MOTU_LAYER_RIVER_BED 3
#define MOTU_LAYER_BEACH 4
#define MOTU_LAYER_STONES 5
#define MOTU_LAYER_ROAD 6

sampler3D _CliffNoise3D;
sampler2D _GrassPatchNoise;

float _CliffNoisePeriod;
half _RockPatchNoiseDetailScale;
half _RockBoundaryNoiseStrength;
half _RiverEdgeNoiseStrength;
half _RiverEdgeBlendWidth;
half _ForestFloorEdgeNoiseStrength;
half _ForestFloorEdgeBlendWidth;
half _StonesEdgeNoiseStrength;
half _StonesEdgeBlendWidth;
half _BeachEdgeNoiseStrength;
half _BeachEdgeBlendWidth;
float _SandPatchNoiseWorldSize;
float _GrassPatchNoiseWorldSize;
float _SnowLine;
float _SnowEdgeNoiseMetres;
float _SnowMacroNoiseMetres;
float4 _TerrainLayerWorldSizesA;
float4 _TerrainLayerWorldSizesB;
float4 _MotuTerrainFieldOffset;
float4 _MotuTerrainSeam;
half4 _TerrainHeightInfluencesA;
half4 _TerrainHeightInfluencesB;
half _TerrainHeightBlendDepth;

// Coastal sections use a common logical metre field. Geometry, map UVs and
// sea-level height stay island-local, including after a floating-origin move.
float3 MotuTerrainFieldPosition(float3 localPosition)
{
    return localPosition + float3(_MotuTerrainFieldOffset.x, 0, _MotuTerrainFieldOffset.z);
}

// Independently baked normal/AO maps cannot see the neighboring section.
// At a join use the shared boundary normal and neutral AO, then recover the
// baked detail smoothly within this section's collar. X=0 disables the fade.
half MotuTerrainBakedMapWeight(float3 localPosition)
{
    if (_MotuTerrainSeam.x <= 0) return 1;
    float edgeDistance = max(_MotuTerrainSeam.x - abs(localPosition.x), 0);
    return smoothstep(0, max(_MotuTerrainSeam.y, 0.001), edgeDistance);
}

struct MotuTerrainNoise
{
    half3 broad;
    half3 macro;
    half3 patch;
    half3 detail;
};

struct MotuTerrainCoverage
{
    MotuTerrainNoise noise;
    half dirt;
    half rock;
    half river;
    half beach;
    half forestFloor;
    half stones;
    half road;
    half gravel;
    half grass;
    half snow;
    half topProjection;
    half coastalNoise;
};

struct MotuBaseWeights
{
    half4 a;
    half2 b;
};

half MotuAntialiasedMask(half signedDistance)
{
    half transitionWidth = max(fwidth(signedDistance), 1.0e-4h);
    return smoothstep(-transitionWidth, transitionWidth, signedDistance);
}

half MotuCoherentMask(
    half source,
    half noise,
    half noiseStrength,
    half blendWidth)
{
    half distance = saturate(source) - (0.5h + noise * noiseStrength);
    half transition = max(max(blendWidth, fwidth(distance)), 1.0e-4h);
    return smoothstep(-transition, transition, distance);
}

half MotuTerrainWeightPatch(half weight, half noise, half blendWidth)
{
    // A coherent threshold breaks up partial coverage. Both endpoints keep
    // their full strength, even where the interpolated input changes quickly.
    half interior = 4.0h * weight * (1.0h - weight);
    half signal = saturate(weight + noise * interior);
    half transition = min(max(blendWidth, fwidth(signal)), 0.5h);
    return smoothstep(0.5h - transition, 0.5h + transition, signal);
}

MotuTerrainNoise MotuSampleTerrainNoise(float3 localPosition)
{
    float3 position = MotuTerrainFieldPosition(localPosition) / max(_CliffNoisePeriod, 1.0);
    MotuTerrainNoise result;
    result.broad = tex3D(_CliffNoise3D, position).rgb * 2.0h - 1.0h;
    result.macro = tex3D(
        _CliffNoise3D,
        position * (1.0 / 3.0) + float3(0.23, 0.71, 0.41)).rgb * 2.0h - 1.0h;
    result.patch = tex3D(
        _CliffNoise3D,
        position * _RockPatchNoiseDetailScale + float3(0.67, 0.31, 0.91)).rgb;
    result.detail = tex3D(
        _CliffNoise3D,
        position * (_RockPatchNoiseDetailScale * 4.0)
            + float3(0.11, 0.83, 0.47)).rgb * 2.0h - 1.0h;
    return result;
}

MotuTerrainCoverage MotuBuildTerrainCoverage(
    float3 localPosition,
    half4 material,
    half4 environment)
{
    MotuTerrainCoverage result;
    result.noise = MotuSampleTerrainNoise(localPosition);
    half elevation = localPosition.y;

    // Native vertex weights are dirt in the substrate, grass, river bed,
    // and sand. Exposed stone is the complement of the dirt blend.
    half dirtStrength = saturate(material.r);
    half dirtPatch = dot(result.noise.patch.rg, half2(0.65h, 0.35h));
    result.dirt = MotuTerrainWeightPatch(
        dirtStrength,
        (dirtPatch * 2.0h - 1.0h) * _RockBoundaryNoiseStrength,
        0.18h);
    result.rock = 1.0h - result.dirt;
    result.topProjection = smoothstep(0.0h, 0.30h, dirtStrength);

    half riverNoise = clamp(
        dot(result.noise.broad, half3(0.577h, -0.577h, 0.577h)),
        -1.0h,
        1.0h);
    result.river = MotuCoherentMask(
        material.b,
        riverNoise,
        _RiverEdgeNoiseStrength,
        _RiverEdgeBlendWidth);

    float2 fieldXZ = MotuTerrainFieldPosition(localPosition).xz;
    half sandStrength = saturate(material.a) * (1.0h - result.river);
    float2 sandUv = fieldXZ / max(_SandPatchNoiseWorldSize, 0.1)
        + float2(0.37, 0.73);
    half2 sandLayers = tex2D(_GrassPatchNoise, sandUv).rg;
    half sandPatch = sandLayers.r * 0.40h + sandLayers.g * 0.60h;
    half beachNoise = clamp(
        result.noise.detail.g * 0.65h
            + result.noise.detail.b * 0.35h,
        -1.0h,
        1.0h);
    half beachDistance = sandPatch
        - (1.0h - sandStrength)
        + beachNoise * _BeachEdgeNoiseStrength;
    half beachTransition = max(
        max(_BeachEdgeBlendWidth, fwidth(beachDistance)),
        1.0e-4h);
    result.beach = smoothstep(
        -beachTransition,
        beachTransition,
        beachDistance)
        * step(1.0e-4h, sandStrength);
    result.beach = max(
        result.beach,
        step(elevation, 0.0h) * sandStrength);

    half forestNoise = clamp(
        result.noise.detail.g * 0.70h + result.noise.detail.b * 0.30h,
        -1.0h,
        1.0h);
    result.forestFloor = MotuCoherentMask(
        environment.x,
        forestNoise,
        _ForestFloorEdgeNoiseStrength,
        _ForestFloorEdgeBlendWidth)
        * step(0.0h, elevation);

    half stonesNoise = clamp(
        result.noise.detail.r * 0.45h + result.noise.detail.b * 0.55h,
        -1.0h,
        1.0h);
    result.stones = MotuCoherentMask(
        environment.y,
        stonesNoise,
        _StonesEdgeNoiseStrength,
        _StonesEdgeBlendWidth)
        * step(0.0h, elevation);

    // Crop soil replaces grass and the loose organic/stone surface. Paths
    // and roads use the vertex profiles below, over the crop soil.
    half uncultivated = 1.0h - MotuWheatGroundCoverage(localPosition.xz);
    result.forestFloor *= uncultivated * result.topProjection;
    result.stones *= uncultivated * result.topProjection;

    // Paving blends into the underlying soil. Stair paths and ruts share the gravel mask.
    result.road = smoothstep(0.5h, 1.0h, environment.z);
    result.gravel = saturate(1.0h - environment.w) * (1.0h - result.road);

    float noisySnowLine = _SnowLine
        + result.noise.macro.r * _SnowMacroNoiseMetres
        + result.noise.broad.g * _SnowEdgeNoiseMetres;
    result.snow = MotuAntialiasedMask(elevation - noisySnowLine)
        * result.topProjection;

    float2 grassUv = fieldXZ / max(_GrassPatchNoiseWorldSize, 0.1);
    half2 grassLayers = tex2D(_GrassPatchNoise, grassUv).rg;
    half grassPatch = grassLayers.r * 0.65h + grassLayers.g * 0.35h;
    half grassObstruction = max(max(result.river, result.beach),
        max(result.forestFloor, result.stones));
    result.grass = MotuTerrainWeightPatch(
        saturate(material.g), grassPatch - 0.5h, 0.05h)
        * step(0.0h, elevation)
        * (1.0h - grassObstruction)
        * (1.0h - max(result.gravel, result.road))
        * (1.0h - result.snow)
        * uncultivated;

    result.coastalNoise = clamp(
        result.noise.detail.g * 0.65h
            + (result.noise.patch.r * 2.0h - 1.0h) * 0.35h,
        -1.0h,
        1.0h);
    return result;
}

float MotuLayerWorldSize(int layer)
{
    if (layer < 4)
    {
        return _TerrainLayerWorldSizesA[layer];
    }
    return _TerrainLayerWorldSizesB[layer - 4];
}

float2 MotuLayerUv(float3 localPosition, int layer)
{
    return MotuTerrainFieldPosition(localPosition).xz / max(MotuLayerWorldSize(layer), 0.01);
}

// Authored recipes use top-down XZ projection. Coherent patches leave some
// of the procedural ground showing through the recipe.
half MotuTopTextureWeight(half topProjection, half coherentNoise)
{
    half patchSignal = coherentNoise + lerp(-0.10h, 0.25h, topProjection);
    return min(topProjection, smoothstep(-0.22h, 0.22h, patchSignal));
}

half MotuDirtTextureProjection(half topProjection, half coherentNoise, half normalUp)
{
    // Baked dirt weights can intentionally cover steep cut banks. Fade just
    // the top-down recipe from about 40 to 50 degrees around cos(45 degrees).
    half slopeFade = smoothstep(0.6471h, 0.7671h, normalUp);
    return MotuTopTextureWeight(topProjection, coherentNoise) * slopeFade;
}

MotuBaseWeights MotuHeightBlendBase(
    half4 candidatesA,
    half2 candidatesB,
    half4 heightsA,
    half2 heightsB)
{
    half4 activityA = 4.0h * candidatesA * (1.0h - candidatesA);
    half2 activityB = 4.0h * candidatesB * (1.0h - candidatesB);
    half4 scoresA = candidatesA
        + (heightsA * 2.0h - 1.0h)
            * _TerrainHeightInfluencesA
            * activityA
            * 0.5h;
    half2 scoresB = candidatesB
        + (heightsB * 2.0h - 1.0h)
            * _TerrainHeightInfluencesB.xy
            * activityB
            * 0.5h;
    half peak = max(max(max(scoresA.x, scoresA.y), max(scoresA.z, scoresA.w)),
        max(scoresB.x, scoresB.y));
    half depth = max(_TerrainHeightBlendDepth, 1.0e-3h);
    MotuBaseWeights result;
    result.a = candidatesA * saturate((scoresA - peak + depth) / depth);
    result.b = candidatesB * saturate((scoresB - peak + depth) / depth);
    half total = dot(result.a, half4(1.0h, 1.0h, 1.0h, 1.0h))
        + result.b.x + result.b.y;
    if (total <= 1.0e-4h)
    {
        result.a = half4(1.0h, 0.0h, 0.0h, 0.0h);
        result.b = half2(0.0h, 0.0h);
        return result;
    }
    result.a /= total;
    result.b /= total;
    return result;
}

void MotuTerrainBaseCandidates(
    MotuTerrainCoverage coverage,
    out half4 candidatesA,
    out half2 candidatesB)
{
    half overlay = max(max(coverage.forestFloor, coverage.river),
        max(coverage.beach, coverage.stones));
    half substrate = 1.0h - overlay;
    candidatesA = half4(substrate * coverage.dirt, coverage.forestFloor,
        substrate * coverage.rock, coverage.river);
    candidatesB = half2(coverage.beach, coverage.stones);
}

MotuBaseWeights MotuTerrainUnderlyingWeights(MotuTerrainCoverage coverage)
{
    half substrate = 1.0h - coverage.beach;
    MotuBaseWeights weights;
    weights.a = half4(substrate * coverage.dirt, 0.0h, substrate * coverage.rock, 0.0h);
    weights.b = half2(coverage.beach, 0.0h);
    return weights;
}

#endif
