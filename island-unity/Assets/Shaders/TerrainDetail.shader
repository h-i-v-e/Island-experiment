Shader "Motu/Terrain Unified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        _RockColor ("Exposed Rock", Color) = (0.34, 0.32, 0.29, 1)
        [NoScaleOffset] _RockAlbedoMap ("Rock Top Surface Colour", 2D) = "white" {}
        [NoScaleOffset] _RockNormalMap ("Rock Top Surface Normal", 2D) = "bump" {}
        [NoScaleOffset] _RockMaskMap ("Rock Top Surface Mask (R Height, G Occlusion)", 2D) = "gray" {}
        _RockTextureWorldSize ("Rock Top Surface Texture Size (metres)", Float) = 4
        _RockNormalMapStrength ("Rock Top Surface Normal Strength", Range(0, 2)) = 0
        _RockHeightBlendStrength ("Rock Slope Blend Height Influence", Range(0, 1)) = 1
        _RockTextureOcclusionStrength ("Rock Top Surface Texture Occlusion", Range(0, 1)) = 0
        _RiverBedColor ("Riverbed Tint", Color) = (0.34, 0.32, 0.29, 1)
        [NoScaleOffset] _RiverBedAlbedoMap ("Riverbed Colour", 2D) = "white" {}
        [NoScaleOffset] _RiverBedNormalMap ("Riverbed Normal", 2D) = "bump" {}
        [NoScaleOffset] _RiverBedMaskMap ("Riverbed Mask (R Height, G Occlusion)", 2D) = "gray" {}
        _RiverBedTextureWorldSize ("Riverbed Texture Size (metres)", Float) = 2
        _RiverBedNormalMapStrength ("Riverbed Normal Strength", Range(0, 2)) = 0
        _RiverBedHeightBlendStrength ("Riverbed Slope Blend Height Influence", Range(0, 1)) = 1
        _RiverBedTextureOcclusionStrength ("Riverbed Texture Occlusion", Range(0, 1)) = 0
        _TopTextureFadeOutSlope ("Rock and Riverbed Texture Fade-Out Slope (degrees)", Range(1, 89)) = 45
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
        _GrassThinDepositColor ("Bare Grass Ground", Color) = (0.55, 0.46, 0.28, 1)
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
        _GroundDirtColor ("Grass Ground Dirt", Color) = (0.24, 0.14, 0.07, 1)
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
                half3 material : TEXCOORD5;
                float3 islandLocalPosition : TEXCOORD6;
            };

            sampler2D _WorldNormal;
            sampler2D _Occlusion;
            sampler2D _GrassPatchNoise;
            sampler2D _RockAlbedoMap;
            sampler2D _RockNormalMap;
            sampler2D _RockMaskMap;
            sampler2D _RiverBedAlbedoMap;
            sampler2D _RiverBedNormalMap;
            sampler2D _RiverBedMaskMap;
            sampler3D _CliffNoise3D;
            fixed4 _Color;
            fixed4 _RockColor;
            fixed4 _RiverBedColor;
            float _RockTextureWorldSize;
            half _RockNormalMapStrength;
            half _RockHeightBlendStrength;
            half _RockTextureOcclusionStrength;
            float _RiverBedTextureWorldSize;
            half _RiverBedNormalMapStrength;
            half _RiverBedHeightBlendStrength;
            half _RiverBedTextureOcclusionStrength;
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
                output.material = input.material.rgb;
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
                fixed4 rockMaskSample = tex2D(_RockMaskMap, rockUv);
                float2 riverBedUv = input.islandLocalPosition.xz
                    / max(_RiverBedTextureWorldSize, 0.01);
                fixed4 riverBedMaskSample = tex2D(
                    _RiverBedMaskMap,
                    riverBedUv);
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
                half topTextureSlopeWeight = smoothstep(
                    topTextureBlendStart,
                    1.0h,
                    noisyTextureUpNormal);
                half flatTexturePatchWeight = lerp(
                    0.0h,
                    1.0h,
                    smoothstep(-0.65h, 0.65h, simpleStoneBlendNoise));
                topTextureSlopeWeight *= flatTexturePatchWeight;
                half rockTextureWeight = HeightModulatedTextureWeight(
                    topTextureSlopeWeight,
                    rockMaskSample.r,
                    _RockHeightBlendStrength);
                half riverBedTextureWeight = HeightModulatedTextureWeight(
                    topTextureSlopeWeight,
                    riverBedMaskSample.r,
                    _RiverBedHeightBlendStrength);
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
                // Retain the legacy river-texture properties on existing
                // materials, but river beds no longer select that surface.
                half riverCoverage = 0.0h;
                // The exported blue channel stays one through two metres from
                // the connected sea, then fades to zero at twenty metres.
                // Keep loose cover and the progressive noise threshold so the
                // beach boundary remains naturally broken up.
                half seaProximity = saturate(input.material.b);
                half sandRichness = looseCover
                    * seaProximity
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

                fixed3 deep = fixed3(0.08, 0.16, 0.12);
                fixed3 sand = fixed3(0.62, 0.57, 0.34);
                // Ground colour is deliberately softer than the physical fur
                // mask, blending continuously between bare soil and grass.
                float2 grassColorUv = input.islandLocalPosition.xz
                    / max(_GrassColorNoiseWorldSize, 1.0);
                half grassColorNoise = tex2D(
                    _GrassPatchNoise,
                    grassColorUv).b;
                fixed3 establishedGrassColor = lerp(
                    _GrassColorA.rgb,
                    _GrassColorB.rgb,
                    smoothstep(0.1h, 0.9h, grassColorNoise));
                fixed3 grass = lerp(
                    _GrassThinDepositColor.rgb,
                    establishedGrassColor,
                    groundGrassCoverage);
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
                // Riverbed height alters riverCoverage above; use that soft
                // weight directly instead of restoring a binary bank edge.
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
                    * (1.0 - cliffWeight);
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
                half materialTextureOcclusion = lerp(
                    1.0h,
                    rockTextureOcclusion,
                    rockTextureCoverage);
                materialTextureOcclusion = lerp(
                    materialTextureOcclusion,
                    riverTextureOcclusion,
                    riverTextureCoverage);
                occlusion *= materialTextureOcclusion;

                float3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                half diffuse = saturate(dot(normal, lightDirection)) * attenuation;
                half3 direct = _LightColor0.rgb * diffuse;
                half3 ambient = ShadeSH9(half4(normal, 1.0));
                fixed4 color = fixed4(
                    baseColor * _Color.rgb * occlusion * (ambient + direct),
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
