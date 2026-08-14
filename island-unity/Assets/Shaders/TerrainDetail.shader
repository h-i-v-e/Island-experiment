Shader "Motu/Terrain Unified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        [NoScaleOffset] _WorldNormal ("Shared World Normal", 2D) = "bump" {}
        [PerRendererData] _WorldNormalWeight ("World Normal Weight", Float) = 1
        [NoScaleOffset] _Occlusion ("Shared Occlusion", 2D) = "white" {}
        _OcclusionStrength ("Occlusion Strength", Range(0, 1)) = 0.25
        _SnowLine ("Snow Line (metres)", Float) = 100
        _SnowEdgeNoiseMetres ("Snow Edge Noise (metres)", Range(0, 10)) = 2.5
        _SnowMacroNoiseMetres ("Snow Macro Noise (metres)", Range(0, 40)) = 18
        _BeachEdgeNoiseMetres ("Beach Edge Noise (metres)", Range(0, 8)) = 2.5
        _BeachMaximumElevation ("Beach Maximum Elevation (metres)", Float) = 1
        _RiverEdgeNoiseStrength ("River Edge Noise Strength", Range(0, 0.45)) = 0.20
        _RiverEdgeBlendWidth ("River Edge Blend Width", Range(0.01, 0.5)) = 0.20
        _CliffNormalCutoff ("Cliff Up-Normal Cutoff", Range(0, 1)) = 0.55
        _CliffBoundaryNoiseStrength ("Cliff Boundary Noise Strength", Range(0, 0.5)) = 0.30
        _RockBoundaryNoiseStrength ("Rock Boundary Noise Strength", Range(0, 0.4)) = 0.18
        _SandRockSlopeThreshold ("Sand Rock Slope Threshold", Range(0, 0.5)) = 0.10
        [NoScaleOffset] _CliffNoise3D ("Cliff 3D Noise", 3D) = "gray" {}
        _CliffNoisePeriod ("Cliff Noise Period (metres)", Float) = 160
        _CliffNoiseDetailScale ("Cliff Detail Frequency", Range(2, 32)) = 16
        _CliffNormalStrength ("Cliff Normal Strength", Range(0, 0.5)) = 0.12
        _GrassNormalDetailScale ("Grass Detail Frequency", Range(16, 192)) = 96
        _SandNormalDetailScale ("Sand Detail Frequency", Range(32, 1024)) = 768
        _GrassNormalStrength ("Grass Normal Strength", Range(0, 0.5)) = 0.22
        _SandNormalStrength ("Sand Normal Strength", Range(0, 0.5)) = 0.10
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
            };

            sampler2D _WorldNormal;
            sampler2D _Occlusion;
            sampler3D _CliffNoise3D;
            fixed4 _Color;
            half _WorldNormalWeight;
            half _OcclusionStrength;
            float _SnowLine;
            float _SnowEdgeNoiseMetres;
            float _SnowMacroNoiseMetres;
            float _BeachEdgeNoiseMetres;
            float _BeachMaximumElevation;
            half _RiverEdgeNoiseStrength;
            half _RiverEdgeBlendWidth;
            half _CliffNormalCutoff;
            half _CliffBoundaryNoiseStrength;
            half _RockBoundaryNoiseStrength;
            half _SandRockSlopeThreshold;
            float _CliffNoisePeriod;
            half _CliffNoiseDetailScale;
            half _CliffNormalStrength;
            half _GrassNormalDetailScale;
            half _SandNormalDetailScale;
            half _GrassNormalStrength;
            half _SandNormalStrength;
            half _GrassEnabled;
            float3 _GrassPlayerPosition;
            fixed4 _GroundDirtColor;
            float _GroundDirtCoreRadius;
            float _GroundDirtFadeWidth;

            half AntialiasedMask(float signedDistance)
            {
                float transitionWidth = max(fwidth(signedDistance), 1.0e-4);
                return smoothstep(-transitionWidth, transitionWidth, signedDistance);
            }

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.pos = UnityObjectToClipPos(input.vertex);
                output.uv = input.uv;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
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
                    normal = normalize(float3(
                        encoded.r * 2.0 - 1.0,
                        encoded.b * 2.0 - 1.0,
                        encoded.g * 2.0 - 1.0));
                }

                half sampledOcclusion = tex2D(_Occlusion, input.uv).r;
                half occlusion = lerp(1.0, sampledOcclusion, _OcclusionStrength);
                // Mesh import scales normalized Rust coordinates into Unity
                // metres, so world-space Y is already the physical elevation.
                float elevation = input.worldPosition.y;
                float slope = 1.0 - saturate(normal.y);
                float noisePeriod = max(_CliffNoisePeriod, 1.0);
                float3 noisePosition = input.worldPosition / noisePeriod;
                half3 broadNoise = tex3D(_CliffNoise3D, noisePosition).rgb * 2.0 - 1.0;
                half3 macroNoise = tex3D(
                    _CliffNoise3D,
                    noisePosition * (1.0 / 3.0)
                        + float3(0.23, 0.71, 0.41)).rgb * 2.0 - 1.0;
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
                half hardness = saturate(input.material.r);
                half looseCover = saturate(input.material.g);
                half riverBed = saturate(input.material.b);
                half rockBoundaryNoise = clamp(
                    broadNoise.b * 0.55 + macroNoise.b * 0.45,
                    -1.0,
                    1.0);
                half riverNoise = clamp(
                    dot(broadNoise, half3(0.577, -0.577, 0.577)),
                    -1.0,
                    1.0);
                half riverThreshold = 0.5 + riverNoise * _RiverEdgeNoiseStrength;
                half riverDistance = riverBed - riverThreshold;
                half riverTransition = max(
                    _RiverEdgeBlendWidth,
                    fwidth(riverDistance));
                half riverCoverage = smoothstep(
                    -riverTransition,
                    riverTransition,
                    riverDistance);
                float noisyBeachElevation = elevation
                    - broadNoise.b * _BeachEdgeNoiseMetres;
                half coastCoverage = AntialiasedMask(
                    _BeachMaximumElevation - noisyBeachElevation);
                half beachDepositCoverage = AntialiasedMask(
                    looseCover - 0.5);
                half beachCandidateCoverage = beachDepositCoverage
                    * coastCoverage
                    * (1.0 - riverCoverage);

                half geologyRockWeight = saturate(
                    slope * lerp(1.3, 3.0, hardness));
                half rockThreshold = 0.5
                    + rockBoundaryNoise * _RockBoundaryNoiseStrength;
                half geologyRockCoverage = AntialiasedMask(
                    geologyRockWeight - rockThreshold);
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
                half exposedRockCoverage = max(
                    geologyRockCoverage,
                    cliffWeight);

                fixed3 deep = fixed3(0.08, 0.16, 0.12);
                fixed3 sand = fixed3(0.62, 0.57, 0.34);
                fixed3 grass = fixed3(0.20, 0.48, 0.16);
                fixed3 rock = fixed3(0.34, 0.32, 0.29);
                fixed3 snow = fixed3(0.82, 0.84, 0.81);
                fixed3 baseColor;
                half beachCoverage = 0.0;
                half grassGroundCoverage = 0.0;
                if (elevation < 0.0)
                {
                    baseColor = lerp(deep, sand, saturate((elevation + 8.0) / 8.0));
                }
                else
                {
                    baseColor = lerp(grass, rock, exposedRockCoverage);

                    beachCoverage = beachCandidateCoverage
                        * (1.0 - exposedRockCoverage);
                    baseColor = lerp(baseColor, sand, beachCoverage);

                    baseColor = lerp(
                        baseColor,
                        rock,
                        riverCoverage * (1.0 - exposedRockCoverage));

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
                        * dirtProximity;
                    baseColor = lerp(
                        baseColor,
                        _GroundDirtColor.rgb,
                        dirtCoverage);
                }
                // Preserve geology rock as an authoritative base class before
                // applying the one allowed overlay: high-altitude snow.
                baseColor = lerp(baseColor, rock, geologyRockCoverage);
                float noisySnowLine = _SnowLine
                    + macroNoise.r * _SnowMacroNoiseMetres
                    + broadNoise.g * _SnowEdgeNoiseMetres;
                half snowCoverage = AntialiasedMask(elevation - noisySnowLine)
                    * (1.0 - cliffWeight);
                baseColor = lerp(baseColor, snow, snowCoverage);

                // Geology-classified rock remains authoritative over sediment,
                // beaches, and rivers, but high rock may carry snow. True
                // normal-cutoff cliffs still override every surface class.
                baseColor = lerp(baseColor, rock, cliffWeight);

                half stoneNormalCoverage = max(
                    exposedRockCoverage,
                    riverCoverage);
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
                    && max(_GrassNormalStrength, _SandNormalStrength) > 0.0)
                {
                    half soilDetailScale = lerp(
                        _GrassNormalDetailScale,
                        _SandNormalDetailScale,
                        saturate(beachCoverage));
                    half soilNormalStrength = lerp(
                        _GrassNormalStrength,
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
