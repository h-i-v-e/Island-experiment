Shader "Hidden/Motu/Tests/Squall Field"
{
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            HLSLPROGRAM
            #pragma vertex vert_img
            #pragma fragment frag
            #pragma target 3.0
            #include "Packages/com.motu.runtime/Runtime/Shaders/MotuUrp.hlsl"
            float4 _FieldTestLine;
            float _FieldTestCount;
            float _FieldTestTimeOffset;
            #define MOTU_SQUALL_TIME_OFFSET _FieldTestTimeOffset
            #include "Packages/com.motu.runtime/Runtime/Shaders/WorldSquallField.cginc"
            float4 frag(v2f_img input) : SV_Target
            {
                float index = floor(input.uv.x * _FieldTestCount);
                float2 position = _FieldTestLine.xy + _FieldTestLine.zw * index;
                return float4(MotuSquallWindMultiplier(position), MotuSquallWeightAt(position),
                    MotuSquallWindMultiplierGradient(position));
            }
            ENDHLSL
        }
    }
}
