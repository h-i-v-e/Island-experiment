Shader "Motu/Wheat Canopy"
{
    Properties
    {
        _WheatTip ("Ripe Wheat", Color) = (0.89, 0.67, 0.30, 1)
        _WheatBase ("Straw Base", Color) = (0.43, 0.30, 0.12, 1)
        _WheatPorosity ("Canopy Gaps", Range(0, 0.5)) = 0.17
        [HideInInspector] _WheatFade ("Island Coverage", Range(0, 1)) = 1
        [HideInInspector] _WheatDetailPresence ("Detailed Wheat", Range(0, 1)) = 0
        [HideInInspector] _WheatDetailBlend ("Detail Fade", Vector) = (0, 0, 0, 0)
    }

    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="Opaque" "Queue"="Geometry" }

        HLSLINCLUDE
        #include "../MotuUrp.hlsl"
        #include "../CloudCommon.cginc"
        #include "../WheatDetailBlend.hlsl"
        #include "Packages/com.unity.render-pipelines.core/ShaderLibrary/Random.hlsl"

        CBUFFER_START(UnityPerMaterial)
            half4 _WheatTip;
            half4 _WheatBase;
            float _WheatPorosity;
            float _WheatFade;
            float _WheatDetailPresence;
            float4 _WheatDetailBlend;
        CBUFFER_END

        struct Attributes
        {
            float3 positionOS : POSITION;
            float3 normalOS : NORMAL;
            float2 fieldPosition : TEXCOORD0;
            half4 colour : COLOR;
            UNITY_VERTEX_INPUT_INSTANCE_ID
        };

        float GrainNoise(float2 position)
        {
            float2 cell = floor(position);
            float2 blend = frac(position);
            blend = blend * blend * (3.0 - 2.0 * blend);
            return lerp(
                lerp(MotuCloudHash(cell), MotuCloudHash(cell + float2(1, 0)), blend.x),
                lerp(MotuCloudHash(cell + float2(0, 1)), MotuCloudHash(cell + float2(1, 1)), blend.x),
                blend.y);
        }

        void ClipCoverage(float2 pixelPosition, float2 fieldPosition, float fieldSeed, float2 worldXZ)
        {
            float detail = _WheatDetailPresence * MotuWheatDetailCoverage(worldXZ, _WheatDetailBlend);
            float coverage = _WheatFade * (1.0 - detail);
            // Match the opposite side of the clump shader's dither threshold.
            if (coverage < 1.0)
                clip(coverage - max(1.0 - InterleavedGradientNoise(pixelPosition, 0), 0.00001));

            if (_WheatPorosity > 0.0)
            {
                // Vary the opening density in plot space so the pattern crosses tile seams.
                float pockets = GrainNoise(fieldPosition * float2(2.2, 4.1) + fieldSeed * 43.7);
                float gaps = _WheatPorosity * lerp(0.55, 1.45, pockets);
                clip(GenerateHashedRandomFloat((uint2)pixelPosition) - gaps);
            }
        }

        half3 WheatAlbedo(float2 position, half2 crop)
        {
            // UVs stay in plot metres across all tiles and align with the planted rows.
            float2 noisePosition = position + float2(37.7, 72.3) * (1.0 + crop.y);
            float broad = GrainNoise(noisePosition * float2(0.53, 0.77));
            float detail = GrainNoise(noisePosition * float2(1.95, 2.8));
            float mottle = saturate(0.69 * broad + 0.31 * detail);
            float row = position.y / 0.28;
            float rowDetail = 1.0 - abs(frac(row) * 2.0 - 1.0);
            rowDetail = lerp(rowDetail, 0.5, saturate(fwidth(row) * 1.5));
            half3 ripe = _WheatTip.rgb * float3(1.0, 0.83 + mottle * 0.23, 0.67 + mottle * 0.49);
            ripe *= lerp(0.79, 1.12, mottle);
            half3 straw = _WheatBase.rgb * lerp(0.83, 1.17, detail);
            // Blend the ears with the darker gaps between rows as the clumps
            // resolve into one surface. Keep the average stable at a distance.
            half grainCover = 0.41 + mottle * 0.18 + rowDetail * 0.29;
            return lerp(straw, ripe, crop.x * grainCover);
        }
        ENDHLSL

        Pass
        {
            Name "Forward"
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

            struct Varyings
            {
                float4 positionCS : SV_POSITION;
                float3 positionWS : TEXCOORD0;
                half3 normalWS : TEXCOORD1;
                float2 fieldPosition : TEXCOORD2;
                half2 crop : TEXCOORD3;
                float3 shadowRelativePosition : TEXCOORD4;
                UNITY_FOG_COORDS(5)
                UNITY_VERTEX_OUTPUT_STEREO
            };

            Varyings Vertex(Attributes input)
            {
                UNITY_SETUP_INSTANCE_ID(input);
                Varyings output = (Varyings)0;
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                output.positionWS = TransformObjectToWorld(input.positionOS);
                output.positionCS = TransformWorldToHClip(output.positionWS);
                output.normalWS = TransformObjectToWorldNormal(input.normalOS);
                output.fieldPosition = input.fieldPosition;
                output.crop = input.colour.rg;
                output.shadowRelativePosition = MotuCameraRelativePosition(output.positionWS);
                UNITY_TRANSFER_FOG(output, output.positionCS);
                return output;
            }

            half4 Fragment(Varyings input) : SV_Target
            {
                ClipCoverage(input.positionCS.xy, input.fieldPosition, input.crop.y, input.positionWS.xz);
                half3 normal = normalize(input.normalWS);
                half3 albedo = WheatAlbedo(input.fieldPosition, input.crop);
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.positionWS);
                half3 ambient = max(SampleSH(normal) * cloud.ambientTransmittance,
                    MotuDaylightAmbientFloor(0.10h));
                half diffuse = saturate(dot(normal, _MainLightPosition.xyz));
                half shadow = MotuMainLightShadowAttenuation(input.shadowRelativePosition);
                half3 direct = _MainLightColor.rgb * diffuse * shadow * cloud.directTransmittance;
                half4 result = half4(albedo * (ambient + direct
                    + MotuAdditionalLighting(input.positionWS, normal)), 1.0h);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDHLSL
        }

        Pass
        {
            Name "ShadowCaster"
            Tags { "LightMode"="ShadowCaster" }
            ZWrite On
            ColorMask 0

            HLSLPROGRAM
            #pragma vertex ShadowVertex
            #pragma fragment ShadowFragment
            #pragma target 3.5
            #pragma multi_compile_vertex _ _CASTING_PUNCTUAL_LIGHT_SHADOW
            #pragma multi_compile_instancing

            struct ShadowVaryings
            {
                float4 positionCS : SV_POSITION;
                float2 fieldPosition : TEXCOORD0;
                half fieldSeed : TEXCOORD1;
                float2 worldXZ : TEXCOORD2;
            };

            ShadowVaryings ShadowVertex(Attributes input)
            {
                UNITY_SETUP_INSTANCE_ID(input);
                ShadowVaryings output;
                float3 positionWS = TransformObjectToWorld(input.positionOS);
                output.positionCS = MotuShadowPosition(positionWS,
                    TransformObjectToWorldNormal(input.normalOS));
                output.fieldPosition = input.fieldPosition;
                output.fieldSeed = input.colour.g;
                output.worldXZ = positionWS.xz;
                return output;
            }

            half4 ShadowFragment(ShadowVaryings input) : SV_Target
            {
                ClipCoverage(input.positionCS.xy, input.fieldPosition, input.fieldSeed, input.worldXZ);
                return 0;
            }
            ENDHLSL
        }
    }

    FallBack Off
}
