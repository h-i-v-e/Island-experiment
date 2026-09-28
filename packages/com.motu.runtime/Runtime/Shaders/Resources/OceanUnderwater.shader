Shader "Hidden/Motu/Ocean Underwater"
{
    Properties { _MainTex ("Source", 2D) = "white" {} }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Cull Off ZWrite Off ZTest Always
        HLSLINCLUDE
        #include "../MotuUrp.hlsl"
        #include "Packages/com.motu.runtime/Runtime/Shaders/OceanSurfaceGeometry.cginc"
        #include "Packages/com.unity.render-pipelines.core/Runtime/Utilities/Blit.hlsl"
        sampler2D_float _UnderwaterNear, _UnderwaterSurface;

        UNITY_DECLARE_DEPTH_TEXTURE(_CameraDepthTexture);
        float4x4 _UnderwaterInvProjection, _UnderwaterCameraToWorld, _UnderwaterVP, _UnderwaterView;
        float4 _UnderwaterOrigin, _UnderwaterParams, _UnderwaterScatter;
        float4 _AbsorptionCoefficients;
        float _AbsorptionStrength, _UnderwaterOrthographic;

        float3 ViewPosition(float2 uv, float eyeDepth)
        {
            float4 position = mul(_UnderwaterInvProjection, float4(uv * 2 - 1, .5, 1));
            position.xyz /= position.w;
            position.xy *= lerp(eyeDepth / max(-position.z, .0001), 1, _UnderwaterOrthographic);
            return float3(position.xy, -eyeDepth);
        }
        float NearHeight(Varyings input) : SV_Target
        {
            float3 nearPosition = mul(_UnderwaterCameraToWorld,
                float4(ViewPosition(input.texcoord, _UnderwaterParams.w), 1)).xyz;
            float2 samplePosition = nearPosition.xz;
            float3 displacement;
            [unroll]
            for (int iteration = 0; iteration < 8; iteration++)
            {
                MotuEvaluateOceanWaveDisplacement(samplePosition,
                    length(samplePosition - _UnderwaterOrigin.xz), displacement);
                samplePosition += (nearPosition.xz - samplePosition - displacement.xz) * .75;
            }
            float4 deckData;
            float3 surface = MotuOceanSurfacePosition(float3(samplePosition.x, _UnderwaterOrigin.y, samplePosition.y),
                length(samplePosition - _UnderwaterOrigin.xz), deckData);
            return surface.y - nearPosition.y;
        }
        struct SurfaceOutput { float4 pos : SV_POSITION; float depth : TEXCOORD0; };
        SurfaceOutput SurfaceVertex(appdata_base input)
        {
            float4 deckData;
            float3 surface = MotuOceanSurfacePosition(mul(unity_ObjectToWorld, input.vertex).xyz,
                length(input.vertex.xz), deckData);
            SurfaceOutput output;
            output.pos = mul(_UnderwaterVP, float4(surface, 1));
            output.depth = -mul(_UnderwaterView, float4(surface, 1)).z;
            return output;
        }
        float2 SurfaceFragment(SurfaceOutput input, float facing : VFACE) : SV_Target
        {
            // A back face is an exit from water; a front face is an entry.
            return float2(input.depth, facing < 0 ? 1 : -1);
        }
        float4 Composite(Varyings input) : SV_Target
        {
            float4 source = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord);
            float2 uv = input.texcoord;
            float signedDepth = tex2D(_UnderwaterNear, uv).r;
            float softness = max(_UnderwaterParams.z, fwidth(signedDepth));
            float wet = smoothstep(-softness, softness, signedDepth);
            // Above-water pixels already receive depth optics at the sea surface.
            if (wet <= .0001) return source;
            float2 surface = tex2D(_UnderwaterSurface, uv).rg;
            float rawDepth = SAMPLE_DEPTH_TEXTURE(_CameraDepthTexture, uv);
            float sceneDepth = lerp(LinearEyeDepth(rawDepth),
                lerp(_ProjectionParams.y, _ProjectionParams.z,
                    #if defined(UNITY_REVERSED_Z)
                    1 - rawDepth
                    #else
                    rawDepth
                    #endif
                ), _UnderwaterOrthographic);
            // Stop absorption at the first exit, or at an opaque foreground object.
            float endDepth = sceneDepth;
            if (surface.y > 0) endDepth = min(endDepth, surface.x);
            float3 nearPosition = ViewPosition(uv, _UnderwaterParams.w);
            float3 endPosition = ViewPosition(uv, max(endDepth, _UnderwaterParams.w));
            float distance = min(length(endPosition - nearPosition), _UnderwaterParams.y);
            float3 extinction = max(_AbsorptionCoefficients.rgb, 0) * max(_AbsorptionStrength, 0)
                + _UnderwaterParams.x;
            float3 transmission = exp(-extinction * distance);
            float3 water = source.rgb * transmission + _UnderwaterScatter.rgb * (1 - transmission);
            return float4(lerp(source.rgb, water, wet), source.a);
        }
        ENDHLSL
        Pass
        {
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment NearHeight
            ENDHLSL
        }
        Pass
        {
            ZWrite On ZTest LEqual
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex SurfaceVertex
            #pragma fragment SurfaceFragment
            ENDHLSL
        }
        Pass
        {
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment Composite
            ENDHLSL
        }
    }
}
