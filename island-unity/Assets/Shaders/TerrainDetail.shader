Shader "Motu/Terrain Unified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        _RockColor ("Exposed Rock", Color) = (0.30, 0.32, 0.29, 1)
        [NoScaleOffset] _RockAlbedoMap ("Rock Top Surface Colour", 2D) = "white" {}
        [NoScaleOffset] _RockNormalMap ("Rock Top Surface Normal", 2D) = "bump" {}
        [NoScaleOffset] _RockMaskMap ("Rock Top Surface Mask (R Height, G Occlusion)", 2D) = "gray" {}
        _RockTextureWorldSize ("Rock Top Surface Texture Size (metres)", Float) = 4
        _RockNormalMapStrength ("Rock Top Surface Normal Strength", Range(0, 2)) = 0
        _RockParallaxDepth ("Rock Top Surface Parallax Depth (metres)", Range(0, 0.15)) = 0.05
        _RockHeightBlendStrength ("Rock Slope Blend Height Influence", Range(0, 1)) = 1
        _RockTextureOcclusionStrength ("Rock Top Surface Texture Occlusion", Range(0, 1)) = 0
        _RiverBedColor ("Riverbed Tint", Color) = (0.30, 0.32, 0.29, 1)
        [NoScaleOffset] _RiverBedAlbedoMap ("Riverbed Colour", 2D) = "white" {}
        [NoScaleOffset] _RiverBedNormalMap ("Riverbed Normal", 2D) = "bump" {}
        [NoScaleOffset] _RiverBedMaskMap ("Riverbed Mask (R Height, G Occlusion)", 2D) = "gray" {}
        _RiverBedTextureWorldSize ("Riverbed Texture Size (metres)", Float) = 2
        _RiverBedNormalMapStrength ("Riverbed Normal Strength", Range(0, 2)) = 0
        _RiverBedParallaxDepth ("Riverbed Parallax Depth (metres)", Range(0, 0.1)) = 0.025
        _RiverBedHeightBlendStrength ("Riverbed Height Blend Influence", Range(0, 1)) = 1
        _RiverBedTextureOcclusionStrength ("Riverbed Texture Occlusion", Range(0, 1)) = 0
        _ForestFloorColor ("Forest Floor Tint", Color) = (1, 1, 1, 1)
        [NoScaleOffset] _ForestFloorAlbedoMap ("Forest Floor Colour", 2D) = "white" {}
        [NoScaleOffset] _ForestFloorNormalMap ("Forest Floor Normal", 2D) = "bump" {}
        [NoScaleOffset] _ForestFloorMaskMap ("Forest Floor Mask (R Height, G Occlusion)", 2D) = "gray" {}
        _ForestFloorTextureWorldSize ("Forest Floor Texture Size (metres)", Float) = 2
        _ForestFloorNormalMapStrength ("Forest Floor Normal Strength", Range(0, 2)) = 1
        _ForestFloorParallaxDepth ("Forest Floor Parallax Depth (metres)", Range(0, 0.08)) = 0.018
        _ForestFloorHeightBlendStrength ("Forest Floor Height Blend Influence", Range(0, 1)) = 1
        _ForestFloorTextureOcclusionStrength ("Forest Floor Texture Occlusion", Range(0, 1)) = 0.7
        _ForestFloorEdgeNoiseStrength ("Forest Floor Edge Noise Strength", Range(0, 0.45)) = 0.22
        _ForestFloorEdgeBlendWidth ("Forest Floor Edge Blend Width", Range(0.01, 0.5)) = 0.035
        _StonesColor ("Fallen Stones Tint", Color) = (1, 1, 1, 1)
        [NoScaleOffset] _StonesAlbedoMap ("Fallen Stones Colour", 2D) = "white" {}
        [NoScaleOffset] _StonesNormalMap ("Fallen Stones Normal", 2D) = "bump" {}
        [NoScaleOffset] _StonesMaskMap ("Fallen Stones Mask (R Height, G Occlusion)", 2D) = "gray" {}
        [HideInInspector][NoScaleOffset] _RockRiverMaskMap ("Runtime Rock + River Mask", 2D) = "gray" {}
        [HideInInspector][NoScaleOffset] _ForestStonesMaskMap ("Runtime Forest + Stones Mask", 2D) = "gray" {}
        _StonesTextureWorldSize ("Fallen Stones Texture Size (metres)", Float) = 2
        _StonesNormalMapStrength ("Fallen Stones Normal Strength", Range(0, 2)) = 1
        _StonesParallaxDepth ("Fallen Stones Parallax Depth (metres)", Range(0, 0.08)) = 0.025
        _StonesHeightBlendStrength ("Fallen Stones Height Blend Influence", Range(0, 1)) = 1
        _StonesTextureOcclusionStrength ("Fallen Stones Texture Occlusion", Range(0, 1)) = 0.75
        _StonesEdgeNoiseStrength ("Fallen Stones Edge Noise Strength", Range(0, 0.45)) = 0.22
        _StonesEdgeBlendWidth ("Fallen Stones Edge Blend Width", Range(0.01, 0.5)) = 0.035
        _WetBankBlendExponent ("Wet Bank Blend Exponent", Range(0.2, 1)) = 0.45
        _WetDarkening ("Wet Surface Darkening", Range(0, 0.75)) = 0.48
        _WetSmoothness ("Wet Surface Smoothness", Range(0, 1)) = 0.65
        _WetSpecularStrength ("Wet Surface Highlight Strength", Range(0, 1)) = 0.55
        _TopTextureFadeOutSlope ("Top Texture Fade-Out Slope (degrees)", Range(1, 89)) = 45
        [NoScaleOffset] _WorldNormal ("Shared World Normal", 2D) = "bump" {}
        [PerRendererData] _WorldNormalWeight ("World Normal Weight", Float) = 1
        [NoScaleOffset] _Occlusion ("Shared Occlusion", 2D) = "white" {}
        _OcclusionStrength ("Occlusion Strength", Range(0, 1)) = 0.25
        _SnowLine ("Snow Line (metres)", Float) = 100
        _SnowEdgeNoiseMetres ("Snow Edge Noise (metres)", Range(0, 10)) = 2.5
        _SnowMacroNoiseMetres ("Snow Macro Noise (metres)", Range(0, 40)) = 18
        _SandPatchNoiseWorldSize ("Sand Patch Repeat (metres)", Float) = 32
        _RiverEdgeNoiseStrength ("River Edge Noise Strength", Range(0, 0.45)) = 0.20
        _RiverEdgeBlendWidth ("River Edge Blend Width", Range(0.01, 0.5)) = 0.20
        _CliffNormalCutoff ("Cliff Up-Normal Cutoff", Range(0, 1)) = 0.55
        _CliffBoundaryNoiseStrength ("Cliff Boundary Noise Strength", Range(0, 0.5)) = 0.30
        _RockBoundaryNoiseStrength ("Sand Rock Edge Noise Strength", Range(0, 0.4)) = 0.18
        _SandRockSlopeThreshold ("Sand Rock Slope Threshold", Range(0, 0.5)) = 0.10
        [NoScaleOffset] _CliffNoise3D ("Cliff 3D Noise", 3D) = "gray" {}
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
        _GrassThinDepositColor ("Poor Soil", Color) = (0.09, 0.055, 0.026, 1)
        _GrassColorA ("Grass Colour A", Color) = (0.18, 0.46, 0.14, 1)
        _GrassColorB ("Grass Colour B", Color) = (0.34, 0.50, 0.14, 1)
        _GrassColorNoiseWorldSize ("Grass Colour Noise Repeat (metres)", Float) = 2048
        [NoScaleOffset] _GrassPatchNoise ("Grass Patch Noise", 2D) = "white" {}
        _GrassPatchNoiseWorldSize ("Grass Patch Repeat (metres)", Float) = 32
        [HideInInspector] _GrassWindDirection ("Grass Wind Direction", Vector) = (1, 0, 0.35, 0)
        [HideInInspector] _GrassWindStrength ("Grass Wind Bend (metres)", Range(0, 0.25)) = 0.07
        [HideInInspector] _GrassWindSpeed ("Grass Wind Speed (metres/second)", Range(0, 10)) = 1.8
        [HideInInspector] _GrassWindWorldSize ("Grass Wind Gust Size (metres)", Range(1, 64)) = 12
        [HideInInspector] _GrassWindNormalStrength ("Grass Wind Normal Strength", Range(0, 1)) = 0.35
        [HideInInspector] _GrassEnabled ("Local Grass Enabled", Float) = 0
        [HideInInspector] _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
        _GroundDirtColor ("Dirt", Color) = (0.09, 0.055, 0.026, 1)
        _GroundDirtCoreRadius ("Dirt Core Radius (metres)", Float) = 0.5
        _GroundDirtFadeWidth ("Dirt Fade Width (metres)", Float) = 2
    }

    SubShader
    {
        Tags { "RenderType"="Opaque" "Queue"="Geometry" }
        LOD 300

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog

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
                // x = forest floor, y = settled stones.
                half2 environment : TEXCOORD7;
            };

            sampler2D _WorldNormal;
            sampler2D _Occlusion;
            sampler2D _GrassPatchNoise;
            sampler2D _RockAlbedoMap;
            sampler2D _RockNormalMap;
            sampler2D _RockRiverMaskMap;
            sampler2D _RiverBedAlbedoMap;
            sampler2D _RiverBedNormalMap;
            sampler2D _ForestFloorAlbedoMap;
            sampler2D _ForestFloorNormalMap;
            sampler2D _StonesAlbedoMap;
            sampler2D _StonesNormalMap;
            sampler2D _ForestStonesMaskMap;
            sampler3D _CliffNoise3D;
            fixed4 _Color;
            fixed4 _RockColor;
            fixed4 _RiverBedColor;
            fixed4 _ForestFloorColor;
            float _RockTextureWorldSize;
            half _RockNormalMapStrength;
            float _RockParallaxDepth;
            half _RockHeightBlendStrength;
            half _RockTextureOcclusionStrength;
            float _RiverBedTextureWorldSize;
            half _RiverBedNormalMapStrength;
            float _RiverBedParallaxDepth;
            half _RiverBedHeightBlendStrength;
            half _RiverBedTextureOcclusionStrength;
            float _ForestFloorTextureWorldSize;
            half _ForestFloorNormalMapStrength;
            float _ForestFloorParallaxDepth;
            half _ForestFloorHeightBlendStrength;
            half _ForestFloorTextureOcclusionStrength;
            half _ForestFloorEdgeNoiseStrength;
            half _ForestFloorEdgeBlendWidth;
            fixed4 _StonesColor;
            float _StonesTextureWorldSize;
            half _StonesNormalMapStrength;
            float _StonesParallaxDepth;
            half _StonesHeightBlendStrength;
            half _StonesTextureOcclusionStrength;
            half _StonesEdgeNoiseStrength;
            half _StonesEdgeBlendWidth;
            half _WetBankBlendExponent;
            half _WetDarkening;
            half _WetSmoothness;
            half _WetSpecularStrength;
            half _TopTextureFadeOutSlope;
            half _WorldNormalWeight;
            half _OcclusionStrength;
            float _SnowLine;
            float _SnowEdgeNoiseMetres;
            float _SnowMacroNoiseMetres;
            float _SandPatchNoiseWorldSize;
            half _RiverEdgeNoiseStrength;
            half _RiverEdgeBlendWidth;
            half _CliffNormalCutoff;
            half _CliffBoundaryNoiseStrength;
            half _RockBoundaryNoiseStrength;
            half _SandRockSlopeThreshold;
            float _CliffNoisePeriod;
            half _RockPatchNoiseDetailScale;
            half _CliffNoiseDetailScale;
            half _CliffNormalStrength;
            half _GrassNormalDetailScale;
            half _SandNormalDetailScale;
            half _SnowNormalDetailScale;
            half _DirtNormalStrength;
            half _GrassNormalStrength;
            half _SandNormalStrength;
            half _SnowNormalStrength;
            fixed4 _GrassThinDepositColor;
            fixed4 _GrassColorA;
            fixed4 _GrassColorB;
            float _GrassColorNoiseWorldSize;
            float _GrassPatchNoiseWorldSize;
            float4 _GrassWindDirection;
            float _GrassWindStrength;
            float _GrassWindSpeed;
            float _GrassWindWorldSize;
            half _GrassWindNormalStrength;
            half _GrassEnabled;
            float3 _GrassPlayerPosition;
            fixed4 _GroundDirtColor;
            float _GroundDirtCoreRadius;
            float _GroundDirtFadeWidth;
            float4x4 _IslandWorldToLocal;

            #include "GrassWindCommon.cginc"

            half AntialiasedMask(float signedDistance)
            {
                float transitionWidth = max(fwidth(signedDistance), 1.0e-4);
                return smoothstep(-transitionWidth, transitionWidth, signedDistance);
            }

            half HeightModulatedTextureWeight(
                half slopeWeight,
                half sampledHeight,
                half influence)
            {
                // Height only shapes the middle of the slope transition. The
                // zero and one endpoints remain authoritative, keeping level
                // ground fully textured and the fade-out slope fully old-style.
                half transitionWindow = 4.0h
                    * slopeWeight
                    * (1.0h - slopeWeight);
                half centredHeight = sampledHeight * 2.0h - 1.0h;
                return saturate(
                    slopeWeight
                        + centredHeight * influence * transitionWindow * 0.5h);
            }

            float2 ParallaxUv(
                float2 uv,
                sampler2D maskMap,
                half4 heightChannel,
                float textureWorldSize,
                float depthMetres,
                float3 localViewDirection,
                half projectionWeight)
            {
                projectionWeight = saturate(projectionWeight);
                UNITY_BRANCH
                if (depthMetres <= 0.0 || projectionWeight <= 0.001h)
                {
                    return uv;
                }
                float grazingSafeUp = max(abs(localViewDirection.y), 0.2);
                float2 viewOffset = localViewDirection.xz / grazingSafeUp;
                float depthInTiles = depthMetres
                    / max(textureWorldSize, 0.01);
                float2 rayOffset = viewOffset
                    * (depthInTiles * projectionWeight);
                const half layerStep = 0.125h;
                float2 uvStep = rayOffset * layerStep;
                float2 currentUv = uv;
                half currentLayer = 0.0h;
                half surfaceDepth = 1.0h
                    - dot(tex2D(maskMap, currentUv), heightChannel);
                [unroll]
                for (int stepIndex = 0; stepIndex < 8; ++stepIndex)
                {
                    if (currentLayer >= surfaceDepth)
                    {
                        break;
                    }
                    currentUv -= uvStep;
                    currentLayer += layerStep;
                    surfaceDepth = 1.0h
                        - dot(tex2D(maskMap, currentUv), heightChannel);
                }

                float2 previousUv = currentUv + uvStep;
                half previousLayer = max(currentLayer - layerStep, 0.0h);
                half previousSurfaceDepth = 1.0h
                    - dot(tex2D(maskMap, previousUv), heightChannel);
                half afterDepth = surfaceDepth - currentLayer;
                half beforeDepth = previousSurfaceDepth - previousLayer;
                half interpolationDenominator = afterDepth - beforeDepth;
                half interpolation = abs(interpolationDenominator) > 1.0e-4h
                    ? saturate(afterDepth / interpolationDenominator)
                    : 0.0h;
                return lerp(currentUv, previousUv, interpolation);
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
                TRANSFER_SHADOW(output);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float3 normal = normalize(input.geometricWorldNormal);
                UNITY_BRANCH
                if (_WorldNormalWeight > 0.5)
                {
                    // Rust stores normals as XYZ with Z up. Unity uses XZY with Y up.
                    float3 encoded = tex2D(_WorldNormal, input.uv).rgb;
                    float3 islandLocalNormal = normalize(float3(
                        encoded.r * 2.0 - 1.0,
                        encoded.b * 2.0 - 1.0,
                        encoded.g * 2.0 - 1.0));
                    normal = normalize(UnityObjectToWorldNormal(islandLocalNormal));
                }

                half sampledOcclusion = tex2D(_Occlusion, input.uv).r;
                half occlusion = lerp(1.0, sampledOcclusion, _OcclusionStrength);
                float3 islandLocalNormal = normalize(mul(
                    (float3x3)_IslandWorldToLocal,
                    normal));
                // Asset textures use one top-down projection only. Fade the
                // entire authored texture set out before that projection can
                // stretch across a steep face; vertical terrain then retains
                // the original solid colour and procedural noisy normal.
                half topTextureFadeOutUpNormal = cos(radians(
                    _TopTextureFadeOutSlope));
                float2 rockUv = input.islandLocalPosition.xz
                    / max(_RockTextureWorldSize, 0.01);
                float2 riverBedUv = input.islandLocalPosition.xz
                    / max(_RiverBedTextureWorldSize, 0.01);
                float2 forestFloorUv = input.islandLocalPosition.xz
                    / max(_ForestFloorTextureWorldSize, 0.01);
                float2 stonesUv = input.islandLocalPosition.xz
                    / max(_StonesTextureWorldSize, 0.01);
                // Mesh import scales normalized Rust coordinates into Unity
                // metres, so world-space Y is already the physical elevation.
                float elevation = input.islandLocalPosition.y;
                float slope = 1.0 - saturate(normal.y);
                float noisePeriod = max(_CliffNoisePeriod, 1.0);
                float3 noisePosition = input.islandLocalPosition / noisePeriod;
                half3 broadNoise = tex3D(_CliffNoise3D, noisePosition).rgb * 2.0 - 1.0;
                half3 macroNoise = tex3D(
                    _CliffNoise3D,
                    noisePosition * (1.0 / 3.0)
                        + float3(0.23, 0.71, 0.41)).rgb * 2.0 - 1.0;
                half3 rockPatchLayers = tex3D(
                    _CliffNoise3D,
                    noisePosition * _RockPatchNoiseDetailScale
                        + float3(0.67, 0.31, 0.91)).rgb;
                half3 bankDetailNoise = tex3D(
                    _CliffNoise3D,
                    noisePosition * (_RockPatchNoiseDetailScale * 4.0)
                        + float3(0.11, 0.83, 0.47)).rgb * 2.0 - 1.0;
                // Blend the repeated top-down texture into the simpler stone
                // over a wider slope band. Broad and mid-scale coherent patches
                // remain active on flat tops to break up visible repetition.
                half localUpNormal = saturate(islandLocalNormal.y);
                half simpleStoneBlendNoise = clamp(
                    macroNoise.r * 0.20
                        + broadNoise.g * 0.25
                        + (rockPatchLayers.g * 2.0h - 1.0h) * 0.55,
                    -1.0,
                    1.0);
                half noisyTextureUpNormal = saturate(
                    localUpNormal
                        + simpleStoneBlendNoise
                            * 0.16h
                            * (1.0h - localUpNormal));
                half topTextureBlendStart = saturate(
                    topTextureFadeOutUpNormal - 0.20h);
                half topProjectionWeight = smoothstep(
                    topTextureBlendStart,
                    1.0h,
                    noisyTextureUpNormal);
                half flatTexturePatchWeight = lerp(
                    0.0h,
                    1.0h,
                    smoothstep(-0.65h, 0.65h, simpleStoneBlendNoise));
                half rockTextureSlopeWeight = topProjectionWeight
                    * flatTexturePatchWeight;
                half riverBedTextureSlopeWeight = topProjectionWeight;
                // Fallen stones use the same coherent textured/basic-surface
                // breakup as exposed rock, with independent noise channels so
                // the two material classes do not reveal one shared stencil.
                half stonesTextureBlendNoise = clamp(
                    macroNoise.b * 0.20h
                        + broadNoise.r * 0.25h
                        + (rockPatchLayers.b * 2.0h - 1.0h) * 0.55h,
                    -1.0h,
                    1.0h);
                half stonesTexturePatchWeight = smoothstep(
                    -0.65h,
                    0.65h,
                    stonesTextureBlendNoise);
                // Turn the interpolated tree-support switch into the same kind
                // of fine coherent boundary used by the other terrain classes.
                // Keeping this formula byte-for-byte aligned with the grass
                // shader prevents either rendering path bleeding across it.
                half forestFloorBoundaryNoise = clamp(
                    bankDetailNoise.g * 0.70h
                        + bankDetailNoise.b * 0.30h,
                    -1.0h,
                    1.0h);
                half forestFloorDistance = saturate(input.environment.x)
                    - (0.5h
                        + forestFloorBoundaryNoise
                            * _ForestFloorEdgeNoiseStrength);
                half forestFloorTransition = max(
                    _ForestFloorEdgeBlendWidth,
                    fwidth(forestFloorDistance));
                half forestFloorSource = smoothstep(
                    -forestFloorTransition,
                    forestFloorTransition,
                    forestFloorDistance);
                // Use a different coherent combination from the forest floor,
                // while retaining the same boundary construction in the grass
                // shader so the fur ends precisely at the visible stone patch.
                half stonesBoundaryNoise = clamp(
                    bankDetailNoise.r * 0.45h
                        + bankDetailNoise.b * 0.55h,
                    -1.0h,
                    1.0h);
                half stonesDistance = saturate(input.environment.y)
                    - (0.5h
                        + stonesBoundaryNoise * _StonesEdgeNoiseStrength);
                half stonesTransition = max(
                    _StonesEdgeBlendWidth,
                    fwidth(stonesDistance));
                half stonesSource = smoothstep(
                    -stonesTransition,
                    stonesTransition,
                    stonesDistance);
                half forestFloorTextureSlopeWeight = topProjectionWeight;
                half stonesTextureSlopeWeight = topProjectionWeight
                    * stonesTexturePatchWeight;
                float3 localViewDirection = normalize(mul(
                    (float3x3)_IslandWorldToLocal,
                    UnityWorldSpaceViewDir(input.worldPosition)));
                rockUv = ParallaxUv(
                    rockUv,
                    _RockRiverMaskMap,
                    half4(1.0h, 0.0h, 0.0h, 0.0h),
                    _RockTextureWorldSize,
                    _RockParallaxDepth,
                    localViewDirection,
                    rockTextureSlopeWeight);
                riverBedUv = ParallaxUv(
                    riverBedUv,
                    _RockRiverMaskMap,
                    half4(0.0h, 0.0h, 1.0h, 0.0h),
                    _RiverBedTextureWorldSize,
                    _RiverBedParallaxDepth,
                    localViewDirection,
                    riverBedTextureSlopeWeight * saturate(input.material.b));
                forestFloorUv = ParallaxUv(
                    forestFloorUv,
                    _ForestStonesMaskMap,
                    half4(1.0h, 0.0h, 0.0h, 0.0h),
                    _ForestFloorTextureWorldSize,
                    _ForestFloorParallaxDepth,
                    localViewDirection,
                    forestFloorTextureSlopeWeight * forestFloorSource);
                stonesUv = ParallaxUv(
                    stonesUv,
                    _ForestStonesMaskMap,
                    half4(0.0h, 0.0h, 1.0h, 0.0h),
                    _StonesTextureWorldSize,
                    _StonesParallaxDepth,
                    localViewDirection,
                    stonesTextureSlopeWeight * stonesSource);
                fixed4 rockPackedMaskSample = tex2D(
                    _RockRiverMaskMap,
                    rockUv);
                fixed4 riverPackedMaskSample = tex2D(
                    _RockRiverMaskMap,
                    riverBedUv);
                fixed4 forestPackedMaskSample = tex2D(
                    _ForestStonesMaskMap,
                    forestFloorUv);
                fixed4 stonesPackedMaskSample = tex2D(
                    _ForestStonesMaskMap,
                    stonesUv);
                half2 rockMaskSample = rockPackedMaskSample.rg;
                half2 riverBedMaskSample = riverPackedMaskSample.ba;
                half2 forestFloorMaskSample = forestPackedMaskSample.rg;
                half2 stonesMaskSample = stonesPackedMaskSample.ba;
                half rockTextureWeight = HeightModulatedTextureWeight(
                    rockTextureSlopeWeight,
                    rockMaskSample.r,
                    _RockHeightBlendStrength);
                half riverBedTextureWeight = HeightModulatedTextureWeight(
                    riverBedTextureSlopeWeight,
                    riverBedMaskSample.r,
                    _RiverBedHeightBlendStrength);
                half forestFloorTextureWeight = HeightModulatedTextureWeight(
                    forestFloorTextureSlopeWeight,
                    forestFloorMaskSample.r,
                    _ForestFloorHeightBlendStrength);
                half stonesTextureWeight = HeightModulatedTextureWeight(
                    stonesTextureSlopeWeight,
                    stonesMaskSample.r,
                    _StonesHeightBlendStrength);
                // Signed world-space noise shifts both sides of the cutoff.
                // This prevents a linear interpolant from drawing the
                // underlying terrain triangles into the material boundary.
                half cliffBoundaryNoise = clamp(
                    broadNoise.r * 0.65 + macroNoise.g * 0.35,
                    -1.0,
                    1.0);
                half cutoffNormal = normal.y
                    - cliffBoundaryNoise * _CliffBoundaryNoiseStrength;
                half cliffWeight = AntialiasedMask(_CliffNormalCutoff - cutoffNormal);
                half rockBoundaryNoise = clamp(
                    broadNoise.b * 0.55 + macroNoise.b * 0.45,
                    -1.0,
                    1.0);
                half forcedRockBoundaryNoise = clamp(
                    (rockPatchLayers.b * 2.0h - 1.0h) * 0.35h
                        + bankDetailNoise.r * 0.65h,
                    -1.0,
                    1.0);
                // Exact maximum hardness is the one-hot forced-rock value.
                // Two finer coherent scales push river banks around inside the
                // interpolation band instead of following straight mesh edges.
                half forcedRockBlendWidth = max(_RiverEdgeBlendWidth, 0.001h);
                half forcedRockBlendStart = saturate(
                    1.0h
                        - forcedRockBlendWidth
                        + forcedRockBoundaryNoise
                            * forcedRockBlendWidth
                            * 0.75h);
                half forcedRockCoverage = smoothstep(
                    forcedRockBlendStart,
                    1.0h,
                    input.material.r);
                half hardness = saturate(input.material.r);
                half looseCover = saturate(input.material.g);
                half riverBed = saturate(input.material.b);
                half riverNoise = clamp(
                    dot(broadNoise, half3(0.577h, -0.577h, 0.577h)),
                    -1.0h,
                    1.0h);
                half riverThreshold = 0.5h
                    + riverNoise * _RiverEdgeNoiseStrength;
                half riverDistance = riverBed - riverThreshold;
                half riverTransition = max(
                    _RiverEdgeBlendWidth,
                    fwidth(riverDistance));
                half riverHeightDistance = riverDistance
                    + (riverBedMaskSample.r * 2.0h - 1.0h)
                        * riverTransition
                        * _RiverBedHeightBlendStrength;
                half riverCoverage = smoothstep(
                    -riverTransition,
                    riverTransition,
                    riverHeightDistance);
                // The exported alpha channel stays one through two metres from
                // the connected sea, then fades to zero at twenty metres.
                // Keep loose cover and the progressive noise threshold so the
                // beach boundary remains naturally broken up.
                half seaProximity = saturate(input.material.a);
                half sandAltitudeWeight = 1.0h - smoothstep(
                    2.0h,
                    4.0h,
                    elevation);
                half sandRichness = looseCover
                    * seaProximity
                    * sandAltitudeWeight
                    * (1.0 - riverCoverage);
                float2 sandPatchUv = input.islandLocalPosition.xz
                    / max(_SandPatchNoiseWorldSize, 0.1)
                    + float2(0.37, 0.73);
                half2 sandPatchLayers = tex2D(
                    _GrassPatchNoise,
                    sandPatchUv).rg;
                half sandPatchNoise = sandPatchLayers.r * 0.40
                    + sandPatchLayers.g * 0.60;
                half beachCandidateCoverage = AntialiasedMask(
                    sandPatchNoise - (1.0 - sandRichness))
                    * step(1.0e-4, sandRichness);

                half geologyRockWeight = saturate(
                    slope * lerp(1.3, 3.0, hardness));
                half rockPatchNoise = rockPatchLayers.r * 0.65
                    + rockPatchLayers.g * 0.35;
                // Treat the coherent rock stencil as a broad blend instead of
                // a binary threshold. Keep the richness ramp too, so a trace
                // of slope/hardness cannot produce fully opaque rock flecks.
                half rockMaskDistance = rockPatchNoise
                    - (1.0 - geologyRockWeight);
                half rockBlendWidth = max(
                    0.20h,
                    fwidth(rockMaskDistance));
                half geologyRockCoverage = smoothstep(
                    -rockBlendWidth,
                    rockBlendWidth,
                    rockMaskDistance)
                    * smoothstep(0.0h, 0.20h, geologyRockWeight);
                // Loose beach deposits cannot sustain slopes as steep as
                // ordinary earth. Use raw geometric slope so underlying
                // bedrock hardness does not make sand artificially stronger.
                half sandRockThreshold = _SandRockSlopeThreshold
                    + rockBoundaryNoise * _RockBoundaryNoiseStrength * 0.25;
                half sandRockCoverage = beachCandidateCoverage
                    * AntialiasedMask(slope - sandRockThreshold);
                geologyRockCoverage = max(
                    geologyRockCoverage,
                    sandRockCoverage);
                geologyRockCoverage = max(
                    geologyRockCoverage,
                    forcedRockCoverage);
                half exposedRockCoverage = max(
                    geologyRockCoverage,
                    cliffWeight);

                // The fur shader keeps this field as a hard physical stencil.
                // The ground shader instead broadens the same boundary so the
                // green surface blends naturally into bare dirt beneath it.
                float2 grassPatchUv = input.islandLocalPosition.xz
                    / max(_GrassPatchNoiseWorldSize, 0.1);
                half2 grassPatchLayers = tex2D(
                    _GrassPatchNoise,
                    grassPatchUv).rg;
                half grassPatchNoise = grassPatchLayers.r * 0.65
                    + grassPatchLayers.g * 0.35;
                half grassPatchDistance = grassPatchNoise
                    - (1.0h - looseCover);
                half groundGrassBlendWidth = max(
                    0.18h,
                    fwidth(grassPatchDistance));
                half groundGrassPresence = smoothstep(
                    -groundGrassBlendWidth,
                    groundGrassBlendWidth,
                    grassPatchDistance)
                    * smoothstep(0.0h, 0.20h, looseCover);
                half groundBeachCoverage = beachCandidateCoverage
                    * (1.0 - exposedRockCoverage);
                float noisySnowLine = _SnowLine
                    + macroNoise.r * _SnowMacroNoiseMetres
                    + broadNoise.g * _SnowEdgeNoiseMetres;
                half snowCoverage = AntialiasedMask(
                    elevation - noisySnowLine)
                    * (1.0 - cliffWeight);
                half groundGrassCoverage = groundGrassPresence
                    * (1.0h - exposedRockCoverage)
                    * (1.0h - groundBeachCoverage)
                    * (1.0h - riverCoverage)
                    * (1.0h - snowCoverage)
                    * step(0.0, elevation);
                half forestFloorCoverage = forestFloorSource
                    * forestFloorTextureWeight
                    * (1.0h - exposedRockCoverage)
                    * (1.0h - groundBeachCoverage)
                    * (1.0h - riverCoverage)
                    * (1.0h - snowCoverage)
                    * step(0.0h, elevation);
                half stonesCoverage = stonesSource
                    * (1.0h - exposedRockCoverage)
                    * (1.0h - groundBeachCoverage)
                    * (1.0h - riverCoverage)
                    * (1.0h - snowCoverage)
                    * (1.0h - forestFloorCoverage)
                    * step(0.0h, elevation);

                fixed3 deep = fixed3(0.08, 0.16, 0.12);
                fixed3 sand = fixed3(0.62, 0.57, 0.34);
                // Soil still blends continuously, but established green grass
                // uses the same hard visible-rock cutoff as the fur layer.
                float2 grassColorUv = input.islandLocalPosition.xz
                    / max(_GrassColorNoiseWorldSize, 1.0);
                half grassColorNoise = tex2D(
                    _GrassPatchNoise,
                    grassColorUv).b;
                fixed3 establishedGrassColor = lerp(
                    _GrassColorA.rgb,
                    _GrassColorB.rgb,
                    smoothstep(0.1h, 0.9h, grassColorNoise));
                half groundGrassColorCoverage = groundGrassCoverage
                    * (1.0h - stonesCoverage)
                    * (1.0h - step(0.01h, exposedRockCoverage));
                fixed3 grassBase = lerp(
                    _GrassThinDepositColor.rgb,
                    _GroundDirtColor.rgb,
                    stonesCoverage);
                fixed3 grass = lerp(
                    grassBase,
                    establishedGrassColor,
                    groundGrassColorCoverage);
                fixed3 rock = lerp(
                    _RockColor.rgb,
                    tex2D(_RockAlbedoMap, rockUv).rgb,
                    rockTextureWeight);
                fixed3 riverBedSurface = lerp(
                    _RiverBedColor.rgb,
                    tex2D(_RiverBedAlbedoMap, riverBedUv).rgb,
                    riverBedTextureWeight);
                fixed3 snow = fixed3(0.82, 0.84, 0.81);
                fixed3 baseColor;
                half beachCoverage = 0.0;
                half grassGroundCoverage = 0.0;
                half riverSurfaceCoverage = riverCoverage
                    * (1.0h - exposedRockCoverage);
                if (elevation < 0.0)
                {
                    baseColor = lerp(deep, sand, saturate((elevation + 8.0) / 8.0));
                }
                else
                {
                    baseColor = lerp(
                        grass,
                        rock,
                        exposedRockCoverage);

                    beachCoverage = beachCandidateCoverage
                        * (1.0 - exposedRockCoverage);
                    baseColor = lerp(baseColor, sand, beachCoverage);

                    grassGroundCoverage = (1.0 - exposedRockCoverage)
                        * (1.0 - beachCoverage)
                        * (1.0 - riverCoverage);
                    float dirtDistance = distance(
                        input.worldPosition.xz,
                        _GrassPlayerPosition.xz);
                    half dirtProximity = 1.0 - smoothstep(
                        max(_GroundDirtCoreRadius, 0.0),
                        max(_GroundDirtCoreRadius, 0.0)
                            + max(_GroundDirtFadeWidth, 0.001),
                        dirtDistance);
                    half dirtCoverage = _GrassEnabled
                        * grassGroundCoverage
                        * groundGrassCoverage
                        * dirtProximity;
                    baseColor = lerp(
                        baseColor,
                        _GroundDirtColor.rgb,
                        dirtCoverage);
                }
                fixed3 forestFloorSurface = tex2D(
                    _ForestFloorAlbedoMap,
                    forestFloorUv).rgb * _ForestFloorColor.rgb;
                baseColor = lerp(
                    baseColor,
                    forestFloorSurface,
                    forestFloorCoverage);
                baseColor = lerp(
                    baseColor,
                    lerp(
                        _GroundDirtColor.rgb,
                        tex2D(_StonesAlbedoMap, stonesUv).rgb
                            * _StonesColor.rgb,
                        stonesTextureSlopeWeight),
                    stonesCoverage);
                grassGroundCoverage *= (1.0h - forestFloorCoverage)
                    * (1.0h - stonesCoverage);
                // Use the height-shaped coverage directly instead of
                // restoring a binary bank edge.
                baseColor = lerp(
                    baseColor,
                    riverBedSurface,
                    riverSurfaceCoverage);
                half surfaceGeologyRockCoverage = geologyRockCoverage;
                baseColor = lerp(
                    baseColor,
                    rock,
                    surfaceGeologyRockCoverage);
                baseColor = lerp(baseColor, snow, snowCoverage);

                // Geology-classified rock remains authoritative over sediment,
                // beaches, and rivers, but high rock may carry snow. True
                // normal-cutoff cliffs still override every surface class.
                baseColor = lerp(baseColor, rock, cliffWeight);

                // River-bed coverage already occupies material.z. At the
                // coast, everything at or below sea level is fully wet and
                // the first five centimetres above it provide the blend-out.
                half coastalWetness = 1.0h - smoothstep(0.0h, 0.05h, elevation);
                half wetnessSource = max(
                    saturate(input.material.z),
                    coastalWetness);
                half wetness = pow(
                    saturate(wetnessSource),
                    max(_WetBankBlendExponent, 0.01h));
                baseColor *= 1.0h - wetness * _WetDarkening;

                // Snow owns its visible normal detail instead of inheriting
                // the coarser stone beneath it. Cliffs remain stone because
                // snowCoverage already excludes the normal-cutoff cliffs.
                half surfaceExposedRockCoverage = max(
                    surfaceGeologyRockCoverage,
                    cliffWeight);
                half stoneNormalCoverage = max(
                    surfaceExposedRockCoverage,
                    riverCoverage) * (1.0 - snowCoverage);
                UNITY_BRANCH
                if (stoneNormalCoverage > 0.01 && _CliffNormalStrength > 0.0)
                {
                    half3 detailNoise = tex3D(
                        _CliffNoise3D,
                        noisePosition * _CliffNoiseDetailScale
                            + float3(0.37, 0.61, 0.83)).rgb * 2.0 - 1.0;
                    half3 perturbation = broadNoise * 0.45 + detailNoise * 0.55;
                    perturbation -= normal * dot(perturbation, normal);
                    normal = normalize(
                        normal
                            + perturbation
                                * (_CliffNormalStrength * stoneNormalCoverage));
                }

                half soilCoverage = max(beachCoverage, grassGroundCoverage)
                    * (1.0 - snowCoverage)
                    * (1.0 - cliffWeight)
                    * (1.0 - forestFloorCoverage)
                    * (1.0 - stonesCoverage);
                UNITY_BRANCH
                if (soilCoverage > 0.01
                    && max(
                        max(_DirtNormalStrength, _GrassNormalStrength),
                        _SandNormalStrength) > 0.0)
                {
                    half soilDetailScale = lerp(
                        _GrassNormalDetailScale,
                        _SandNormalDetailScale,
                        saturate(beachCoverage));
                    half grassOrDirtNormalStrength = lerp(
                        _DirtNormalStrength,
                        _GrassNormalStrength,
                        groundGrassCoverage);
                    half soilNormalStrength = lerp(
                        grassOrDirtNormalStrength,
                        _SandNormalStrength,
                        saturate(beachCoverage));
                    half3 soilNoise = tex3D(
                        _CliffNoise3D,
                        noisePosition * soilDetailScale
                            + float3(0.79, 0.17, 0.53)).rgb * 2.0 - 1.0;
                    soilNoise -= normal * dot(soilNoise, normal);
                    normal = normalize(
                        normal
                            + soilNoise
                                * (soilNormalStrength * soilCoverage));
                }

                UNITY_BRANCH
                if (snowCoverage > 0.01 && _SnowNormalStrength > 0.0)
                {
                    half3 snowNoise = tex3D(
                        _CliffNoise3D,
                        noisePosition * _SnowNormalDetailScale
                            + float3(0.19, 0.89, 0.43)).rgb * 2.0 - 1.0;
                    snowNoise -= normal * dot(snowNoise, normal);
                    normal = normalize(
                        normal
                            + snowNoise
                                * (_SnowNormalStrength * snowCoverage));
                }

                half rockTextureCoverage = surfaceExposedRockCoverage
                    * (1.0h - snowCoverage)
                    * rockTextureWeight;
                half riverTextureCoverage = riverCoverage
                    * (1.0h - surfaceExposedRockCoverage)
                    * (1.0h - snowCoverage)
                    * riverBedTextureWeight;
                half forestFloorTextureCoverage = forestFloorCoverage;
                half stonesTextureCoverage = stonesCoverage
                    * stonesTextureWeight;
                UNITY_BRANCH
                if (rockTextureCoverage > 0.01h
                    && _RockNormalMapStrength > 0.0h)
                {
                    half3 rockTangentNormal = UnpackNormal(tex2D(
                        _RockNormalMap,
                        rockUv));
                    half3 rockLocalNormal = normalize(half3(
                        rockTangentNormal.x,
                        rockTangentNormal.z,
                        rockTangentNormal.y));
                    half3 rockWorldNormal = normalize(mul(
                        (float3x3)unity_ObjectToWorld,
                        rockLocalNormal));
                    normal = normalize(lerp(
                        normal,
                        rockWorldNormal,
                        saturate(
                            rockTextureCoverage
                                * _RockNormalMapStrength)));
                }
                UNITY_BRANCH
                if (riverTextureCoverage > 0.01h
                    && _RiverBedNormalMapStrength > 0.0h)
                {
                    half3 riverTangentNormal = UnpackNormal(tex2D(
                        _RiverBedNormalMap,
                        riverBedUv));
                    half3 riverLocalNormal = normalize(half3(
                        riverTangentNormal.x,
                        riverTangentNormal.z,
                        riverTangentNormal.y));
                    half3 riverWorldNormal = normalize(mul(
                        (float3x3)unity_ObjectToWorld,
                        riverLocalNormal));
                    normal = normalize(lerp(
                        normal,
                        riverWorldNormal,
                        saturate(
                            riverTextureCoverage
                                * _RiverBedNormalMapStrength)));
                }
                UNITY_BRANCH
                if (forestFloorTextureCoverage > 0.01h
                    && _ForestFloorNormalMapStrength > 0.0h)
                {
                    half3 forestFloorTangentNormal = UnpackNormal(tex2D(
                        _ForestFloorNormalMap,
                        forestFloorUv));
                    half3 forestFloorLocalNormal = normalize(half3(
                        forestFloorTangentNormal.x,
                        forestFloorTangentNormal.z,
                        forestFloorTangentNormal.y));
                    half3 forestFloorWorldNormal = normalize(mul(
                        (float3x3)unity_ObjectToWorld,
                        forestFloorLocalNormal));
                    normal = normalize(lerp(
                        normal,
                        forestFloorWorldNormal,
                        saturate(
                            forestFloorTextureCoverage
                                * _ForestFloorNormalMapStrength)));
                }
                UNITY_BRANCH
                if (stonesTextureCoverage > 0.01h
                    && _StonesNormalMapStrength > 0.0h)
                {
                    half3 stonesTangentNormal = UnpackNormal(tex2D(
                        _StonesNormalMap,
                        stonesUv));
                    half3 stonesLocalNormal = normalize(half3(
                        stonesTangentNormal.x,
                        stonesTangentNormal.z,
                        stonesTangentNormal.y));
                    half3 stonesWorldNormal = normalize(mul(
                        (float3x3)unity_ObjectToWorld,
                        stonesLocalNormal));
                    normal = normalize(lerp(
                        normal,
                        stonesWorldNormal,
                        saturate(
                            stonesTextureCoverage
                                * _StonesNormalMapStrength)));
                }

                // Distant ground grass has no fur geometry, so carry the same
                // advected wind field into its lighting normal. Restrict the
                // perturbation to the final grass material coverage so sand,
                // snow, rock, and riverbed shading remain stationary.
                float3 groundWind = MotuGrassWindSample(input.worldPosition.xz);
                float3 horizontalGroundWind = float3(
                    groundWind.x,
                    0.0,
                    groundWind.z);
                float3 tangentGroundWind = horizontalGroundWind
                    - normal * dot(horizontalGroundWind, normal);
                tangentGroundWind *= rsqrt(max(
                    dot(tangentGroundWind, tangentGroundWind),
                    1.0e-4));
                normal = normalize(
                    normal
                        + tangentGroundWind
                            * (groundWind.y
                                * _GrassWindNormalStrength
                                * groundGrassCoverage));

                half rockTextureOcclusion = lerp(
                    1.0h,
                    rockMaskSample.g,
                    _RockTextureOcclusionStrength);
                half riverTextureOcclusion = lerp(
                    1.0h,
                    riverBedMaskSample.g,
                    _RiverBedTextureOcclusionStrength);
                half forestFloorTextureOcclusion = lerp(
                    1.0h,
                    forestFloorMaskSample.g,
                    _ForestFloorTextureOcclusionStrength);
                half stonesTextureOcclusion = lerp(
                    1.0h,
                    stonesMaskSample.g,
                    _StonesTextureOcclusionStrength);
                half materialTextureOcclusion = lerp(
                    1.0h,
                    rockTextureOcclusion,
                    rockTextureCoverage);
                materialTextureOcclusion = lerp(
                    materialTextureOcclusion,
                    riverTextureOcclusion,
                    riverTextureCoverage);
                materialTextureOcclusion = lerp(
                    materialTextureOcclusion,
                    forestFloorTextureOcclusion,
                    forestFloorTextureCoverage);
                materialTextureOcclusion = lerp(
                    materialTextureOcclusion,
                    stonesTextureOcclusion,
                    stonesTextureCoverage);
                occlusion *= materialTextureOcclusion;

                float3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                half diffuse = saturate(dot(normal, lightDirection)) * attenuation;
                half3 direct = _LightColor0.rgb * diffuse;
                half3 ambient = ShadeSH9(half4(normal, 1.0));
                float3 viewDirection = normalize(
                    UnityWorldSpaceViewDir(input.worldPosition));
                float3 halfDirection = normalize(
                    lightDirection + viewDirection);
                half wetHighlightPower = lerp(
                    10.0h,
                    110.0h,
                    _WetSmoothness);
                half wetHighlight = pow(
                    saturate(dot(normal, halfDirection)),
                    wetHighlightPower)
                    * _WetSpecularStrength
                    * wetness
                    * attenuation
                    * step(0.0h, dot(normal, lightDirection));
                // A small grazing response keeps wet ground legible away from
                // the main sun highlight without pretending to be a mirror.
                half wetFresnel = pow(
                    1.0h - saturate(dot(normal, viewDirection)),
                    4.0h)
                    * (_WetSpecularStrength * 0.12h)
                    * wetness;
                half3 wetReflection = _LightColor0.rgb * wetHighlight
                    + ambient * wetFresnel * occlusion;
                fixed4 color = fixed4(
                    baseColor * _Color.rgb * occlusion * (ambient + direct)
                        + wetReflection,
                    1.0);
                UNITY_APPLY_FOG(input.fogCoord, color);
                return color;
            }
            ENDCG
        }

        UsePass "Legacy Shaders/VertexLit/SHADOWCASTER"
    }

    FallBack "Diffuse"
}
