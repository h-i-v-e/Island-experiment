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
            #include "Packages/com.motu.runtime/Runtime/Shaders/WaterCommon.cginc"
            #include "Packages/com.motu.runtime/Runtime/Shaders/OceanWaves.cginc"
            #include "Packages/com.motu.runtime/Runtime/Shaders/OceanOptics.cginc"
            #include "Packages/com.motu.runtime/Runtime/Shaders/OceanShipWaves.cginc"
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
                if (_ProbeMode < 3.5) return MotuWaterRefractionValidity(_ProbeDistance, 10);
                if (_ProbeMode < 4.5) return MotuOceanCoastalTint(_ProbeView.xz);
                if (_ProbeMode > 6.5)
                {
                    MotuCloudLighting cloud;
                    cloud.directTransmittance = 1;
                    cloud.ambientTransmittance = 1;
                    return float4(MotuWaterShadeOptics(0, 0, 0, float3(0, 1, 0),
                        float3(0, 0, 1), _ProbeView.xyz, _ProbeLight.xy, .2, 1, cloud, 1), 1);
                }
                if (_ProbeMode > 5.5)
                    return MotuOceanSurfaceFoam(_ProbeView.x, _ProbeView.y, _ProbeView.z, _ProbeView.w);
                float3 shipField = MotuShipWaveField(_ProbeView.xz);
                return float4(MotuOceanHistoryFoam(_ProbeView.xz), shipField.z, shipField.xy);
            }
            ENDCG
        }
    }
}
