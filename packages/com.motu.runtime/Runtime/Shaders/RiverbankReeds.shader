Shader "Motu/Riverbank Reeds"
{
    Properties
    {
        _BaseColor ("Base Colour", Color) = (0.12, 0.24, 0.055, 1)
        _TipColor ("Tip Colour", Color) = (0.38, 0.48, 0.09, 1)
        _Cutoff ("Silhouette Cutoff", Range(0, 1)) = 0.46
        _ReedFadeStart ("LOD 0 Fade Start", Float) = 34
        _ReedFadeEnd ("LOD 0 Fade End", Float) = 47
        [HideInInspector] _WorldSize ("Island World Size", Float) = 2000
        [HideInInspector] _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
    }

    HLSLINCLUDE
    #include "MotuUrp.hlsl"


    fixed4 _BaseColor;
    fixed4 _TipColor;
    float _WorldSize;
    half _Cutoff;
    float _ReedFadeStart;
    float _ReedFadeEnd;
    float4 _GrassPlayerPosition;
    float4x4 _IslandWorldToLocal;

    #include "WeatherWindCommon.cginc"
    #include "CloudCommon.cginc"

    struct ReedVertexInput
    {
        float4 vertex : POSITION;
        float3 normal : NORMAL;
        float2 uv : TEXCOORD0;
        float2 root : TEXCOORD1;
        float4 data : COLOR;
        UNITY_VERTEX_INPUT_INSTANCE_ID
    };

    struct ReedVertexOutput
    {
        float4 pos : SV_POSITION;
        float3 worldPosition : TEXCOORD0;
        half3 worldNormal : TEXCOORD1;
        float2 uv : TEXCOORD2;
        half4 data : TEXCOORD3;
        float rootDistance : TEXCOORD4;
        UNITY_FOG_COORDS(5)
        UNITY_VERTEX_OUTPUT_STEREO
    };

    struct ReedShadowOutput
    {
        float4 pos : SV_POSITION;
        float2 uv : TEXCOORD0;
        half4 data : TEXCOORD1;
        float rootDistance : TEXCOORD2;
    };

    #include "ReedClumpCommon.cginc"

    ReedVertexOutput ReedVertex(ReedVertexInput input)
    {
        ReedVertexOutput output;
        UNITY_SETUP_INSTANCE_ID(input);
        UNITY_INITIALIZE_OUTPUT(ReedVertexOutput, output);
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        output.worldPosition = MotuReedWorldPosition(input.vertex.xyz, input.uv, input.root, MotuReedData(input.data));
        output.pos = UnityWorldToClipPos(output.worldPosition);
        output.worldNormal = UnityObjectToWorldNormal(input.normal);
        output.uv = input.uv;
        output.data = MotuReedData(input.data);
        output.rootDistance = distance(output.worldPosition.xz, _GrassPlayerPosition.xz);
        UNITY_TRANSFER_FOG(output, output.pos);
        return output;
    }

    void ClipReed(ReedVertexOutput input)
    {
        float silhouette = MotuReedSilhouette(input.uv, input.data);
        float fade = 1.0 - smoothstep(_ReedFadeStart, _ReedFadeEnd, input.rootDistance);
        float dither = frac(sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
        clip(silhouette - _Cutoff);
        clip(fade - dither);
    }
    ENDHLSL

    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="MotuReedCutout" "Queue"="AlphaTest" "IgnoreProjector"="True" "MotuReflection"="Reeds" }
        Cull Off
        AlphaToMask On

        Pass
        {
            Tags { "LightMode"="UniversalForwardOnly" }
            ZWrite On

            HLSLPROGRAM
            #pragma vertex ReedVertex
            #pragma fragment ReedFragment
            #pragma target 3.0
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS
            #pragma multi_compile_fragment _ _ADDITIONAL_LIGHT_SHADOWS
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #pragma multi_compile _ MOTU_REED_CLUMPS
            #pragma instancing_options forcemaxcount:128

            fixed4 ReedFragment(ReedVertexOutput input, fixed facing : VFACE) : SV_Target
            {
                ClipReed(input);
                half3 normal = normalize(input.worldNormal) * (facing >= 0 ? 1.0h : -1.0h);
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half attenuation = 1.0h;
                half diffuse = saturate(dot(normal, lightDirection));
                fixed3 albedo = lerp(_BaseColor.rgb, _TipColor.rgb, input.uv.y);
                albedo *= lerp(0.88, 1.12, input.data.y);
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                half3 ambient = max(
                    ShadeSH9(half4(normal, 1.0h))
                        * cloud.ambientTransmittance,
                    MotuDaylightAmbientFloor(0.10h));
                fixed4 result = fixed4(
                    albedo * (ambient + _LightColor0.rgb
                        * attenuation
                        * (0.18h + 0.82h * diffuse)
                        * cloud.directTransmittance
                        + MotuAdditionalLighting(input.worldPosition, normal)),
                    1.0h);
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
            #pragma target 3.0
            #pragma multi_compile_instancing
            #pragma multi_compile _ MOTU_REED_CLUMPS
            #pragma instancing_options forcemaxcount:128
            #pragma multi_compile_vertex _ _CASTING_PUNCTUAL_LIGHT_SHADOW

            ReedShadowOutput ShadowVertex(ReedVertexInput input)
            {
                ReedShadowOutput output;
                UNITY_SETUP_INSTANCE_ID(input);
                float3 worldPosition = MotuReedWorldPosition(input.vertex.xyz, input.uv, input.root, MotuReedData(input.data));
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.pos = MotuShadowPosition(worldPosition, normal);
                output.uv = input.uv;
                output.data = MotuReedData(input.data);
                output.rootDistance = distance(worldPosition.xz, _GrassPlayerPosition.xz);
                return output;
            }

            float4 ShadowFragment(ReedShadowOutput input) : SV_Target
            {
                float silhouette = MotuReedSilhouette(input.uv, input.data);
                float fade = 1.0 - smoothstep(
                    _ReedFadeStart,
                    _ReedFadeEnd,
                    input.rootDistance);
                float dither = frac(
                    sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
                clip(silhouette - _Cutoff);
                clip(fade - dither);
                return 0;
            }
            ENDHLSL
        }
        UsePass "Motu/Planar Reflection Simplified/ReflectionReeds"

    }
    FallBack Off
}
