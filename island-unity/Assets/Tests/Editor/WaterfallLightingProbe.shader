Shader "Hidden/Motu/Tests/Waterfall Lighting"
{
    SubShader
    {
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            HLSLPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.5
            #include "Packages/com.motu.runtime/Runtime/Shaders/MotuUrp.hlsl"
            #include "Packages/com.motu.runtime/Runtime/Shaders/WaterfallSprayLighting.cginc"
            float _ProbeShadow;
            float4 Fragment(v2f_img input) : SV_Target
            {
                return float4(MotuWaterfallSprayLighting(float3(1, 1, 1), float3(0, 0, 0), _ProbeShadow), 1);
            }
            ENDHLSL
        }
    }
}
