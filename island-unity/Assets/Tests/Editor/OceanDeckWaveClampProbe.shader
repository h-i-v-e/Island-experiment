Shader "Hidden/Motu/Tests/Deck Wave Clamp"
{
    Properties { _MainTex ("Probe positions and heights", 2D) = "black" {} }
    SubShader
    {
        Pass
        {
            ZTest Always Cull Off ZWrite Off
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.5
            #include "UnityCG.cginc"
            #include "Assets/Shaders/OceanDeckWaveClamp.cginc"
            sampler2D _MainTex;
            float4 Fragment(v2f_img input) : SV_Target
            {
                float4 sample = tex2D(_MainTex, input.uv);
                float weight = MotuDeckWaveClampWeight(sample.xy);
                return float4(sample.w + MotuClampDeckWaveHeight(sample.z, weight), weight, 0, 1);
            }
            ENDCG
        }
    }
}
