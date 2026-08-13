Shader "Motu/Terrain Unified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        [NoScaleOffset] _WorldNormal ("Shared World Normal", 2D) = "bump" {}
        [PerRendererData] _WorldNormalWeight ("World Normal Weight", Float) = 1
        [NoScaleOffset] _Occlusion ("Shared Occlusion", 2D) = "white" {}
        _OcclusionStrength ("Occlusion Strength", Range(0, 1)) = 0.25
        _SnowLine ("Snow Line (metres)", Float) = 15
        _SnowEdgeNoiseMetres ("Snow Edge Noise (metres)", Range(0, 10)) = 2.5
        _BeachEdgeNoiseMetres ("Beach Edge Noise (metres)", Range(0, 8)) = 2.5
        _RiverEdgeNoiseStrength ("River Edge Noise Strength", Range(0, 0.45)) = 0.20
        _RiverEdgeBlendWidth ("River Edge Blend Width", Range(0.01, 0.5)) = 0.20
        _CliffNormalCutoff ("Cliff Up-Normal Cutoff", Range(0, 1)) = 0.55
        _CliffBoundaryNoiseStrength ("Cliff Boundary Noise Strength", Range(0, 0.5)) = 0.30
        [NoScaleOffset] _CliffNoise3D ("Cliff 3D Noise", 3D) = "gray" {}
        _CliffNoisePeriod ("Cliff Noise Period (metres)", Float) = 160
        _CliffNoiseDetailScale ("Cliff Detail Frequency", Range(2, 32)) = 16
        _CliffNormalStrength ("Cliff Normal Strength", Range(0, 0.5)) = 0.12
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
            float _BeachEdgeNoiseMetres;
            half _RiverEdgeNoiseStrength;
            half _RiverEdgeBlendWidth;
            half _CliffNormalCutoff;
            half _CliffBoundaryNoiseStrength;
            float _CliffNoisePeriod;
            half _CliffNoiseDetailScale;
            half _CliffNormalStrength;

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
                // Noise may make a marginal face appear steeper for the
                // cutoff, but can never remove an already classified cliff.
                half boundarySteepening = max(broadNoise.r, 0.0)
                    * _CliffBoundaryNoiseStrength;
                half cutoffNormal = normal.y - boundarySteepening;
                half cliffWeight = AntialiasedMask(_CliffNormalCutoff - cutoffNormal);
                half hardness = saturate(input.material.r);
                half looseCover = saturate(input.material.g);
                half riverBed = saturate(input.material.b);
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

                fixed3 deep = fixed3(0.08, 0.16, 0.12);
                fixed3 sand = fixed3(0.62, 0.57, 0.34);
                fixed3 grass = fixed3(0.20, 0.48, 0.16);
                fixed3 rock = fixed3(0.34, 0.32, 0.29);
                fixed3 riverRock = fixed3(0.20, 0.18, 0.14);
                fixed3 riverSilt = fixed3(0.34, 0.27, 0.17);
                fixed3 snow = fixed3(0.82, 0.84, 0.81);
                fixed3 baseColor;
                if (elevation < 0.0)
                {
                    baseColor = lerp(deep, sand, saturate((elevation + 8.0) / 8.0));
                }
                else
                {
                    // Keep geology-driven exposure, but also classify genuinely
                    // steep faces directly from the final shading normal. This
                    // restores rocky cliffs without changing terrain geometry.
                    half geologyRockWeight = saturate(slope * lerp(1.3, 3.0, hardness));
                    half rockWeight = max(geologyRockWeight, cliffWeight);

                    // Loose deposits can cover sloping ground, but should not
                    // paint near-vertical cliff faces as soil or grass.
                    half looseRockMask = lerp(0.85, 0.10, cliffWeight);
                    rockWeight *= 1.0 - looseCover * looseRockMask;
                    baseColor = lerp(grass, rock, rockWeight);

                    float noisyBeachElevation = elevation
                        - broadNoise.b * _BeachEdgeNoiseMetres;
                    half coastProximity = 1.0
                        - smoothstep(0.5, 8.0, noisyBeachElevation);
                    half beachWeight = looseCover
                        * coastProximity
                        * (1.0 - riverCoverage)
                        * (1.0 - cliffWeight);
                    baseColor = lerp(baseColor, sand, beachWeight);

                    fixed3 riverColor = lerp(riverRock, riverSilt, looseCover);
                    baseColor = lerp(baseColor, riverColor, riverCoverage);
                }
                float noisySnowLine = _SnowLine + broadNoise.g * _SnowEdgeNoiseMetres;
                half snowCoverage = AntialiasedMask(elevation - noisySnowLine)
                    * (1.0 - cliffWeight);
                baseColor = lerp(baseColor, snow, snowCoverage);

                // Cliff exposure is authoritative: neither sediment, river
                // coverage, underwater colouring, nor snow may cover it.
                baseColor = lerp(baseColor, rock, cliffWeight);

                UNITY_BRANCH
                if (cliffWeight > 0.5 && _CliffNormalStrength > 0.0)
                {
                    half3 detailNoise = tex3D(
                        _CliffNoise3D,
                        noisePosition * _CliffNoiseDetailScale
                            + float3(0.37, 0.61, 0.83)).rgb * 2.0 - 1.0;
                    half3 perturbation = broadNoise * 0.45 + detailNoise * 0.55;
                    perturbation -= normal * dot(perturbation, normal);
                    normal = normalize(normal + perturbation * _CliffNormalStrength);
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
