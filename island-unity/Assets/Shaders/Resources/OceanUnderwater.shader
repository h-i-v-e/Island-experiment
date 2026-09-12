Shader "Hidden/Motu/Ocean Underwater"
{
    Properties { _MainTex ("Source", 2D) = "white" {} }
    SubShader
    {
        Cull Off ZWrite Off ZTest Always
        CGINCLUDE
        #include "UnityCG.cginc"
        #include "Assets/Shaders/OceanSurfaceGeometry.cginc"
        sampler2D _MainTex;
        sampler2D_float _UnderwaterNear, _UnderwaterSurface;
        float4 _MainTex_TexelSize;
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
        float NearHeight(v2f_img input) : SV_Target
        {
            float3 nearPosition = mul(_UnderwaterCameraToWorld,
                float4(ViewPosition(input.uv, _UnderwaterParams.w), 1)).xyz;
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
        float4 Composite(v2f_img input) : SV_Target
        {
            float4 source = tex2D(_MainTex, input.uv);
            float2 uv = input.uv;
            #if UNITY_UV_STARTS_AT_TOP
            if (_MainTex_TexelSize.y < 0) uv.y = 1 - uv.y;
            #endif
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
        ENDCG
        Pass
        {
            CGPROGRAM
            #pragma target 3.5
            #pragma vertex vert_img
            #pragma fragment NearHeight
            ENDCG
        }
        Pass
        {
            ZWrite On ZTest LEqual
            CGPROGRAM
            #pragma target 3.5
            #pragma vertex SurfaceVertex
            #pragma fragment SurfaceFragment
            ENDCG
        }
        Pass
        {
            CGPROGRAM
            #pragma target 3.5
            #pragma vertex vert_img
            #pragma fragment Composite
            ENDCG
        }
    }
}
