#ifndef MOTU_WHEAT_DETAIL_BLEND_INCLUDED
#define MOTU_WHEAT_DETAIL_BLEND_INCLUDED

// XY is the streaming focus in world XZ; ZW are the fade distances in metres.
float MotuWheatDetailCoverage(float2 worldXZ, float4 blend)
{
    return blend.w > blend.z
        ? 1.0 - smoothstep(blend.z, blend.w, distance(worldXZ, blend.xy))
        : 1.0;
}

#endif
