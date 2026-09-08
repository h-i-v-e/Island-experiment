Shader "Hidden/Motu/Tests/Ship Wave Field"
{
    Properties { _MainTex ("World positions and wave heights", 2D) = "black" {} }
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
            #include "Assets/Shaders/OceanShipWaves.cginc"
            sampler2D _MainTex;
            float4 Fragment(v2f_img input) : SV_Target
            {
                float4 sample = tex2D(_MainTex, input.uv);
                float3 field = MotuShipWaveField(sample.xy);
                float3 normal = float3(0, 1, 0);
                float foam = 0;
                MotuShipWaveShading(sample.xy, sample.z, 0, normal, foam);
                return float4(sample.w + MotuShipWaveHeight(sample.z, field, 0), field.x, field.y, foam);
            }
            ENDCG
        }
    }
}
