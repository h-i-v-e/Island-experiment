Shader "Hidden/Motu/Ocean Wave Transition Probe"
{
    SubShader
    {
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.5
            #include "UnityCG.cginc"
            #include "../Shaders/OceanWaves.cginc"
            float4 _ProbeWorldRect;

            float4 Fragment(v2f_img input) : SV_Target
            {
                float3 displacement;
                float2 slope;
                MotuEvaluateOceanWaveField(
                    _ProbeWorldRect.xy + input.uv * _ProbeWorldRect.zw,
                    displacement,
                    slope);
                return float4(displacement.y, slope.x, slope.y, 1.0);
            }
            ENDCG
        }
    }
}
