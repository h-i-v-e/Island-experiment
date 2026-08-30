#ifndef MOTU_TERRAIN_COVERAGE_COMMON_INCLUDED
#define MOTU_TERRAIN_COVERAGE_COMMON_INCLUDED

#define MOTU_LAYER_DIRT 0
#define MOTU_LAYER_FOREST_FLOOR 1
#define MOTU_LAYER_ROCK 2
#define MOTU_LAYER_RIVER_BED 3
#define MOTU_LAYER_BEACH 4
#define MOTU_LAYER_STONES 5

sampler3D _CliffNoise3D;
sampler2D _GrassPatchNoise;
UNITY_DECLARE_TEX2DARRAY(_TerrainMaskArray);

float _CliffNoisePeriod;
half _RockPatchNoiseDetailScale;
half _CliffNormalCutoff;
half _CliffBoundaryNoiseStrength;
half _RockBoundaryNoiseStrength;
half _SandRockSlopeThreshold;
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
half4 _TerrainHeightInfluencesA;
half4 _TerrainHeightInfluencesB;
half _TerrainHeightBlendDepth;
half _TopTextureFadeOutSlope;
half _SteepStoneBlendWidth;

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
    half rock;
    half river;
    half beach;
    half forestFloor;
    half stones;
    half grass;
    half snow;
    half cliff;
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

MotuTerrainNoise MotuSampleTerrainNoise(float3 localPosition)
{
    float3 position = localPosition / max(_CliffNoisePeriod, 1.0);
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
    half3 worldNormal,
    half4 material,
    half2 environment)
{
    MotuTerrainCoverage result;
    result.noise = MotuSampleTerrainNoise(localPosition);
    half elevation = localPosition.y;
    half slope = 1.0h - saturate(worldNormal.y);
    half hardness = saturate(material.r);
    half looseCover = saturate(material.g);
    half riverBed = saturate(material.b);

    half cliffNoise = clamp(
        result.noise.broad.r * 0.65h + result.noise.macro.g * 0.35h,
        -1.0h,
        1.0h);
    half cutoffNormal = worldNormal.y
        - cliffNoise * _CliffBoundaryNoiseStrength;
    result.cliff = MotuAntialiasedMask(_CliffNormalCutoff - cutoffNormal);

    half forcedRockNoise = clamp(
        (result.noise.patch.b * 2.0h - 1.0h) * 0.35h
            + result.noise.detail.r * 0.65h,
        -1.0h,
        1.0h);
    half forcedRockBlendWidth = max(_RiverEdgeBlendWidth, 0.001h);
    half forcedRockBlendStart = saturate(
        1.0h
            - forcedRockBlendWidth
            + forcedRockNoise * forcedRockBlendWidth * 0.75h);
    half forcedRock = smoothstep(
        forcedRockBlendStart,
        1.0h,
        hardness);
    half geologyStrength = saturate(slope * lerp(1.3h, 3.0h, hardness));
    half geologyNoise = result.noise.patch.r * 0.65h
        + result.noise.patch.g * 0.35h;
    half geologyDistance = geologyNoise - (1.0h - geologyStrength);
    half geologyRock = smoothstep(
        -max(0.20h, fwidth(geologyDistance)),
        max(0.20h, fwidth(geologyDistance)),
        geologyDistance)
        * smoothstep(0.0h, 0.20h, geologyStrength);
    // Hardness describes the underlying geology, not exposed stone. Loose
    // cover suppresses that material-driven rock while leaving explicit
    // slope thresholds free to expose cliffs through the soil cover.
    half exposedGeologyRock = max(geologyRock, forcedRock)
        * (1.0h - looseCover);

    half riverNoise = clamp(
        dot(result.noise.broad, half3(0.577h, -0.577h, 0.577h)),
        -1.0h,
        1.0h);
    result.river = MotuCoherentMask(
        riverBed,
        riverNoise,
        _RiverEdgeNoiseStrength,
        _RiverEdgeBlendWidth);

    half seaProximity = saturate(material.a);
    half sandAltitude = 1.0h - smoothstep(2.0h, 4.0h, elevation);
    half sandRichness = looseCover
        * seaProximity
        * sandAltitude
        * (1.0h - result.river);
    float2 sandUv = localPosition.xz / max(_SandPatchNoiseWorldSize, 0.1)
        + float2(0.37, 0.73);
    half2 sandLayers = tex2D(_GrassPatchNoise, sandUv).rg;
    half sandPatch = sandLayers.r * 0.40h + sandLayers.g * 0.60h;
    half beachNoise = clamp(
        result.noise.detail.g * 0.65h
            + result.noise.detail.b * 0.35h,
        -1.0h,
        1.0h);
    half beachDistance = sandPatch
        - (1.0h - sandRichness)
        + beachNoise * _BeachEdgeNoiseStrength;
    half beachTransition = max(
        max(_BeachEdgeBlendWidth, fwidth(beachDistance)),
        1.0e-4h);
    result.beach = smoothstep(
        -beachTransition,
        beachTransition,
        beachDistance)
        * step(1.0e-4h, sandRichness);
    result.beach = max(
        result.beach,
        step(elevation, 0.0h) * seaProximity * (1.0h - result.river));

    half rockBoundaryNoise = clamp(
        result.noise.broad.b * 0.55h + result.noise.macro.b * 0.45h,
        -1.0h,
        1.0h);
    half sandRockThreshold = _SandRockSlopeThreshold
        + rockBoundaryNoise * _RockBoundaryNoiseStrength * 0.25h;
    half beachRock = result.beach
        * MotuAntialiasedMask(slope - sandRockThreshold);
    result.rock = max(exposedGeologyRock, max(result.cliff, beachRock));

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

    float2 grassUv = localPosition.xz / max(_GrassPatchNoiseWorldSize, 0.1);
    half2 grassLayers = tex2D(_GrassPatchNoise, grassUv).rg;
    half grassPatch = grassLayers.r * 0.65h + grassLayers.g * 0.35h;
    half grassDistance = grassPatch - (1.0h - looseCover);
    result.grass = smoothstep(
        -max(0.18h, fwidth(grassDistance)),
        max(0.18h, fwidth(grassDistance)),
        grassDistance)
        * smoothstep(0.0h, 0.20h, looseCover)
        * step(0.0h, elevation);

    float noisySnowLine = _SnowLine
        + result.noise.macro.r * _SnowMacroNoiseMetres
        + result.noise.broad.g * _SnowEdgeNoiseMetres;
    result.snow = MotuAntialiasedMask(elevation - noisySnowLine)
        * (1.0h - result.cliff);
    result.coastalNoise = clamp(
        result.noise.detail.g * 0.65h
            + (result.noise.patch.r * 2.0h - 1.0h) * 0.35h,
        -1.0h,
        1.0h);

    // Build a coherent shoulder on the shallower side of the cutoff. The
    // perturbation vanishes at both ends, so the blend starts cleanly and
    // still converges to exclusive procedural stone at the exact cutoff.
    half stoneCutoffDegrees = clamp(_TopTextureFadeOutSlope, 1.0h, 89.0h);
    half stoneBlendStartDegrees = max(
        stoneCutoffDegrees - max(_SteepStoneBlendWidth, 0.1h),
        0.0h);
    half stoneCutoffUp = cos(radians(stoneCutoffDegrees));
    half stoneBlendStartUp = max(
        cos(radians(stoneBlendStartDegrees)),
        stoneCutoffUp + 1.0e-4h);
    half stoneSlopeProgress = 1.0h - smoothstep(
        stoneCutoffUp,
        stoneBlendStartUp,
        saturate(worldNormal.y));
    half stoneSlopeInterior = 4.0h
        * stoneSlopeProgress
        * (1.0h - stoneSlopeProgress);
    half coherentStoneProgress = saturate(
        stoneSlopeProgress
            + cliffNoise
                * _CliffBoundaryNoiseStrength
                * stoneSlopeInterior);
    half steepStone = smoothstep(0.0h, 1.0h, coherentStoneProgress);
    half nonSteep = 1.0h - steepStone;
    result.rock = max(result.rock, steepStone);
    result.river *= nonSteep;
    result.beach *= nonSteep;
    result.forestFloor *= nonSteep;
    result.stones *= nonSteep;
    result.grass *= nonSteep;
    result.snow *= nonSteep;
    result.cliff = max(result.cliff, steepStone);
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

half MotuLayerHeightInfluence(int layer)
{
    if (layer < 4)
    {
        return _TerrainHeightInfluencesA[layer];
    }
    return _TerrainHeightInfluencesB[layer - 4];
}

float2 MotuLayerUv(float3 localPosition, int layer)
{
    return localPosition.xz / max(MotuLayerWorldSize(layer), 0.01);
}

half2 MotuSampleTerrainMask(float3 localPosition, int layer)
{
    return UNITY_SAMPLE_TEX2DARRAY(
        _TerrainMaskArray,
        float3(MotuLayerUv(localPosition, layer), layer)).rg;
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

MotuBaseWeights MotuResolveBaseWeights(
    MotuTerrainCoverage coverage,
    half4 heightsA,
    half2 heightsB)
{
    half strongest = max(
        max(max(coverage.forestFloor, coverage.rock), coverage.river),
        max(coverage.beach, coverage.stones));
    half dirt = saturate(1.0h - strongest);
    return MotuHeightBlendBase(
        half4(dirt, coverage.forestFloor, coverage.rock, coverage.river),
        half2(coverage.beach, coverage.stones),
        heightsA,
        heightsB);
}

half MotuGrassEligibleDirtWeight(
    float3 localPosition,
    MotuTerrainCoverage coverage)
{
    half hardExclusion = max(
        max(coverage.rock, coverage.river),
        max(coverage.beach, coverage.snow));
    if (hardExclusion >= 0.5h)
    {
        return 0.0h;
    }

    half dirtHeight = MotuSampleTerrainMask(
        localPosition,
        MOTU_LAYER_DIRT).r;
    half forestHeight = MotuSampleTerrainMask(
        localPosition,
        MOTU_LAYER_FOREST_FLOOR).r;
    half stonesHeight = MotuSampleTerrainMask(
        localPosition,
        MOTU_LAYER_STONES).r;
    half dirt = saturate(
        1.0h - max(coverage.forestFloor, coverage.stones));
    MotuBaseWeights weights = MotuHeightBlendBase(
        half4(dirt, coverage.forestFloor, 0.0h, 0.0h),
        half2(0.0h, coverage.stones),
        half4(dirtHeight, forestHeight, 0.5h, 0.5h),
        half2(0.5h, stonesHeight));
    return weights.a.x;
}

#endif
