Shader "Motu/TownBuilding"
{
    Properties
    {
        [MainTexture] _BaseMap("Albedo", 2D) = "white" {}
        [MainColor] _BaseColor("Colour", Color) = (1,1,1,1)
        _NormalMap("Normal", 2D) = "bump" {}
        _Masks("Occlusion Roughness Metal", 2D) = "white" {}
        [HideInInspector] _UseMasks("Use Native Masks", Float) = 0
        _Roughness("Roughness", Range(0,1)) = 0.75
        _Metallic("Metallic", Range(0,1)) = 0
        _Cutoff("Alpha Cutoff", Range(0,1)) = 0.001
        [HideInInspector] _UseGrubCoordinates("Native Support Coordinates", Float) = 0
        [HideInInspector] _Grub0("Weathering Strengths", Vector) = (0,0,1.5,1.25)
        [HideInInspector] _Grub1("Weathering Dirt", Vector) = (0.075,0.055,0.035,1)
        [HideInInspector] _Grub2("Weathering Moss", Vector) = (0.07,0.115,0.025,1)
        [HideInInspector] _Grub3("Weathering Offset", Vector) = (0,0,0,0)
        [HideInInspector] _Grub4("Weathering Fine Noise", Vector) = (0.18,0.35,0,0)
        [HideInInspector] _SrcBlend("Source Blend", Float) = 1
        [HideInInspector] _DstBlend("Destination Blend", Float) = 0
        [HideInInspector] _ZWrite("Depth Write", Float) = 1
    }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="Opaque" }
        Pass
        {
            Name "ForwardLit"
            Tags { "LightMode"="UniversalForward" }
            Blend [_SrcBlend] [_DstBlend]
            ZWrite [_ZWrite]
            Cull Off
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma multi_compile_instancing
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Lighting.hlsl"
            TEXTURE2D(_BaseMap); SAMPLER(sampler_BaseMap);
            TEXTURE2D(_NormalMap); SAMPLER(sampler_NormalMap);
            TEXTURE2D(_Masks); SAMPLER(sampler_Masks);
            CBUFFER_START(UnityPerMaterial)
                float4 _BaseMap_ST, _BaseColor;
                float _Cutoff, _UseGrubCoordinates, _UseMasks, _Roughness, _Metallic;
                float4 _Grub0, _Grub1, _Grub2, _Grub3, _Grub4;
            CBUFFER_END
            struct Attributes { float4 positionOS:POSITION; float3 normalOS:NORMAL; float2 uv:TEXCOORD0; float3 grub:TEXCOORD1; UNITY_VERTEX_INPUT_INSTANCE_ID };
            struct Varyings { float4 positionCS:SV_POSITION; float3 positionWS:TEXCOORD0; float3 normalWS:TEXCOORD1; float2 uv:TEXCOORD2; float3 grub:TEXCOORD3; UNITY_VERTEX_INPUT_INSTANCE_ID UNITY_VERTEX_OUTPUT_STEREO };
            Varyings Vert(Attributes input)
            {
                Varyings output = (Varyings)0;
                UNITY_SETUP_INSTANCE_ID(input);
                UNITY_TRANSFER_INSTANCE_ID(input, output);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                output.positionWS = TransformObjectToWorld(input.positionOS.xyz);
                output.positionCS = TransformWorldToHClip(output.positionWS);
                output.normalWS = TransformObjectToWorldNormal(input.normalOS);
                output.uv = TRANSFORM_TEX(input.uv, _BaseMap);
                output.grub = lerp(input.positionOS.xyz, input.grub, _UseGrubCoordinates);
                return output;
            }
            float GrubHash(int3 cell)
            {
                uint3 p = asuint(cell);
                uint h = p.x * 1597334677u ^ p.y * 3812015801u ^ p.z * 2798796415u;
                h = (h ^ (h >> 16u)) * 2146121005u;
                h = (h ^ (h >> 15u)) * 2221713035u;
                return float((h ^ (h >> 16u)) & 16777215u) / 16777215.0;
            }
            float GrubNoise(float3 p)
            {
                int3 cell = int3(floor(p));
                float3 f = frac(p), t = f*f*(3-2*f);
                float a = lerp(GrubHash(cell), GrubHash(cell+int3(1,0,0)), t.x);
                float b = lerp(GrubHash(cell+int3(0,1,0)), GrubHash(cell+int3(1,1,0)), t.x);
                float c = lerp(GrubHash(cell+int3(0,0,1)), GrubHash(cell+int3(1,0,1)), t.x);
                float d = lerp(GrubHash(cell+int3(0,1,1)), GrubHash(cell+int3(1,1,1)), t.x);
                return lerp(lerp(a,b,t.y), lerp(c,d,t.y), t.z);
            }
            float GrubLayers(float3 p)
            {
                float broad = GrubNoise(p / _Grub0.w + _Grub3.xyz);
                float fine = GrubNoise(p / _Grub4.x + _Grub3.xyz + float3(17.3,4.2,11.1));
                return lerp(smoothstep(0.25,0.75,broad), smoothstep(0.25,0.75,fine), _Grub4.y);
            }
            half3 Weathered(float3 position, float3 normal, half3 albedo)
            {
                float influence = 2.0 / (1.0 + exp2(position.y / _Grub0.z));
                position.z = -position.z;
                float dirt = saturate(_Grub0.x * saturate(influence * GrubLayers(position)));
                float mossPatch = smoothstep(0.4,0.75,GrubLayers(position+float3(21.4,8.7,14.2)));
                float moss = saturate(_Grub0.y * saturate(influence*mossPatch) * smoothstep(0.35,0.85,normal.y));
                return lerp(lerp(albedo, _Grub1.rgb, dirt), _Grub2.rgb, moss);
            }
            half4 Frag(Varyings input, bool frontFace:SV_IsFrontFace):SV_Target
            {
                UNITY_SETUP_INSTANCE_ID(input);
                half4 albedo = SAMPLE_TEXTURE2D(_BaseMap, sampler_BaseMap, input.uv) * _BaseColor;
                clip(albedo.a - _Cutoff);
                half3 masks = lerp(half3(1, _Roughness, _Metallic), SAMPLE_TEXTURE2D(_Masks, sampler_Masks, input.uv).rgb, _UseMasks);
                float3 normal = normalize(input.normalWS) * (frontFace ? 1 : -1);
                float3 dp1 = ddx(input.positionWS), dp2 = ddy(input.positionWS);
                float2 duv1 = ddx(input.uv), duv2 = ddy(input.uv);
                float3 t = cross(dp2, normal) * duv1.x + cross(normal, dp1) * duv2.x;
                float3 b = cross(dp2, normal) * duv1.y + cross(normal, dp1) * duv2.y;
                float inverseScale = rsqrt(max(max(dot(t,t), dot(b,b)), 1e-10));
                // Native maps store ordinary RGB tangent-space normals, not Unity DXT5nm.
                float3 tangentNormal = SAMPLE_TEXTURE2D(_NormalMap, sampler_NormalMap, input.uv).xyz * 2 - 1;
                normal = normalize(t * inverseScale * tangentNormal.x + b * inverseScale * tangentNormal.y + normal * tangentNormal.z);
                InputData lighting = (InputData)0;
                lighting.positionWS = input.positionWS;
                lighting.normalWS = normal;
                lighting.viewDirectionWS = GetWorldSpaceNormalizeViewDir(input.positionWS);
                lighting.shadowCoord = TransformWorldToShadowCoord(input.positionWS);
                lighting.bakedGI = SampleSH(normal);
                lighting.normalizedScreenSpaceUV = GetNormalizedScreenSpaceUV(input.positionCS);
                lighting.shadowMask = half4(1,1,1,1);
                SurfaceData surface = (SurfaceData)0;
                surface.albedo = Weathered(input.grub, normal, albedo.rgb);
                surface.metallic = masks.b;
                surface.smoothness = 1 - masks.g;
                surface.normalTS = half3(0,0,1);
                surface.occlusion = masks.r;
                surface.alpha = albedo.a;
                return UniversalFragmentPBR(lighting, surface);
            }
            ENDHLSL
        }
        Pass
        {
            Name "ShadowCaster"
            Tags { "LightMode"="ShadowCaster" }
            ZWrite On ZTest LEqual ColorMask 0 Cull Off
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex ShadowVert
            #pragma fragment ShadowFrag
            #pragma multi_compile_instancing
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            TEXTURE2D(_BaseMap); SAMPLER(sampler_BaseMap);
            CBUFFER_START(UnityPerMaterial)
                float4 _BaseMap_ST, _BaseColor;
                float _Cutoff, _UseGrubCoordinates, _UseMasks, _Roughness, _Metallic;
                float4 _Grub0, _Grub1, _Grub2, _Grub3, _Grub4;
            CBUFFER_END
            struct Attributes { float4 positionOS:POSITION; float2 uv:TEXCOORD0; UNITY_VERTEX_INPUT_INSTANCE_ID };
            struct Varyings { float4 positionCS:SV_POSITION; float2 uv:TEXCOORD0; UNITY_VERTEX_INPUT_INSTANCE_ID };
            Varyings ShadowVert(Attributes input)
            {
                Varyings output = (Varyings)0;
                UNITY_SETUP_INSTANCE_ID(input); UNITY_TRANSFER_INSTANCE_ID(input, output);
                output.positionCS = TransformObjectToHClip(input.positionOS.xyz);
                output.uv = TRANSFORM_TEX(input.uv, _BaseMap);
                return output;
            }
            half4 ShadowFrag(Varyings input):SV_Target
            {
                UNITY_SETUP_INSTANCE_ID(input);
                clip(SAMPLE_TEXTURE2D(_BaseMap, sampler_BaseMap, input.uv).a * _BaseColor.a - _Cutoff);
                return 0;
            }
            ENDHLSL
        }
    }
}
