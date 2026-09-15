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
            #include "Packages/com.motu.runtime/Runtime/Shaders/OceanWaves.cginc"
            float4 _ProbeWorldRect;
            float _ProbeFilteredWaves, _ProbeFilteredFoam, _ProbePixelFootprint;
            float _ProbeDomainWarp;
            float _ProbeCurvature;
            float _ProbeClampEnvelope;
            float _ProbeHeightResponse;
            float _ProbeHeight;
            float _ProbeFoam;
            float _ProbeFoamActivity;
            float _ProbeSurfaceTranslucency;
            float _ProbeRadius;

            float4 Fragment(v2f_img input) : SV_Target
            {
                if (_ProbeFilteredWaves > 0.5 || _ProbeFilteredFoam > 0.5)
                {
                    float2 world = _ProbeWorldRect.xy + input.uv * _ProbeWorldRect.zw;
                    if (_ProbeFilteredFoam > 0.5) return MotuOceanFoamPatches(world, _ProbePixelFootprint);
                    float3 displacement;
                    float2 slope;
                    float curvature, variance;
                    MotuEvaluateOceanWaveField(world, _ProbePixelFootprint, variance, displacement, slope, curvature);
                    return float4(displacement.y, slope, variance);
                }
                if (_ProbeDomainWarp > 0.5)
                {
                    float2 world = _ProbeWorldRect.xy + input.uv * _ProbeWorldRect.zw;
                    return float4(MotuOceanDomainWarp(MotuWindAdvectedPosition(world)), 0, 1);
                }
                if (_ProbeFoamActivity > 0.5 || _ProbeSurfaceTranslucency > 0.5)
                {
                    float3 normal;
                    float foam, crest, allowance;
                    MotuEvaluateOceanWaveNormal(_ProbeWorldRect.xy, normal, foam, crest, allowance);
                    return _ProbeSurfaceTranslucency > 0.5
                        ? float4(MotuOceanSurfaceTranslucency(_ProbeHeight, crest, _ProbeRadius), crest, 0, 1)
                        : float4(foam, allowance, crest, 1);
                }
                if (_ProbeClampEnvelope > 0.5)
                    return float4(MotuOceanClampHeightEnvelope(_ProbeWorldRect.xy), 0, 0, 1);
                if (_ProbeFoam > 0.5)
                {
                    float2 position = _ProbeWorldRect.xy + input.uv * _ProbeWorldRect.zw;
                    return float4(MotuOceanFoamUv(position), MotuOceanFoamPatches(position), 1.0);
                }
                float3 displacement;
                float2 slope;
                float curvature;
                MotuEvaluateOceanWaveField(
                    _ProbeWorldRect.xy + input.uv * _ProbeWorldRect.zw,
                    displacement,
                    slope,
                    curvature);
                if (_ProbeHeightResponse > 0.5)
                    return float4(MotuOceanHeightTranslucency(_ProbeHeight),
                        MotuOceanLargestWaveHeight(), 0, 1);
                return float4(displacement.y, slope.x, slope.y,
                    _ProbeCurvature > 0.5 ? curvature : 1.0);
            }
            ENDCG
        }
    }
}
