Shader "Motu/Rock Decoration"
{
    Properties
    {
        _RockColor ("Rock Color", Color) = (0.30, 0.32, 0.29, 1)
        [PerRendererData] _RockTint ("Rock Tint", Color) = (1, 1, 1, 1)
        [NoScaleOffset] _CliffNoise3D ("Rock 3D Noise", 3D) = "gray" {}
        _CliffNoisePeriod ("Rock Noise Period (metres)", Float) = 160
        _CliffNoiseDetailScale ("Rock Detail Frequency", Range(2, 32)) = 16
        _CliffNormalStrength ("Rock Normal Strength", Range(0, 0.5)) = 0.12
    }

    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="Opaque" "Queue"="Geometry" "MotuReflection"="Rock" }
        LOD 250

        Pass
        {
            Tags { "LightMode"="UniversalForwardOnly" }

            HLSLPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS
            #pragma multi_compile_fragment _ _ADDITIONAL_LIGHT_SHADOWS
            #pragma multi_compile_fog
            #pragma multi_compile_instancing

            #include "MotuUrp.hlsl"



            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float3 worldPosition : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float3 shadowRelativePosition : TEXCOORD2;
                UNITY_FOG_COORDS(3)
                UNITY_VERTEX_INPUT_INSTANCE_ID
                UNITY_VERTEX_OUTPUT_STEREO
                float3 islandLocalPosition : TEXCOORD4;
            };

            sampler3D _CliffNoise3D;
            fixed4 _RockColor;
            float _CliffNoisePeriod;
            half _CliffNoiseDetailScale;
            half _CliffNormalStrength;
            float4x4 _IslandWorldToLocal;

            #include "CloudCommon.cginc"

            UNITY_INSTANCING_BUFFER_START(RockProperties)
                UNITY_DEFINE_INSTANCED_PROP(fixed4, _RockTint)
            UNITY_INSTANCING_BUFFER_END(RockProperties)

            VertexOutput Vertex(VertexInput v)
            {
                VertexOutput output;
                UNITY_SETUP_INSTANCE_ID(v);
                UNITY_INITIALIZE_OUTPUT(VertexOutput, output);
                UNITY_TRANSFER_INSTANCE_ID(v, output);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                output.pos = UnityObjectToClipPos(v.vertex);
                output.worldPosition = TransformObjectToWorld(v.vertex.xyz);
                output.shadowRelativePosition = MotuCameraRelativePosition(output.worldPosition);
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                output.worldNormal = UnityObjectToWorldNormal(v.normal);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                UNITY_SETUP_INSTANCE_ID(input);
                float3 normal = normalize(input.worldNormal);
                float noisePeriod = max(_CliffNoisePeriod, 1.0);
                float3 noisePosition = input.islandLocalPosition / noisePeriod;
                half3 broadNoise = tex3D(_CliffNoise3D, noisePosition).rgb * 2.0 - 1.0;
                half3 detailNoise = tex3D(
                    _CliffNoise3D,
                    noisePosition * _CliffNoiseDetailScale
                        + float3(0.37, 0.61, 0.83)).rgb * 2.0 - 1.0;
                half3 perturbation = broadNoise * 0.45 + detailNoise * 0.55;
                perturbation -= normal * dot(perturbation, normal);
                normal = normalize(normal + perturbation * _CliffNormalStrength);

                half broadVariation = dot(broadNoise, half3(0.45, 0.35, 0.20));
                fixed3 tint = UNITY_ACCESS_INSTANCED_PROP(RockProperties, _RockTint).rgb;
                fixed3 albedo = _RockColor.rgb * tint * (1.0 + broadVariation * 0.10);
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                half3 ambient = ShadeSH9(half4(normal, 1.0))
                    * cloud.ambientTransmittance;
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half diffuse = saturate(dot(normal, lightDirection));
                half attenuation = MotuMainLightShadowAttenuation(input.shadowRelativePosition);
                fixed3 color = albedo
                    * (ambient + _LightColor0.rgb
                        * diffuse
                        * attenuation
                        * cloud.directTransmittance
                        + MotuAdditionalLighting(input.worldPosition, normal));
                fixed4 result = fixed4(color, 1.0);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDHLSL
        }

        Pass
        {
            Tags { "LightMode"="ShadowCaster" }
            ZWrite On
            ZTest LEqual

            HLSLPROGRAM
            #pragma vertex ShadowVertex
            #pragma fragment ShadowFragment
            #pragma target 3.5
            #pragma multi_compile_vertex _ _CASTING_PUNCTUAL_LIGHT_SHADOW
            #pragma multi_compile_instancing

            #include "MotuUrp.hlsl"

            struct ShadowInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct ShadowOutput
            {
                V2F_SHADOW_CASTER;
                UNITY_VERTEX_OUTPUT_STEREO
            };

            ShadowOutput ShadowVertex(ShadowInput v)
            {
                ShadowOutput output;
                UNITY_SETUP_INSTANCE_ID(v);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                TRANSFER_SHADOW_CASTER_NORMALOFFSET(output)
                return output;
            }

            float4 ShadowFragment(ShadowOutput input) : SV_Target
            {
                SHADOW_CASTER_FRAGMENT(input)
            }
            ENDHLSL
        }
        UsePass "Motu/Planar Reflection Simplified/ReflectionRock"

    }

    FallBack Off
}
