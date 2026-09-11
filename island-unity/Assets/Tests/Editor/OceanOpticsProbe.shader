Shader "Hidden/Motu/Tests/Ocean Optics"
{
    Properties { _MainTex ("Input", 2D) = "black" {} }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Frag
            #pragma target 3.5
            #include "Assets/Shaders/WaterCommon.cginc"
            #include "Assets/Shaders/OceanWaves.cginc"
            #include "Assets/Shaders/OceanOptics.cginc"
            float _ProbeMode, _ProbeDistance, _ProbeSpan;
            float4 _ProbeView, _ProbeLight;
            float4 Frag(v2f_img input) : SV_Target
            {
                if (_ProbeMode < .5) return float4(MotuWaterTransmission(_ProbeDistance), MotuWaterFresnel(input.uv.x));
                if (_ProbeMode < 1.5)
                {
                    float roughness;
                    float3 normal = MotuOceanRippleNormal(float3(input.uv.x, 0, input.uv.y) * _ProbeSpan, float3(0, 1, 0), roughness);
                    return float4(normal, roughness);
                }
                if (_ProbeMode < 2.5) return MotuWaterSunSpecular(float3(0, 1, 0), normalize(_ProbeView.xyz), normalize(_ProbeLight.xyz), _SurfaceRoughness);
                return MotuWaterRefractionValidity(_ProbeDistance, 10);
            }
            ENDCG
        }
    }
}
