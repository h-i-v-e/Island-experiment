Shader "Hidden/Motu/Ocean Shore Distance Probe"
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
            float _ProbeFullSurface;
            float _ProbeBreakerShape;
            float _ProbeCoastDistance;
            float _ProbeWaterDepth;
            float4 Fragment(v2f_img input) : SV_Target
            {
                float3 displacement;
                if (_ProbeBreakerShape > 0.5)
                {
                    float height;
                    float derivative;
                    float breakerFoam;
                    MotuEvaluateOnshoreBreakerShape(input.uv.x * 6.28318530718,
                        _ProbeCoastDistance, _OnshoreWaveParameters.x, _ProbeWaterDepth,
                        height, derivative, breakerFoam);
                    return float4(height, derivative, breakerFoam, 1.0);
                }
                if (_ProbeFullSurface > 0.5)
                {
                    MotuEvaluateOceanWaveDisplacement(float2(0.0, 0.0), 0.0, displacement);
                    float3 normal;
                    float whitecap;
                    MotuEvaluateOceanWaveNormal(float2(0.0, 0.0), normal, whitecap);
                    if (_ProbeFullSurface > 1.5)
                        return float4(displacement.y, whitecap, normal.x, normal.y);
                    return float4(displacement.y, normal);
                }
                float2 slope;
                float influence;
                float foam;
                MotuEvaluateOnshoreWaveField(
                    input.uv, displacement, slope, influence, foam);
                return float4(displacement.y, slope, influence);
            }
            ENDCG
        }
    }
}
