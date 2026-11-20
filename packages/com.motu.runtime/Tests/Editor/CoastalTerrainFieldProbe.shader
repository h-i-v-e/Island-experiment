Shader "Hidden/Motu/Tests/Coastal Terrain Field"
{
    Properties
    {
        _MotuTerrainFieldOffset ("Logical offset", Vector) = (0,0,0,0)
        _MotuTerrainSeam ("Coastal half width and collar", Vector) = (0,0,0,0)
        _ProbeBakedMap ("Probe baked map weight", Float) = 0
        _ProbeLocalPosition ("Local sample", Vector) = (0,0,0,0)
        _ProbeRoadCoverage ("Probe road coverage", Float) = 0
        _ProbeWheatCoverage ("Probe wheat coverage", Float) = 0
        _ProbeEnvironment ("Terrain environment", Vector) = (0,0,0,1)
        _CliffNoisePeriod ("Noise period", Float) = 160
        [NoScaleOffset] _CliffNoise3D ("Noise", 3D) = "gray" {}
        [NoScaleOffset] _GrassPatchNoise ("Grass patches", 2D) = "white" {}
        [NoScaleOffset] _MotuWheatGroundMask ("Wheat ground coverage", 2D) = "black" {}
        _MotuWheatGroundUV ("Wheat ground coordinates", Vector) = (0,0,0,0)
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
            float4 _ProbeEnvironment;
            float _ProbeBakedMap;
            float _ProbeRoadCoverage;
            float _ProbeWheatCoverage;
            float4 frag(v2f_img input) : SV_Target
            {
                if (_ProbeBakedMap > 0.5)
                    return MotuTerrainBakedMapWeight(_ProbeLocalPosition.xyz).xxxx;
                if (_ProbeWheatCoverage > 0.5 || _ProbeRoadCoverage > 0.5)
                {
                    MotuTerrainCoverage coverage = MotuBuildTerrainCoverage(
                        _ProbeLocalPosition.xyz, half3(0, 1, 0), half4(0, 1, 0, 0), _ProbeEnvironment);
                    if (_ProbeWheatCoverage > 0.5)
                        return float4(MotuWheatGroundCoverage(_ProbeLocalPosition.xz),
                            coverage.grass, coverage.forestFloor, coverage.stones);
                    return float4(coverage.cartTrack, coverage.road, coverage.gravel, 1);
                }
                return float4(MotuSampleTerrainNoise(_ProbeLocalPosition.xyz).broad,
                    frac(MotuLayerUv(_ProbeLocalPosition.xyz, MOTU_LAYER_BEACH).x));
            }
            ENDHLSL
        }
    }
}
