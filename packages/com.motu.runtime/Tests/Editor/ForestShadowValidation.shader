Shader "Hidden/Motu/Tests/Forest Shadow"
{
    Properties
    {
        _MotuForestShadowMask ("Mask", 2D) = "black" {}
        _MotuForestShadowUV ("Coordinates", Vector) = (0, 0, 0, 0)
        _MotuForestShadowStrength ("Enabled", Float) = 1
    }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            HLSLPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE
            #include "Packages/com.motu.runtime/Runtime/Shaders/MotuUrp.hlsl"
            #include "Packages/com.motu.runtime/Runtime/Shaders/ForestShadowCommon.hlsl"
            float4 _ForestTestPosition;
            float4 _ForestTestLocalPosition;
            half4 Fragment(v2f_img input) : SV_Target
            {
                return half4(MotuForestBakedShadow(_ForestTestLocalPosition.xyz),
                    MotuForestShadowFade(_ForestTestPosition.xyz),
                    MotuForestGroundShadow(_ForestTestLocalPosition.xyz,
                        _ForestTestPosition.xyz, _ForestTestPosition.w), 1);
            }
            ENDHLSL
        }
    }
}
