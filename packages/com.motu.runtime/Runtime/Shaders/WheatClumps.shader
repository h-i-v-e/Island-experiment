Shader "Motu/Wheat Clumps"
{
    Properties
    {
        _StrawBase ("Straw Base", Color) = (0.43, 0.30, 0.12, 1)
        _StrawTip ("Straw Tip", Color) = (0.75, 0.57, 0.26, 1)
        _GrainColour ("Grain Colour", Color) = (0.89, 0.67, 0.30, 1)
        [PerRendererData] _WheatInstanceData ("Wheat Variation and Visibility", Vector) = (0.5, 1, 0, 0)
        [HideInInspector] _WheatDetailBlend ("Detail Fade", Vector) = (0, 0, 0, 0)
    }

    HLSLINCLUDE
    #include "MotuUrp.hlsl"
    #include "WeatherWindCommon.cginc"
    #include "WheatDetailBlend.hlsl"

    CBUFFER_START(UnityPerMaterial)
        half4 _StrawBase;
        half4 _StrawTip;
        half4 _GrainColour;
        float4 _WheatDetailBlend;
    CBUFFER_END

    UNITY_INSTANCING_BUFFER_START(Wheat)
        UNITY_DEFINE_INSTANCED_PROP(float4, _WheatInstanceData)
    UNITY_INSTANCING_BUFFER_END(Wheat)

    struct WheatInput
    {
        float3 positionOS : POSITION;
        float3 normalOS : NORMAL;
        float2 uv : TEXCOORD0;
        UNITY_VERTEX_INPUT_INSTANCE_ID
    };

    struct WheatVertex
    {
        float4 positionCS : SV_POSITION;
        float3 cameraRelativePosition : TEXCOORD0;
        half3 normalWS : TEXCOORD1;
        float2 uv : TEXCOORD2;
        half2 instanceData : TEXCOORD3;
        UNITY_FOG_COORDS(4)
        UNITY_VERTEX_OUTPUT_STEREO
    };

    struct WheatDepthVertex
    {
        float4 positionCS : SV_POSITION;
        float2 uv : TEXCOORD0;
        half fade : TEXCOORD1;
        UNITY_VERTEX_OUTPUT_STEREO
    };

    half2 WheatData()
    {
        half2 data = saturate(UNITY_ACCESS_INSTANCED_PROP(Wheat, _WheatInstanceData).xy);
        float3 rootWS = TransformObjectToWorld(float3(0, 0, 0));
        data.y *= MotuWheatDetailCoverage(rootWS.xz, _WheatDetailBlend);
        return data;
    }

    float3 WheatWindPosition(float3 positionOS, float variation)
    {
        float3 rootWS = TransformObjectToWorld(float3(0, 0, 0));
        float3 positionWS = TransformObjectToWorld(positionOS);
        float3 wind = MotuWindSample(rootWS.xz);
        float height = saturate(positionOS.y);
        // Keep the roots fixed. The ears and the tips of the flag leaves
        // take most of the bend, driven by the same gusts as the reeds.
        float bend = height * height * lerp(0.75, 1.25, variation);
        positionWS.xz += wind.xz * (MotuWindDisplacementStrength() * wind.y * bend);
        return positionWS;
    }

    WheatVertex WheatVert(WheatInput input)
    {
        UNITY_SETUP_INSTANCE_ID(input);
        WheatVertex output = (WheatVertex)0;
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        output.instanceData = WheatData();
        float3 positionWS = WheatWindPosition(input.positionOS, output.instanceData.x);
        output.positionCS = TransformWorldToHClip(positionWS);
        output.cameraRelativePosition = MotuCameraRelativePosition(positionWS);
        output.normalWS = TransformObjectToWorldNormal(input.normalOS);
        output.uv = input.uv;
        UNITY_TRANSFER_FOG(output, output.positionCS);
        return output;
    }

    float WheatLeaf(float2 uv, float start, float length, float reach)
    {
        float phase = (uv.y - start) / length;
        float along = saturate(phase);
        float width = 0.22 * sin(along * PI);
        float centre = reach * along * (1.60 - 0.60 * along);
        return min(width - abs(uv.x - centre), min(phase, 1.0 - phase));
    }

    void WheatClipFade(float2 pixel, float fade)
    {
        if (fade < 1.0)
            clip(fade - max(InterleavedGradientNoise(pixel, 0), 1.0 / 256.0));
    }

    // Two staggered columns of spikelets sit around a thin rachis. A pointed
    // awn continues from each grain to give the ears their bristled outline.
    float3 WheatSilhouette(float2 uv)
    {
        float across = abs(uv.x);
        float stalk = min(lerp(0.053, 0.022, saturate(uv.y)) - across, (0.974 - uv.y) * 5.0);
        float lowerLeaf = WheatLeaf(uv, 0.19, 0.27, -0.91);
        float flagLeaf = WheatLeaf(uv, 0.56, 0.21, 0.79);

        float ear = (uv.y - 0.79) / 0.18;
        float phase = frac(ear * 4.6 - 0.50 * step(0.0, uv.x));
        float taper = lerp(0.30, 1.0, sin(saturate(ear) * PI));
        float seedOffset = (phase - 0.39) * 2.65;
        float seedShape = saturate(1.0 - seedOffset * seedOffset);
        float seedCentre = (0.085 + 0.115 * phase) * taper;
        float grain = 0.185 * seedShape * taper - abs(across - seedCentre);
        grain = min(grain, min(ear, 1.0 - ear));
        float awnCentre = 0.21 * taper + 0.21 * phase;
        float awn = min(0.020 * (1.0 - phase) - abs(across - awnCentre), phase - 0.32);
        awn = min(awn, min(ear, 1.15 - ear));

        float edgeWidth = max(fwidth(uv.x), 0.001);
        float stem = saturate(0.5 + max(stalk, max(lowerLeaf, flagLeaf)) / edgeWidth);
        float kernels = saturate(0.5 + grain / edgeWidth);
        float whiskers = saturate(0.5 + awn / edgeWidth);
        return float3(max(stem, max(kernels, whiskers)), kernels, whiskers);
    }

    half4 WheatShade(WheatVertex input, half facingSign, half shadow)
    {
        float3 silhouette = WheatSilhouette(input.uv);
        clip(silhouette.x - 0.5);
        WheatClipFade(input.positionCS.xy, input.instanceData.y);
        half3 straw = lerp(_StrawBase.rgb, _StrawTip.rgb, saturate(input.uv.y * 1.5));
        half3 grain = _GrainColour.rgb * lerp(0.87h, 1.05h, silhouette.y);
        half3 albedo = lerp(straw, grain, max(silhouette.y, silhouette.z));
        albedo *= lerp(0.86h, 1.10h, input.instanceData.x);
        float3 positionWS = input.cameraRelativePosition + GetCameraPositionWS();
        // A little wrapped light on the thin straw avoids flat dark stripes
        // as the two sides of a stalk turn across the sun.
        half3 normal = normalize(input.normalWS * facingSign + half3(0, 0.35h, 0));
        MotuCloudLighting cloud = MotuCloudSurfaceLighting(positionWS);
        half3 ambient = max(SampleSH(normal) * cloud.ambientTransmittance, MotuDaylightAmbientFloor(0.13h));
        half diffuse = 0.28h + 0.72h * saturate(dot(normal, _MainLightPosition.xyz));
        half3 light = _MainLightColor.rgb * shadow * diffuse * cloud.directTransmittance;
        half3 additional = MotuAdditionalLighting(positionWS, normal);
        return half4(albedo * (ambient + light + additional), silhouette.x);
    }

    WheatDepthVertex WheatShadowVert(WheatInput input)
    {
        UNITY_SETUP_INSTANCE_ID(input);
        WheatDepthVertex output = (WheatDepthVertex)0;
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        half2 data = WheatData();
        float3 positionWS = WheatWindPosition(input.positionOS, data.x);
        output.positionCS = MotuShadowPosition(positionWS, TransformObjectToWorldNormal(input.normalOS));
        output.uv = input.uv;
        output.fade = data.y;
        return output;
    }

    WheatDepthVertex WheatDepthVert(WheatInput input)
    {
        UNITY_SETUP_INSTANCE_ID(input);
        WheatDepthVertex output = (WheatDepthVertex)0;
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        half2 data = WheatData();
        output.positionCS = TransformWorldToHClip(WheatWindPosition(input.positionOS, data.x));
        output.uv = input.uv;
        output.fade = data.y;
        return output;
    }

    float WheatDepthFrag(WheatDepthVertex input) : SV_Target
    {
        clip(WheatSilhouette(input.uv).x - 0.5);
        WheatClipFade(input.positionCS.xy, input.fade);
        return input.positionCS.z;
    }
    ENDHLSL

    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="TransparentCutout" "Queue"="AlphaTest" }
        Cull Off
        ZWrite On

        Pass
        {
            Name "Forward"
            Tags { "LightMode"="UniversalForwardOnly" }
            AlphaToMask On

            HLSLPROGRAM
            #pragma target 3.0
            #pragma vertex WheatVert
            #pragma fragment WheatFrag
            #pragma multi_compile_instancing
            #pragma instancing_options forcemaxcount:128
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS
            #pragma multi_compile_fragment _ _ADDITIONAL_LIGHT_SHADOWS
            #pragma multi_compile_fog

            half4 WheatFrag(WheatVertex input, FRONT_FACE_TYPE face : FRONT_FACE_SEMANTIC) : SV_Target
            {
                UNITY_SETUP_STEREO_EYE_INDEX_POST_VERTEX(input);
                half shadow = MotuMainLightShadowAttenuation(input.cameraRelativePosition);
                half4 result = WheatShade(input, IS_FRONT_VFACE(face, 1.0h, -1.0h), shadow);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDHLSL
        }

        Pass
        {
            Name "ShadowCaster"
            Tags { "LightMode"="ShadowCaster" }
            ColorMask 0

            HLSLPROGRAM
            #pragma target 3.0
            #pragma vertex WheatShadowVert
            #pragma fragment WheatDepthFrag
            #pragma multi_compile_instancing
            #pragma instancing_options forcemaxcount:128
            #pragma multi_compile_vertex _ _CASTING_PUNCTUAL_LIGHT_SHADOW
            ENDHLSL
        }

        Pass
        {
            Name "DepthOnly"
            Tags { "LightMode"="DepthOnly" }
            ColorMask R

            HLSLPROGRAM
            #pragma target 3.0
            #pragma vertex WheatDepthVert
            #pragma fragment WheatDepthFrag
            #pragma multi_compile_instancing
            #pragma instancing_options forcemaxcount:128
            ENDHLSL
        }

        Pass
        {
            Name "Reflection"
            Tags { "LightMode"="MotuReflection" }
            AlphaToMask On

            HLSLPROGRAM
            #pragma target 3.0
            #pragma vertex WheatVert
            #pragma fragment WheatReflectionFrag
            #pragma multi_compile_instancing
            #pragma instancing_options forcemaxcount:128

            half4 WheatReflectionFrag(WheatVertex input, FRONT_FACE_TYPE face : FRONT_FACE_SEMANTIC) : SV_Target
            {
                UNITY_SETUP_STEREO_EYE_INDEX_POST_VERTEX(input);
                return WheatShade(input, IS_FRONT_VFACE(face, 1.0h, -1.0h), 1.0h);
            }
            ENDHLSL
        }
    }
    FallBack Off
}
