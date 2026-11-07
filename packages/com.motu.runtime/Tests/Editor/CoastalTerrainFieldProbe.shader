Shader "Hidden/Motu/Tests/Coastal Terrain Field"
{
    Properties
    {
        _MotuTerrainFieldOffset ("Logical offset", Vector) = (0,0,0,0)
        _MotuTerrainSeam ("Coastal half width and collar", Vector) = (0,0,0,0)
        _ProbeBakedMap ("Probe baked map weight", Float) = 0
        _ProbeLocalPosition ("Local sample", Vector) = (0,0,0,0)
        _CliffNoisePeriod ("Noise period", Float) = 160
        [NoScaleOffset] _CliffNoise3D ("Noise", 3D) = "gray" {}
        _TerrainLayerWorldSizesB ("Beach and stones metres", Vector) = (3,2,0,0)
    }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            HLSLPROGRAM
            #pragma target 3.5
            #pragma vertex vert_img
            #pragma fragment frag
            #include "Packages/com.motu.runtime/Runtime/Shaders/MotuUrp.hlsl"
            #include "Packages/com.motu.runtime/Runtime/Shaders/TerrainCoverageCommon.cginc"
            float4 _ProbeLocalPosition;
            float _ProbeBakedMap;
            float4 frag(v2f_img input) : SV_Target
            {
                if (_ProbeBakedMap > 0.5)
                    return MotuTerrainBakedMapWeight(_ProbeLocalPosition.xyz).xxxx;
                return float4(MotuSampleTerrainNoise(_ProbeLocalPosition.xyz).broad,
                    frac(MotuLayerUv(_ProbeLocalPosition.xyz, MOTU_LAYER_BEACH).x));
            }
            ENDHLSL
        }
    }
}
