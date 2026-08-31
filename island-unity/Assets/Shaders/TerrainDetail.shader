Shader "Motu/Terrain Unified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        _RockColor ("Exposed Rock", Color) = (0.30, 0.32, 0.29, 1)
        _ForestFloorColor ("Forest Floor Tint", Color) = (1, 1, 1, 1)
        _StonesColor ("Fallen Stones Tint", Color) = (1, 1, 1, 1)
        _GroundDirtColor ("Dirt", Color) = (0.09, 0.055, 0.026, 1)
        _SandColor ("Sand", Color) = (0.62, 0.57, 0.34, 1)
        [NoScaleOffset] _TerrainAlbedoArray ("Runtime Terrain Albedo Array", 2DArray) = "" {}
        [NoScaleOffset] _TerrainNormalArray ("Runtime Terrain Normal Array", 2DArray) = "" {}
        [NoScaleOffset] _TerrainMaskArray ("Runtime Terrain Height + Occlusion Array", 2DArray) = "" {}
        _TerrainLayerWorldSizesA ("Layer Sizes: Dirt Forest Rock River", Vector) = (2, 2, 4, 2)
        _TerrainLayerWorldSizesB ("Layer Sizes: Beach Stones", Vector) = (3, 2, 0, 0)
        _TerrainHeightInfluencesA ("Height Influence: Dirt Forest Rock River", Vector) = (1, 1, 1, 1)
        _TerrainHeightInfluencesB ("Height Influence: Beach Stones", Vector) = (0.65, 0.65, 0, 0)
        _TerrainNormalStrengthsA ("Normal Strength: Dirt Forest Rock River", Vector) = (1, 1, 1, 1)
        _TerrainNormalStrengthsB ("Normal Strength: Beach Stones", Vector) = (1, 1, 0, 0)
        _TerrainParallaxDepthsA ("Parallax: Dirt Forest Rock River", Vector) = (0.02, 0.018, 0.05, 0.025)
        _TerrainParallaxDepthsB ("Parallax: Beach Stones", Vector) = (0.015, 0.025, 0, 0)
        [HideInInspector] _TerrainParallaxNeutralHeightsA ("Neutral Height: Dirt Forest Rock River", Vector) = (0.5, 0.428571, 0.5, 0)
        [HideInInspector] _TerrainParallaxNeutralHeightsB ("Neutral Height: Beach Stones", Vector) = (0.5, 0.304348, 0, 0)
        _TerrainOcclusionStrengthsA ("Occlusion: Dirt Forest Rock River", Vector) = (0.55, 0.7, 0.5, 0.65)
        _TerrainOcclusionStrengthsB ("Occlusion: Beach Stones", Vector) = (0.45, 0.75, 0, 0)
        _TerrainHeightBlendDepth ("Height Blend Depth", Range(0.02, 0.5)) = 0.18
        _TopTextureFadeOutSlope ("Top Texture Cutoff Slope (degrees)", Range(1, 89)) = 45
        _SteepStoneBlendWidth ("Stone Slope Blend Width (degrees)", Range(0.5, 20)) = 8

        [NoScaleOffset] _WorldNormal ("Shared World Normal", 2D) = "bump" {}
        [PerRendererData] _WorldNormalWeight ("World Normal Weight", Float) = 1
        [NoScaleOffset] _Occlusion ("Shared Occlusion", 2D) = "white" {}
        _OcclusionStrength ("Occlusion Strength", Range(0, 1)) = 0.25
        [NoScaleOffset] _CliffNoise3D ("Cliff 3D Noise", 3D) = "gray" {}
        [NoScaleOffset] _GrassPatchNoise ("Grass Patch Noise", 2D) = "white" {}

        _SnowLine ("Snow Line (metres)", Float) = 100
        _SnowEdgeNoiseMetres ("Snow Edge Noise (metres)", Range(0, 10)) = 2.5
        _SnowMacroNoiseMetres ("Snow Macro Noise (metres)", Range(0, 40)) = 18
        _SandPatchNoiseWorldSize ("Sand Patch Repeat (metres)", Float) = 32
        _GrassPatchNoiseWorldSize ("Grass Patch Repeat (metres)", Float) = 32
        _RiverEdgeNoiseStrength ("River Edge Noise Strength", Range(0, 0.45)) = 0.20
        _RiverEdgeBlendWidth ("River Edge Blend Width", Range(0.01, 0.5)) = 0.20
        _ForestFloorEdgeNoiseStrength ("Forest Floor Edge Noise Strength", Range(0, 0.45)) = 0.22
        _ForestFloorEdgeBlendWidth ("Forest Floor Edge Blend Width", Range(0.01, 0.5)) = 0.035
        _StonesEdgeNoiseStrength ("Fallen Stones Edge Noise Strength", Range(0, 0.45)) = 0.22
        _StonesEdgeBlendWidth ("Fallen Stones Edge Blend Width", Range(0.01, 0.5)) = 0.16
        _BeachEdgeNoiseStrength ("Beach Edge Noise Strength", Range(0, 0.45)) = 0.18
        _BeachEdgeBlendWidth ("Beach Edge Blend Width", Range(0.01, 0.5)) = 0.18
        _CliffNormalCutoff ("Cliff Up-Normal Cutoff", Range(0, 1)) = 0.55
        _CliffBoundaryNoiseStrength ("Cliff Boundary Noise Strength", Range(0, 0.5)) = 0.30
        _RockBoundaryNoiseStrength ("Sand Rock Edge Noise Strength", Range(0, 0.4)) = 0.18
        _SandRockSlopeThreshold ("Sand Rock Slope Threshold", Range(0, 0.5)) = 0.10
        _CliffNoisePeriod ("Cliff Noise Period (metres)", Float) = 160
        _RockPatchNoiseDetailScale ("Rock Mask Detail Frequency", Range(1, 32)) = 8

        _CliffNoiseDetailScale ("Rock and Cliff Detail Frequency", Range(2, 32)) = 16
        _CliffNormalStrength ("Rock and Cliff Normal Strength", Range(0, 0.5)) = 0.12
        _GrassNormalDetailScale ("Grass Detail Frequency", Range(16, 192)) = 96
        _SandNormalDetailScale ("Sand Detail Frequency", Range(32, 1024)) = 768
        _SnowNormalDetailScale ("Snow Detail Frequency", Range(16, 256)) = 96
        _DirtNormalStrength ("Bare Dirt Normal Strength", Range(0, 0.5)) = 0.05
        _GrassNormalStrength ("Grass Normal Strength", Range(0, 0.5)) = 0.22
        _SandNormalStrength ("Sand Normal Strength", Range(0, 0.5)) = 0.10
        _SnowNormalStrength ("Snow Normal Strength", Range(0, 0.5)) = 0.08

        _GrassColorA ("Grass Colour A", Color) = (0.18, 0.46, 0.14, 1)
        _GrassColorB ("Grass Colour B", Color) = (0.34, 0.50, 0.14, 1)
        _GrassColorNoiseWorldSize ("Grass Colour Noise Repeat (metres)", Float) = 2048
        [HideInInspector] _GrassWindDirection ("Grass Wind Direction", Vector) = (1, 0, 0.35, 0)
        [HideInInspector] _GrassWindStrength ("Grass Wind Bend (metres)", Range(0, 0.25)) = 0.07
        [HideInInspector] _GrassWindSpeed ("Grass Wind Speed (metres/second)", Range(0, 10)) = 1.8
        [HideInInspector] _GrassWindWorldSize ("Grass Wind Gust Size (metres)", Range(1, 64)) = 12
        [HideInInspector] _GrassWindNormalStrength ("Grass Wind Normal Strength", Range(0, 1)) = 0.35
        [HideInInspector] _GrassEnabled ("Local Grass Enabled", Float) = 0
        [HideInInspector] _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
        _GroundDirtCoreRadius ("Dirt Core Radius (metres)", Float) = 0.5
        _GroundDirtFadeWidth ("Dirt Fade Width (metres)", Float) = 2

        _WetBankBlendExponent ("Wet Bank Blend Exponent", Range(0.2, 1)) = 0.45
        _WetDarkening ("Wet Surface Darkening", Range(0, 0.75)) = 0.24
        _WetSmoothness ("Wet Surface Smoothness", Range(0, 1)) = 0.65
        _WetSpecularStrength ("Wet Surface Highlight Strength", Range(0, 1)) = 0.55
        _CoastalWetnessNoiseStrength ("Coastal Wetness Noise Strength", Range(0, 0.45)) = 0.18
        _CoastalWetnessBlendWidth ("Coastal Wetness Blend Width", Range(0.01, 0.5)) = 0.06
        [HideInInspector] _TerrainDebugView ("Terrain Debug View", Float) = 0
    }

    SubShader
    {
        Tags { "RenderType"="Opaque" "Queue"="Geometry" "MotuReflection"="Terrain" }
        LOD 300

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma shader_feature_local_fragment _ MOTU_TERRAIN_LOD1 MOTU_TERRAIN_LOD2

            #include "UnityCG.cginc"
            #include "Lighting.cginc"
            #include "AutoLight.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 uv : TEXCOORD0;
                float2 environment : TEXCOORD1;
                float4 material : COLOR;
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float2 uv : TEXCOORD0;
                float3 worldPosition : TEXCOORD1;
                float3 geometricWorldNormal : TEXCOORD2;
                SHADOW_COORDS(3)
                UNITY_FOG_COORDS(4)
                half4 material : TEXCOORD5;
                float3 islandLocalPosition : TEXCOORD6;
                half2 environment : TEXCOORD7;
                half coastalWetness : TEXCOORD8;
            };

            sampler2D _WorldNormal;
            sampler2D _Occlusion;
            UNITY_DECLARE_TEX2DARRAY(_TerrainAlbedoArray);
            UNITY_DECLARE_TEX2DARRAY(_TerrainNormalArray);

            fixed4 _Color;
            fixed4 _RockColor;
            fixed4 _ForestFloorColor;
            fixed4 _StonesColor;
            fixed4 _GroundDirtColor;
            fixed4 _SandColor;
            half _WorldNormalWeight;
            half _OcclusionStrength;
            half4 _TerrainNormalStrengthsA;
            half4 _TerrainNormalStrengthsB;
            float4 _TerrainParallaxDepthsA;
            float4 _TerrainParallaxDepthsB;
            half4 _TerrainParallaxNeutralHeightsA;
            half4 _TerrainParallaxNeutralHeightsB;
            half4 _TerrainOcclusionStrengthsA;
            half4 _TerrainOcclusionStrengthsB;
            half _CliffNoiseDetailScale;
            half _CliffNormalStrength;
            half _GrassNormalDetailScale;
            half _SandNormalDetailScale;
            half _SnowNormalDetailScale;
            half _DirtNormalStrength;
            half _GrassNormalStrength;
            half _SandNormalStrength;
            half _SnowNormalStrength;
            fixed4 _GrassColorA;
            fixed4 _GrassColorB;
            float _GrassColorNoiseWorldSize;
            float4 _GrassWindDirection;
            float _GrassWindStrength;
            float _GrassWindSpeed;
            float _GrassWindWorldSize;
            half _GrassWindNormalStrength;
            half _GrassEnabled;
            float3 _GrassPlayerPosition;
            float _GroundDirtCoreRadius;
            float _GroundDirtFadeWidth;
            half _WetBankBlendExponent;
            half _WetDarkening;
            half _WetSmoothness;
            half _WetSpecularStrength;
            half _CoastalWetnessNoiseStrength;
            half _CoastalWetnessBlendWidth;
            half _TerrainDebugView;
            float4x4 _IslandWorldToLocal;

            #include "TerrainCoverageCommon.cginc"
            #include "GrassWindCommon.cginc"
            #include "CloudCommon.cginc"

            float MotuLayerParallaxDepth(int layer)
            {
                if (layer < 4) return _TerrainParallaxDepthsA[layer];
                return _TerrainParallaxDepthsB[layer - 4];
            }

            half MotuLayerParallaxNeutralHeight(int layer)
            {
                if (layer < 4) return _TerrainParallaxNeutralHeightsA[layer];
                return _TerrainParallaxNeutralHeightsB[layer - 4];
            }

            half MotuLayerNormalStrength(int layer)
            {
                if (layer < 4) return _TerrainNormalStrengthsA[layer];
                return _TerrainNormalStrengthsB[layer - 4];
            }

            half MotuLayerOcclusionStrength(int layer)
            {
                if (layer < 4) return _TerrainOcclusionStrengthsA[layer];
                return _TerrainOcclusionStrengthsB[layer - 4];
            }

            half BlendedParallaxSurfaceDepth(
                float2 localPositionXZ,
                MotuBaseWeights weights,
                half4 projectionA,
                half2 projectionB)
            {
                half surfaceDepth = 0.0h;
                half dirtContribution = weights.a.x;
                UNITY_BRANCH
                if (dirtContribution > 0.001h)
                {
                    half height = MotuLayerParallaxNeutralHeight(MOTU_LAYER_DIRT);
                    UNITY_BRANCH
                    if (projectionA.x > 0.001h)
                    {
                        half sampledHeight = UNITY_SAMPLE_TEX2DARRAY(
                            _TerrainMaskArray,
                            float3(
                                localPositionXZ / max(
                                    MotuLayerWorldSize(MOTU_LAYER_DIRT),
                                    0.01),
                                MOTU_LAYER_DIRT)).r;
                        height = lerp(height, sampledHeight, projectionA.x);
                    }
                    surfaceDepth += dirtContribution
                        * MotuLayerParallaxDepth(MOTU_LAYER_DIRT)
                        * (1.0h - height);
                }
                half forestContribution = weights.a.y;
                UNITY_BRANCH
                if (forestContribution > 0.001h)
                {
                    half height = MotuLayerParallaxNeutralHeight(
                        MOTU_LAYER_FOREST_FLOOR);
                    UNITY_BRANCH
                    if (projectionA.y > 0.001h)
                    {
                        half sampledHeight = UNITY_SAMPLE_TEX2DARRAY(
                            _TerrainMaskArray,
                            float3(
                                localPositionXZ / max(
                                    MotuLayerWorldSize(MOTU_LAYER_FOREST_FLOOR),
                                    0.01),
                                MOTU_LAYER_FOREST_FLOOR)).r;
                        height = lerp(height, sampledHeight, projectionA.y);
                    }
                    surfaceDepth += forestContribution
                        * MotuLayerParallaxDepth(MOTU_LAYER_FOREST_FLOOR)
                        * (1.0h - height);
                }
                half rockContribution = weights.a.z;
                UNITY_BRANCH
                if (rockContribution > 0.001h)
                {
                    half height = MotuLayerParallaxNeutralHeight(MOTU_LAYER_ROCK);
                    UNITY_BRANCH
                    if (projectionA.z > 0.001h)
                    {
                        half sampledHeight = UNITY_SAMPLE_TEX2DARRAY(
                            _TerrainMaskArray,
                            float3(
                                localPositionXZ / max(
                                    MotuLayerWorldSize(MOTU_LAYER_ROCK),
                                    0.01),
                                MOTU_LAYER_ROCK)).r;
                        height = lerp(height, sampledHeight, projectionA.z);
                    }
                    surfaceDepth += rockContribution
                        * MotuLayerParallaxDepth(MOTU_LAYER_ROCK)
                        * (1.0h - height);
                }
                half riverContribution = weights.a.w;
                UNITY_BRANCH
                if (riverContribution > 0.001h)
                {
                    half height = MotuLayerParallaxNeutralHeight(
                        MOTU_LAYER_RIVER_BED);
                    UNITY_BRANCH
                    if (projectionA.w > 0.001h)
                    {
                        half sampledHeight = UNITY_SAMPLE_TEX2DARRAY(
                            _TerrainMaskArray,
                            float3(
                                localPositionXZ / max(
                                    MotuLayerWorldSize(MOTU_LAYER_RIVER_BED),
                                    0.01),
                                MOTU_LAYER_RIVER_BED)).r;
                        height = lerp(height, sampledHeight, projectionA.w);
                    }
                    surfaceDepth += riverContribution
                        * MotuLayerParallaxDepth(MOTU_LAYER_RIVER_BED)
                        * (1.0h - height);
                }
                half beachContribution = weights.b.x;
                UNITY_BRANCH
                if (beachContribution > 0.001h)
                {
                    half height = MotuLayerParallaxNeutralHeight(MOTU_LAYER_BEACH);
                    UNITY_BRANCH
                    if (projectionB.x > 0.001h)
                    {
                        half sampledHeight = UNITY_SAMPLE_TEX2DARRAY(
                            _TerrainMaskArray,
                            float3(
                                localPositionXZ / max(
                                    MotuLayerWorldSize(MOTU_LAYER_BEACH),
                                    0.01),
                                MOTU_LAYER_BEACH)).r;
                        height = lerp(height, sampledHeight, projectionB.x);
                    }
                    surfaceDepth += beachContribution
                        * MotuLayerParallaxDepth(MOTU_LAYER_BEACH)
                        * (1.0h - height);
                }
                half stonesContribution = weights.b.y;
                UNITY_BRANCH
                if (stonesContribution > 0.001h)
                {
                    half height = MotuLayerParallaxNeutralHeight(MOTU_LAYER_STONES);
                    UNITY_BRANCH
                    if (projectionB.y > 0.001h)
                    {
                        half sampledHeight = UNITY_SAMPLE_TEX2DARRAY(
                            _TerrainMaskArray,
                            float3(
                                localPositionXZ / max(
                                    MotuLayerWorldSize(MOTU_LAYER_STONES),
                                    0.01),
                                MOTU_LAYER_STONES)).r;
                        height = lerp(height, sampledHeight, projectionB.y);
                    }
                    surfaceDepth += stonesContribution
                        * MotuLayerParallaxDepth(MOTU_LAYER_STONES)
                        * (1.0h - height);
                }
                return surfaceDepth;
            }

            float2 BlendedParallaxWorldOffset(
                float2 localPositionXZ,
                float3 localViewDirection,
                MotuBaseWeights weights,
                half4 projectionA,
                half2 projectionB)
            {
                half maximumDepth = weights.a.x
                        * MotuLayerParallaxDepth(MOTU_LAYER_DIRT)
                    + weights.a.y
                        * MotuLayerParallaxDepth(MOTU_LAYER_FOREST_FLOOR)
                    + weights.a.z * MotuLayerParallaxDepth(MOTU_LAYER_ROCK)
                    + weights.a.w * MotuLayerParallaxDepth(MOTU_LAYER_RIVER_BED)
                    + weights.b.x * MotuLayerParallaxDepth(MOTU_LAYER_BEACH)
                    + weights.b.y * MotuLayerParallaxDepth(MOTU_LAYER_STONES);
                UNITY_BRANCH
                if (maximumDepth <= 1.0e-4h)
                {
                    return float2(0.0, 0.0);
                }

                float2 viewRay = localViewDirection.xz
                    / max(abs(localViewDirection.y), 0.2);
                half neutralDepth = weights.a.x
                        * MotuLayerParallaxDepth(MOTU_LAYER_DIRT)
                        * (1.0h - MotuLayerParallaxNeutralHeight(
                            MOTU_LAYER_DIRT))
                    + weights.a.y
                        * MotuLayerParallaxDepth(MOTU_LAYER_FOREST_FLOOR)
                        * (1.0h - MotuLayerParallaxNeutralHeight(
                            MOTU_LAYER_FOREST_FLOOR))
                    + weights.a.z * MotuLayerParallaxDepth(MOTU_LAYER_ROCK)
                        * (1.0h - MotuLayerParallaxNeutralHeight(MOTU_LAYER_ROCK))
                    + weights.a.w
                        * MotuLayerParallaxDepth(MOTU_LAYER_RIVER_BED)
                        * (1.0h - MotuLayerParallaxNeutralHeight(
                            MOTU_LAYER_RIVER_BED))
                    + weights.b.x * MotuLayerParallaxDepth(MOTU_LAYER_BEACH)
                        * (1.0h - MotuLayerParallaxNeutralHeight(
                            MOTU_LAYER_BEACH))
                    + weights.b.y * MotuLayerParallaxDepth(MOTU_LAYER_STONES)
                        * (1.0h - MotuLayerParallaxNeutralHeight(
                            MOTU_LAYER_STONES));
                float2 rayStart = localPositionXZ + viewRay * neutralDepth;
                const half stepFraction = 0.125h;
                half depthStep = maximumDepth * stepFraction;
                half currentDepth = 0.0h;
                half currentSurface = BlendedParallaxSurfaceDepth(
                    rayStart,
                    weights,
                    projectionA,
                    projectionB);
                half previousDepth = currentDepth;
                half previousSurface = currentSurface;
                [unroll]
                for (int stepIndex = 0; stepIndex < 8; ++stepIndex)
                {
                    if (currentDepth >= currentSurface) break;
                    previousDepth = currentDepth;
                    previousSurface = currentSurface;
                    currentDepth += depthStep;
                    currentSurface = BlendedParallaxSurfaceDepth(
                        rayStart - viewRay * currentDepth,
                        weights,
                        projectionA,
                        projectionB);
                }
                half beforeIntersection = previousSurface - previousDepth;
                half afterIntersection = currentSurface - currentDepth;
                half denominator = beforeIntersection - afterIntersection;
                half intersection = abs(denominator) > 1.0e-4h
                    ? saturate(beforeIntersection / denominator)
                    : 0.0h;
                half hitDepth = lerp(
                    previousDepth,
                    currentDepth,
                    intersection);
                return viewRay * (neutralDepth - hitDepth);
            }

            half3 ArrayWorldNormal(float2 uv, int layer)
            {
                half3 tangentNormal = UNITY_SAMPLE_TEX2DARRAY(
                    _TerrainNormalArray,
                    float3(uv, layer)).xyz * 2.0h - 1.0h;
                half3 localNormal = normalize(half3(
                    tangentNormal.x,
                    tangentNormal.z,
                    tangentNormal.y));
                return normalize(mul((float3x3)unity_ObjectToWorld, localNormal));
            }

            half3 PerturbNormal(
                half3 normal,
                float3 noisePosition,
                half scale,
                float3 offset,
                half strength)
            {
                if (strength <= 0.0h) return normal;
                half3 detail = tex3D(
                    _CliffNoise3D,
                    noisePosition * scale + offset).rgb * 2.0h - 1.0h;
                detail -= normal * dot(detail, normal);
                return normalize(normal + detail * strength);
            }

            // Authored recipes use one top-down XZ projection only. The slope
            // gate and coherent patch mask both contain true zero/one plateaus,
            // leaving full procedural areas, full recipe/parallax areas, and
            // bounded transitions instead of a perpetual fractional mixture.
            half MotuTopTextureWeight(half localUp, half coherentNoise)
            {
                half cutoffDegrees = clamp(
                    _TopTextureFadeOutSlope,
                    1.0h,
                    89.0h);
                half fullyAvailableDegrees = max(
                    cutoffDegrees - max(_SteepStoneBlendWidth, 0.1h),
                    0.0h);
                half cutoffUp = cos(radians(cutoffDegrees));
                half fullyAvailableUp = max(
                    cos(radians(fullyAvailableDegrees)),
                    cutoffUp + 1.0e-4h);
                half slopeGate = smoothstep(
                    cutoffUp,
                    fullyAvailableUp,
                    localUp);
                half flatPreference = smoothstep(
                    fullyAvailableUp,
                    1.0h,
                    localUp);
                half patchSignal = coherentNoise
                    + lerp(-0.10h, 0.25h, flatPreference);
                half patchGate = smoothstep(-0.22h, 0.22h, patchSignal);
                return min(slopeGate, patchGate);
            }

            fixed4 ShadeDistantTerrain(
                VertexOutput input,
                half3 geometricNormal,
                half3 localGeometricNormal,
                float3 localPosition,
                MotuTerrainCoverage coverage,
                half4 candidatesA,
                half2 candidatesB)
            {
                MotuBaseWeights weights = MotuHeightBlendBase(
                    candidatesA,
                    candidatesB,
                    half4(0.5h, 0.5h, 0.5h, 0.5h),
                    half2(0.5h, 0.5h));
                half underlyingDirtCandidate = saturate(
                    1.0h - max(coverage.rock, coverage.beach));
                MotuBaseWeights underlyingWeights = MotuHeightBlendBase(
                    half4(underlyingDirtCandidate, 0.0h, coverage.rock, 0.0h),
                    half2(coverage.beach, 0.0h),
                    half4(0.5h, 0.5h, 0.5h, 0.5h),
                    half2(0.5h, 0.5h));
                half sandVariation = clamp(
                    coverage.noise.detail.r * 0.65h
                        + coverage.noise.macro.b * 0.35h,
                    -1.0h,
                    1.0h);
                fixed3 sand = _SandColor.rgb * (1.0h + sandVariation * 0.08h);
                fixed3 underlying = _GroundDirtColor.rgb * underlyingWeights.a.x
                    + _RockColor.rgb * underlyingWeights.a.z
                    + sand * underlyingWeights.b.x;
                fixed3 baseColor = _GroundDirtColor.rgb * weights.a.x
                    + underlying * 0.72h * weights.a.y
                    + _RockColor.rgb * weights.a.z
                    + underlying * 0.82h * weights.a.w
                    + sand * weights.b.x
                    + lerp(underlying, _RockColor.rgb, 0.35h) * weights.b.y;

                half grassCoverage = smoothstep(0.35h, 0.65h, coverage.grass)
                    * weights.a.x
                    * (1.0h - coverage.snow);
                half grassColorNoise = tex2D(
                    _GrassPatchNoise,
                    localPosition.xz / max(_GrassColorNoiseWorldSize, 1.0)).b;
                fixed3 grassColor = lerp(
                    _GrassColorA.rgb,
                    _GrassColorB.rgb,
                    smoothstep(0.1h, 0.9h, grassColorNoise));
                baseColor = lerp(baseColor, grassColor, grassCoverage);
                baseColor = lerp(baseColor, fixed3(0.82, 0.84, 0.81), coverage.snow);

                float3 noisePosition = localPosition / max(_CliffNoisePeriod, 1.0);
                half3 dirtNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _GrassNormalDetailScale,
                    float3(0.79, 0.17, 0.53),
                    _DirtNormalStrength);
                half3 rockNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _CliffNoiseDetailScale,
                    float3(0.37, 0.61, 0.83),
                    _CliffNormalStrength);
                half3 sandNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _SandNormalDetailScale,
                    float3(0.79, 0.17, 0.53),
                    _SandNormalStrength);
                half3 underlyingNormal = normalize(
                    dirtNormal * underlyingWeights.a.x
                        + rockNormal * underlyingWeights.a.z
                        + sandNormal * underlyingWeights.b.x);
                half3 normal = normalize(
                    dirtNormal * weights.a.x
                        + underlyingNormal * weights.a.y
                        + rockNormal * weights.a.z
                        + underlyingNormal * weights.a.w
                        + sandNormal * weights.b.x
                        + lerp(underlyingNormal, rockNormal, 0.35h) * weights.b.y);
                normal = normalize(lerp(normal, dirtNormal, grassCoverage));
                normal = normalize(lerp(normal, geometricNormal, coverage.snow));

                half coastalDistance = input.coastalWetness
                    - (0.5h + coverage.coastalNoise * _CoastalWetnessNoiseStrength);
                half coastalTransition = max(
                    _CoastalWetnessBlendWidth,
                    fwidth(coastalDistance));
                half coastalWetness = smoothstep(
                    -coastalTransition,
                    coastalTransition,
                    coastalDistance);
                half riverBankWetness = pow(
                    max(saturate(input.material.b), coverage.river),
                    max(_WetBankBlendExponent, 0.01h));
                half aboveSea = step(0.0h, localPosition.y);
                half submerged = 1.0h - aboveSea;
                half wetSurfaceEffects = max(riverBankWetness, coastalWetness)
                    * aboveSea
                    * (1.0h - max(grassCoverage, coverage.snow));
                baseColor *= 1.0h - max(wetSurfaceEffects, submerged) * _WetDarkening;

                float3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                half diffuse = saturate(dot(normal, lightDirection)) * attenuation;
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                half3 lighting = ShadeSH9(half4(normal, 1.0h))
                        * cloud.ambientTransmittance
                    + _LightColor0.rgb * diffuse * cloud.directTransmittance;
                fixed4 color = fixed4(baseColor * _Color.rgb * lighting, 1.0h);
                UNITY_APPLY_FOG(input.fogCoord, color);
                return color;
            }

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.pos = UnityObjectToClipPos(input.vertex);
                output.uv = input.uv;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                output.geometricWorldNormal = UnityObjectToWorldNormal(input.normal);
                output.material = input.material;
                output.environment = input.environment;
                output.coastalWetness = step(output.islandLocalPosition.y, 0.05);
                TRANSFER_SHADOW(output);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                half3 geometricNormal = normalize(input.geometricWorldNormal);
                UNITY_BRANCH
                if (_WorldNormalWeight > 0.5h)
                {
                    float3 encoded = tex2D(_WorldNormal, input.uv).rgb;
                    float3 localNormal = normalize(float3(
                        encoded.r * 2.0 - 1.0,
                        encoded.b * 2.0 - 1.0,
                        encoded.g * 2.0 - 1.0));
                    geometricNormal = normalize(UnityObjectToWorldNormal(localNormal));
                }

                float3 localPosition = input.islandLocalPosition;
                half elevation = localPosition.y;
                half3 localGeometricNormal = normalize(mul(
                    (float3x3)_IslandWorldToLocal,
                    geometricNormal));
                MotuTerrainCoverage coverage = MotuBuildTerrainCoverage(
                    localPosition,
                    geometricNormal,
                    input.material,
                    input.environment);

                half strongest = max(
                    max(max(coverage.forestFloor, coverage.rock), coverage.river),
                    max(coverage.beach, coverage.stones));
                half dirtCandidate = saturate(1.0h - strongest);
                half4 candidatesA = half4(
                    dirtCandidate,
                    coverage.forestFloor,
                    coverage.rock,
                    coverage.river);
                half2 candidatesB = half2(coverage.beach, coverage.stones);

                #if defined(MOTU_TERRAIN_LOD2)
                    return ShadeDistantTerrain(
                        input,
                        geometricNormal,
                        localGeometricNormal,
                        localPosition,
                        coverage,
                        candidatesA,
                        candidatesB);
                #else

                half localUp = saturate(localGeometricNormal.y);
                half dirtPatchNoise = clamp(
                    coverage.noise.macro.g * 0.20h
                        + coverage.noise.broad.b * 0.25h
                        + (coverage.noise.patch.r * 2.0h - 1.0h) * 0.55h,
                    -1.0h,
                    1.0h);
                half rockPatchNoise = clamp(
                    coverage.noise.macro.r * 0.20h
                        + coverage.noise.broad.g * 0.25h
                        + (coverage.noise.patch.g * 2.0h - 1.0h) * 0.55h,
                    -1.0h,
                    1.0h);
                half forestPatchNoise = clamp(
                    coverage.noise.macro.g * 0.20h
                        + coverage.noise.broad.r * 0.25h
                        + coverage.noise.detail.g * 0.55h,
                    -1.0h,
                    1.0h);
                half riverPatchNoise = clamp(
                    coverage.noise.macro.b * 0.20h
                        + coverage.noise.broad.b * 0.25h
                        + coverage.noise.detail.b * 0.55h,
                    -1.0h,
                    1.0h);
                half stonesPatchNoise = clamp(
                    coverage.noise.macro.b * 0.20h
                        + coverage.noise.broad.r * 0.25h
                        + (coverage.noise.patch.b * 2.0h - 1.0h) * 0.55h,
                    -1.0h,
                    1.0h);
                half beachPatchNoise = clamp(
                    coverage.noise.macro.r * 0.20h
                        + coverage.noise.broad.b * 0.25h
                        + coverage.noise.detail.r * 0.55h,
                    -1.0h,
                    1.0h);
                half dirtTextureProjection = MotuTopTextureWeight(
                    localUp,
                    dirtPatchNoise);
                half forestTextureProjection = MotuTopTextureWeight(
                    localUp,
                    forestPatchNoise);
                half rockTextureProjection = MotuTopTextureWeight(
                    localUp,
                    rockPatchNoise);
                half riverTextureProjection = MotuTopTextureWeight(
                    localUp,
                    riverPatchNoise);
                half stonesTextureProjection = MotuTopTextureWeight(
                    localUp,
                    stonesPatchNoise);
                half beachTextureProjection = MotuTopTextureWeight(
                    localUp,
                    beachPatchNoise);
                half simpleOverlayPresence = smoothstep(
                    0.01h,
                    0.20h,
                    max(coverage.river, coverage.stones));
                half underlayRecipeVisibility = 1.0h - simpleOverlayPresence;
                half dirtUnderlayProjection = dirtTextureProjection
                    * underlayRecipeVisibility;
                half forestUnderlayProjection = forestTextureProjection
                    * underlayRecipeVisibility;
                half rockUnderlayProjection = rockTextureProjection
                    * underlayRecipeVisibility;
                half beachUnderlayProjection = beachTextureProjection
                    * underlayRecipeVisibility;

                float2 dirtUv = MotuLayerUv(localPosition, MOTU_LAYER_DIRT);
                float2 forestUv = MotuLayerUv(localPosition, MOTU_LAYER_FOREST_FLOOR);
                float2 rockUv = MotuLayerUv(localPosition, MOTU_LAYER_ROCK);
                float2 riverUv = MotuLayerUv(localPosition, MOTU_LAYER_RIVER_BED);
                float2 beachUv = MotuLayerUv(localPosition, MOTU_LAYER_BEACH);
                float2 stonesUv = MotuLayerUv(localPosition, MOTU_LAYER_STONES);
                float3 localViewDirection = normalize(mul(
                    (float3x3)_IslandWorldToLocal,
                    UnityWorldSpaceViewDir(input.worldPosition)));
                half2 dirtBlendMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(dirtUv, MOTU_LAYER_DIRT)).rg;
                half2 forestBlendMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(forestUv, MOTU_LAYER_FOREST_FLOOR)).rg;
                half2 rockBlendMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(rockUv, MOTU_LAYER_ROCK)).rg;
                half2 riverBlendMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(riverUv, MOTU_LAYER_RIVER_BED)).rg;
                half2 beachBlendMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(beachUv, MOTU_LAYER_BEACH)).rg;
                half2 stonesBlendMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(stonesUv, MOTU_LAYER_STONES)).rg;
                MotuBaseWeights undisplacedWeights = MotuHeightBlendBase(
                    candidatesA,
                    candidatesB,
                    lerp(
                        half4(0.5h, 0.5h, 0.5h, riverBlendMask.r),
                        half4(
                            dirtBlendMask.r,
                            forestBlendMask.r,
                            rockBlendMask.r,
                            riverBlendMask.r),
                        half4(
                            dirtUnderlayProjection,
                            forestUnderlayProjection,
                            rockUnderlayProjection,
                            1.0h)),
                    lerp(
                        half2(0.5h, stonesBlendMask.r),
                        half2(beachBlendMask.r, stonesBlendMask.r),
                        half2(beachUnderlayProjection, 1.0h)));

                // Resolve one physical height field and one world-space ray hit
                // from every contributing recipe. Applying that common offset
                // to all UV scales keeps blended materials spatially aligned.
                half4 parallaxProjectionA = half4(
                    dirtUnderlayProjection,
                    forestUnderlayProjection,
                    rockUnderlayProjection,
                    riverTextureProjection);
                half2 parallaxProjectionB = half2(
                    beachUnderlayProjection,
                    stonesTextureProjection);
                float2 parallaxWorldOffset = float2(0.0, 0.0);
                #if !defined(MOTU_TERRAIN_LOD1)
                    parallaxWorldOffset = BlendedParallaxWorldOffset(
                        localPosition.xz,
                        localViewDirection,
                        undisplacedWeights,
                        parallaxProjectionA,
                        parallaxProjectionB);
                #endif
                dirtUv += parallaxWorldOffset
                    / max(MotuLayerWorldSize(MOTU_LAYER_DIRT), 0.01);
                forestUv += parallaxWorldOffset
                    / max(MotuLayerWorldSize(MOTU_LAYER_FOREST_FLOOR), 0.01);
                rockUv += parallaxWorldOffset
                    / max(MotuLayerWorldSize(MOTU_LAYER_ROCK), 0.01);
                riverUv += parallaxWorldOffset
                    / max(MotuLayerWorldSize(MOTU_LAYER_RIVER_BED), 0.01);
                beachUv += parallaxWorldOffset
                    / max(MotuLayerWorldSize(MOTU_LAYER_BEACH), 0.01);
                stonesUv += parallaxWorldOffset
                    / max(MotuLayerWorldSize(MOTU_LAYER_STONES), 0.01);

                half2 dirtMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(dirtUv, MOTU_LAYER_DIRT)).rg;
                half2 forestMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(forestUv, MOTU_LAYER_FOREST_FLOOR)).rg;
                half2 rockMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(rockUv, MOTU_LAYER_ROCK)).rg;
                half2 riverMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(riverUv, MOTU_LAYER_RIVER_BED)).rg;
                half2 beachMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(beachUv, MOTU_LAYER_BEACH)).rg;
                half2 stonesMask = UNITY_SAMPLE_TEX2DARRAY(_TerrainMaskArray, float3(stonesUv, MOTU_LAYER_STONES)).rg;
                MotuBaseWeights weights = undisplacedWeights;

                // Forest floor, riverbed, and fallen stones are overlays. Their
                // recipe fades into the simple procedural dirt/rock/beach type
                // that would have won if the overlay were absent. Do not carry
                // the underlying material's authored recipe into an overlay.
                half underlyingDirtCandidate = saturate(
                    1.0h - max(coverage.rock, coverage.beach));
                MotuBaseWeights underlyingWeights = MotuHeightBlendBase(
                    half4(
                        underlyingDirtCandidate,
                        0.0h,
                        coverage.rock,
                        0.0h),
                    half2(coverage.beach, 0.0h),
                    half4(
                        0.5h,
                        0.5h,
                        0.5h,
                        0.5h),
                    half2(0.5h, 0.5h));

                fixed3 dirtTexture = UNITY_SAMPLE_TEX2DARRAY(
                    _TerrainAlbedoArray,
                    float3(dirtUv, MOTU_LAYER_DIRT)).rgb;
                fixed3 dirt = lerp(
                    _GroundDirtColor.rgb,
                    dirtTexture,
                    dirtUnderlayProjection);
                fixed3 rockTexture = UNITY_SAMPLE_TEX2DARRAY(
                    _TerrainAlbedoArray,
                    float3(rockUv, MOTU_LAYER_ROCK)).rgb;
                fixed3 rock = lerp(
                    _RockColor.rgb,
                    rockTexture,
                    rockUnderlayProjection);
                half simpleSandVariation = clamp(
                    coverage.noise.detail.r * 0.65h
                        + coverage.noise.macro.b * 0.35h,
                    -1.0h,
                    1.0h);
                fixed3 simpleBeachColor = _SandColor.rgb
                    * (1.0h + simpleSandVariation * 0.08h);
                fixed3 beach = lerp(
                    simpleBeachColor,
                    UNITY_SAMPLE_TEX2DARRAY(
                        _TerrainAlbedoArray,
                        float3(beachUv, MOTU_LAYER_BEACH)).rgb,
                    beachUnderlayProjection);
                fixed3 underlyingColor = _GroundDirtColor.rgb
                        * underlyingWeights.a.x
                    + _RockColor.rgb * underlyingWeights.a.z
                    + simpleBeachColor * underlyingWeights.b.x;
                fixed3 forest = lerp(
                    underlyingColor,
                    UNITY_SAMPLE_TEX2DARRAY(
                        _TerrainAlbedoArray,
                        float3(forestUv, MOTU_LAYER_FOREST_FLOOR)).rgb
                        * _ForestFloorColor.rgb,
                    forestUnderlayProjection);
                fixed3 river = lerp(
                    underlyingColor,
                    UNITY_SAMPLE_TEX2DARRAY(
                        _TerrainAlbedoArray,
                        float3(riverUv, MOTU_LAYER_RIVER_BED)).rgb,
                    riverTextureProjection);
                fixed3 stones = lerp(
                    underlyingColor,
                    UNITY_SAMPLE_TEX2DARRAY(
                        _TerrainAlbedoArray,
                        float3(stonesUv, MOTU_LAYER_STONES)).rgb * _StonesColor.rgb,
                    stonesTextureProjection);
                fixed3 baseColor = dirt * weights.a.x
                    + forest * weights.a.y
                    + rock * weights.a.z
                    + river * weights.a.w
                    + beach * weights.b.x
                    + stones * weights.b.y;

                float2 grassColorUv = localPosition.xz
                    / max(_GrassColorNoiseWorldSize, 1.0);
                half grassColorNoise = tex2D(_GrassPatchNoise, grassColorUv).b;
                fixed3 grassColor = lerp(
                    _GrassColorA.rgb,
                    _GrassColorB.rgb,
                    smoothstep(0.1h, 0.9h, grassColorNoise));
                half grassCoverage = smoothstep(
                    0.35h,
                    0.65h,
                    coverage.grass)
                    * weights.a.x
                    * (1.0h - coverage.snow);
                baseColor = lerp(baseColor, grassColor, grassCoverage);
                float dirtDistance = distance(
                    input.worldPosition.xz,
                    _GrassPlayerPosition.xz);
                half dirtProximity = 1.0h - smoothstep(
                    max(_GroundDirtCoreRadius, 0.0),
                    max(_GroundDirtCoreRadius, 0.0)
                        + max(_GroundDirtFadeWidth, 0.001),
                    dirtDistance);
                baseColor = lerp(
                    baseColor,
                    _GroundDirtColor.rgb,
                    _GrassEnabled * grassCoverage * dirtProximity);
                fixed3 snowColor = fixed3(0.82, 0.84, 0.81);
                baseColor = lerp(baseColor, snowColor, coverage.snow);

                float3 noisePosition = localPosition / max(_CliffNoisePeriod, 1.0);
                half3 simpleDirtNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _GrassNormalDetailScale,
                    float3(0.79, 0.17, 0.53),
                    _DirtNormalStrength);
                half3 dirtNormal = normalize(lerp(
                    simpleDirtNormal,
                    ArrayWorldNormal(dirtUv, MOTU_LAYER_DIRT),
                    saturate(dirtUnderlayProjection
                        * MotuLayerNormalStrength(MOTU_LAYER_DIRT))));
                half3 simpleRockNormal = geometricNormal;
                if (_CliffNormalStrength > 0.0h)
                {
                    half3 rockDetail = tex3D(
                        _CliffNoise3D,
                        noisePosition * _CliffNoiseDetailScale
                            + float3(0.37, 0.61, 0.83)).rgb * 2.0h - 1.0h;
                    half3 rockPerturbation = coverage.noise.broad * 0.45h
                        + rockDetail * 0.55h;
                    rockPerturbation -= simpleRockNormal
                        * dot(rockPerturbation, simpleRockNormal);
                    simpleRockNormal = normalize(
                        simpleRockNormal
                            + rockPerturbation * _CliffNormalStrength);
                }
                half3 rockNormal = normalize(lerp(
                    simpleRockNormal,
                    ArrayWorldNormal(rockUv, MOTU_LAYER_ROCK),
                    saturate(rockUnderlayProjection
                        * MotuLayerNormalStrength(MOTU_LAYER_ROCK))));
                half3 simpleBeachNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _SandNormalDetailScale,
                    float3(0.79, 0.17, 0.53),
                    _SandNormalStrength);
                half3 beachNormal = normalize(lerp(
                    simpleBeachNormal,
                    ArrayWorldNormal(beachUv, MOTU_LAYER_BEACH),
                    saturate(beachUnderlayProjection
                        * MotuLayerNormalStrength(MOTU_LAYER_BEACH))));
                half3 underlyingNormal = normalize(
                    simpleDirtNormal * underlyingWeights.a.x
                        + simpleRockNormal * underlyingWeights.a.z
                        + simpleBeachNormal * underlyingWeights.b.x);
                half3 forestNormal = normalize(lerp(
                    underlyingNormal,
                    ArrayWorldNormal(forestUv, MOTU_LAYER_FOREST_FLOOR),
                    saturate(forestUnderlayProjection
                        * MotuLayerNormalStrength(MOTU_LAYER_FOREST_FLOOR))));
                half3 riverNormal = normalize(lerp(
                    underlyingNormal,
                    ArrayWorldNormal(riverUv, MOTU_LAYER_RIVER_BED),
                    saturate(riverTextureProjection
                        * MotuLayerNormalStrength(MOTU_LAYER_RIVER_BED))));
                half3 stonesNormal = normalize(lerp(
                    underlyingNormal,
                    ArrayWorldNormal(stonesUv, MOTU_LAYER_STONES),
                    saturate(stonesTextureProjection * MotuLayerNormalStrength(MOTU_LAYER_STONES))));
                half3 normal = normalize(
                    dirtNormal * weights.a.x
                        + forestNormal * weights.a.y
                        + rockNormal * weights.a.z
                        + riverNormal * weights.a.w
                        + beachNormal * weights.b.x
                        + stonesNormal * weights.b.y);
                half3 grassNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _GrassNormalDetailScale,
                    float3(0.79, 0.17, 0.53),
                    _GrassNormalStrength);
                normal = normalize(lerp(normal, grassNormal, grassCoverage));
                half3 snowNormal = PerturbNormal(
                    geometricNormal,
                    noisePosition,
                    _SnowNormalDetailScale,
                    float3(0.19, 0.89, 0.43),
                    _SnowNormalStrength);
                normal = normalize(lerp(normal, snowNormal, coverage.snow));

                float3 groundWind = MotuGrassWindSample(input.worldPosition.xz);
                float3 horizontalWind = float3(groundWind.x, 0.0, groundWind.z);
                float3 tangentWind = horizontalWind - normal * dot(horizontalWind, normal);
                tangentWind *= rsqrt(max(dot(tangentWind, tangentWind), 1.0e-4));
                normal = normalize(
                    normal
                        + tangentWind
                            * (groundWind.y
                                * _GrassWindNormalStrength
                                * grassCoverage));

                half sharedOcclusion = lerp(
                    1.0h,
                    tex2D(_Occlusion, input.uv).r,
                    _OcclusionStrength);
                half dirtOcclusion = lerp(
                    1.0h,
                    dirtMask.g,
                    dirtUnderlayProjection
                        * MotuLayerOcclusionStrength(MOTU_LAYER_DIRT));
                half rockOcclusion = lerp(
                    1.0h,
                    rockMask.g,
                    rockUnderlayProjection
                        * MotuLayerOcclusionStrength(MOTU_LAYER_ROCK));
                half beachOcclusion = lerp(
                    1.0h,
                    beachMask.g,
                    beachUnderlayProjection
                        * MotuLayerOcclusionStrength(MOTU_LAYER_BEACH));
                // The simple procedural types have no recipe-local occlusion;
                // the shared terrain occlusion is still applied afterwards.
                half underlyingOcclusion = 1.0h;
                half forestOcclusion = lerp(
                    underlyingOcclusion,
                    forestMask.g,
                    forestUnderlayProjection
                        * MotuLayerOcclusionStrength(MOTU_LAYER_FOREST_FLOOR));
                half riverOcclusion = lerp(
                    underlyingOcclusion,
                    riverMask.g,
                    riverTextureProjection
                        * MotuLayerOcclusionStrength(MOTU_LAYER_RIVER_BED));
                half stonesOcclusion = lerp(
                    underlyingOcclusion,
                    stonesMask.g,
                    stonesTextureProjection
                        * MotuLayerOcclusionStrength(MOTU_LAYER_STONES));
                half materialOcclusion = weights.a.x * dirtOcclusion
                    + weights.a.y * forestOcclusion
                    + weights.a.z * rockOcclusion
                    + weights.a.w * riverOcclusion
                    + weights.b.x * beachOcclusion
                    + weights.b.y * stonesOcclusion;
                half occlusion = sharedOcclusion * materialOcclusion;

                half coastalDistance = input.coastalWetness
                    - (0.5h + coverage.coastalNoise * _CoastalWetnessNoiseStrength);
                half coastalTransition = max(
                    _CoastalWetnessBlendWidth,
                    fwidth(coastalDistance));
                half coastalWetness = smoothstep(
                    -coastalTransition,
                    coastalTransition,
                    coastalDistance);
                half riverBankWetness = pow(
                    max(saturate(input.material.b), coverage.river),
                    max(_WetBankBlendExponent, 0.01h));
                half wettableCoverage = 1.0h - max(grassCoverage, coverage.snow);
                half aboveSea = step(0.0h, elevation);
                half submerged = 1.0h - aboveSea;
                half wetSurfaceEffects = max(riverBankWetness, coastalWetness)
                    * aboveSea
                    * wettableCoverage;
                half wetDarkening = max(wetSurfaceEffects, submerged);
                baseColor *= 1.0h - wetDarkening * _WetDarkening;

                if (_TerrainDebugView > 0.5h && _TerrainDebugView < 1.5h)
                    return fixed4(weights.a.xyz, 1.0h);
                if (_TerrainDebugView >= 1.5h && _TerrainDebugView < 2.5h)
                    return fixed4(weights.a.w, weights.b.x, weights.b.y, 1.0h);
                if (_TerrainDebugView >= 2.5h && _TerrainDebugView < 3.5h)
                    return fixed4(
                        coverage.coastalNoise * 0.5h + 0.5h,
                        wetSurfaceEffects,
                        wetDarkening,
                        1.0h);
                if (_TerrainDebugView >= 3.5h)
                    return fixed4(normal * 0.5h + 0.5h, 1.0h);

                float3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                half diffuse = saturate(dot(normal, lightDirection)) * attenuation;
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                half3 direct = _LightColor0.rgb
                    * diffuse
                    * cloud.directTransmittance;
                half3 ambient = ShadeSH9(half4(normal, 1.0h))
                    * cloud.ambientTransmittance;
                float3 viewDirection = normalize(UnityWorldSpaceViewDir(input.worldPosition));
                float3 halfDirection = normalize(lightDirection + viewDirection);
                half wetHighlightPower = lerp(10.0h, 110.0h, _WetSmoothness);
                half wetHighlight = pow(
                    saturate(dot(normal, halfDirection)),
                    wetHighlightPower)
                    * _WetSpecularStrength
                    * wetSurfaceEffects
                    * attenuation
                    * cloud.directTransmittance
                    * step(0.0h, dot(normal, lightDirection));
                half wetFresnel = pow(
                    1.0h - saturate(dot(normal, viewDirection)),
                    4.0h)
                    * (_WetSpecularStrength * 0.12h)
                    * wetSurfaceEffects;
                half3 wetReflection = _LightColor0.rgb * wetHighlight
                    + ambient * wetFresnel * occlusion;
                fixed4 color = fixed4(
                    baseColor * _Color.rgb * occlusion * (ambient + direct)
                        + wetReflection,
                    1.0h);
                UNITY_APPLY_FOG(input.fogCoord, color);
                return color;
                #endif
            }
            ENDCG
        }

        UsePass "Legacy Shaders/VertexLit/SHADOWCASTER"
    }

    FallBack "Diffuse"
}
