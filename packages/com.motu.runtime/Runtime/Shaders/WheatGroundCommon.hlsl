#ifndef MOTU_WHEAT_GROUND_INCLUDED
#define MOTU_WHEAT_GROUND_INCLUDED

TEXTURE2D(_MotuWheatGroundMask);
float4 _MotuWheatGroundUV;

half MotuWheatGroundCoverage(float2 islandXZ)
{
    UNITY_BRANCH
    if (_MotuWheatGroundUV.x <= 0.0) return 0.0h;

    float2 uv = islandXZ * _MotuWheatGroundUV.xy + _MotuWheatGroundUV.zw;
    float2 dx = ddx(uv);
    float2 dy = ddy(uv);
    UNITY_BRANCH
    if (any(uv < 0.0) || any(uv > 1.0)) return 0.0h;

    return SAMPLE_TEXTURE2D_GRAD(_MotuWheatGroundMask, sampler_LinearClamp, uv, dx, dy).r;
}

half MotuWheatCanopyExclusion(float2 islandXZ)
{
    UNITY_BRANCH
    if (_MotuWheatGroundUV.x <= 0.0) return 0.0h;
    float2 uv = islandXZ * _MotuWheatGroundUV.xy + _MotuWheatGroundUV.zw;
    // Keep small trunk openings at distance even when the soil mask is mipmapped.
    return SAMPLE_TEXTURE2D_LOD(_MotuWheatGroundMask, sampler_LinearClamp, uv, 0).g;
}

#endif
