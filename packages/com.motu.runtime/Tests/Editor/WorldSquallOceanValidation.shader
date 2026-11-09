Shader "Hidden/Motu/Tests/Squall Ocean"
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
            #include "Packages/com.motu.runtime/Runtime/Shaders/OceanWaves.cginc"
            float4 _SquallTestPosition;
            float4 frag(v2f_img input) : SV_Target
            {
                if (_SquallTestPosition.w > 1.5)
                {
                    float2 samplePosition = _SquallTestPosition.xy + (input.uv - .5) * _SquallTestPosition.z;
                    float variance, curvature, maximumHeight; float3 waves; float2 slope;
                    MotuEvaluateOceanWaveField(samplePosition, 0, variance, waves, slope, curvature, maximumHeight);
                    float3 normal; float foam, crest, allowance;
                    MotuEvaluateOceanWaveNormal(samplePosition, normal, foam, crest, allowance);
                    return float4(waves.y, length(slope), foam, maximumHeight);
                }
                float3 displacement;
                if (_SquallTestPosition.w > .5)
                {
                    MotuEvaluateOceanWaveDisplacement(_SquallTestPosition.xy, _SquallTestPosition.z, displacement);
                    return float4(displacement.y, 0, 0, 0);
                }
                float2 derivative;
                float variance, curvature, maximumHeight;
                MotuEvaluateOceanWaveField(_SquallTestPosition.xy, 0, variance,
                    displacement, derivative, curvature, maximumHeight);
                float depthScale = MotuOceanDepthWaveScale(MotuOceanCoastalData(_SquallTestPosition.xy), _SquallTestPosition.xy);
                return float4(displacement.y, derivative, maximumHeight) * depthScale;
            }
            ENDHLSL
        }
    }
}
