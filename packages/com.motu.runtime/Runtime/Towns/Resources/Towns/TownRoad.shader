Shader "Motu/TownRoad"
{
    Properties
    {
        [MainColor] _BaseColor("Fallback Road Colour", Color) = (0.40, 0.29, 0.18, 1)
        [NoScaleOffset] _TerrainAlbedoArray("Island Terrain Albedos", 2DArray) = "" {}
        [HideInInspector] _RoadTileMetres("Road Repeat (metres)", Vector) = (1, 1, 0, 0)
        [HideInInspector] _UseTerrainAlbedo("Use Island Terrain Albedos", Float) = 0
    }

    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="Opaque" "Queue"="Geometry+1" }
        Offset -1, -1

        HLSLINCLUDE
        #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
        #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Lighting.hlsl"

        TEXTURE2D_ARRAY(_TerrainAlbedoArray);
        SAMPLER(sampler_TerrainAlbedoArray);
        CBUFFER_START(UnityPerMaterial)
            half4 _BaseColor;
            float4 _RoadTileMetres;
            float _UseTerrainAlbedo;
        CBUFFER_END

        struct Attributes
        {
            float4 positionOS : POSITION;
            float3 normalOS : NORMAL;
            float2 uv : TEXCOORD0;
            UNITY_VERTEX_INPUT_INSTANCE_ID
        };

        struct Varyings
        {
            float4 positionCS : SV_POSITION;
            float3 positionWS : TEXCOORD0;
            half3 normalWS : TEXCOORD1;
            float2 metres : TEXCOORD2;
            half fog : TEXCOORD3;
            UNITY_VERTEX_OUTPUT_STEREO
        };

        Varyings Vert(Attributes input)
        {
            UNITY_SETUP_INSTANCE_ID(input);
            Varyings output = (Varyings)0;
            UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
            output.positionWS = TransformObjectToWorld(input.positionOS.xyz);
            output.positionCS = TransformWorldToHClip(output.positionWS);
            output.normalWS = TransformObjectToWorldNormal(input.normalOS);
            output.metres = input.uv;
            output.fog = ComputeFogFactor(output.positionCS.z);
            return output;
        }
        ENDHLSL

        Pass
        {
            Name "ForwardLit"
            Tags { "LightMode"="UniversalForward" }
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma multi_compile_fog
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS

            half4 Frag(Varyings input) : SV_Target
            {
                UNITY_SETUP_STEREO_EYE_INDEX_POST_VERTEX(input);
                half3 albedo = _BaseColor.rgb;
                if (_UseTerrainAlbedo > 0.5)
                {
                    // Road layer from the terrain texture arrays.
                    albedo = SAMPLE_TEXTURE2D_ARRAY(_TerrainAlbedoArray, sampler_TerrainAlbedoArray,
                        input.metres / _RoadTileMetres.xy, 6).rgb;
                }

                InputData lighting = (InputData)0;
                lighting.positionWS = input.positionWS;
                lighting.normalWS = NormalizeNormalPerPixel(input.normalWS);
                lighting.viewDirectionWS = GetWorldSpaceNormalizeViewDir(input.positionWS);
                lighting.shadowCoord = TransformWorldToShadowCoord(input.positionWS);
                lighting.bakedGI = SampleSH(lighting.normalWS);
                lighting.normalizedScreenSpaceUV = GetNormalizedScreenSpaceUV(input.positionCS);
                lighting.shadowMask = half4(1, 1, 1, 1);

                SurfaceData surface = (SurfaceData)0;
                surface.albedo = albedo;
                surface.smoothness = 0.10;
                surface.normalTS = half3(0, 0, 1);
                surface.occlusion = 1;
                surface.alpha = 1;
                half4 colour = UniversalFragmentPBR(lighting, surface);
                return half4(MixFog(colour.rgb, input.fog), colour.a);
            }
            ENDHLSL
        }

        Pass
        {
            Name "DepthOnly"
            Tags { "LightMode"="DepthOnly" }
            ZWrite On
            ColorMask R
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment Depth
            half Depth(Varyings input) : SV_Target { return input.positionCS.z; }
            ENDHLSL
        }
    }
}
