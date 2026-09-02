#ifndef MOTU_WATERFALL_SPRAY_LIGHTING_INCLUDED
#define MOTU_WATERFALL_SPRAY_LIGHTING_INCLUDED

#include "CloudCommon.cginc"

fixed3 MotuWaterfallSprayLighting(
    fixed3 tint,
    float3 worldPosition,
    half shadowAttenuation)
{
    MotuCloudLighting cloud = MotuCloudSurfaceLighting(worldPosition);
    half terrainVisibility = saturate(shadowAttenuation);
    // Mist is translucent, but it is not emissive. In a terrain shadow the
    // surrounding sky contribution must fall with the direct light or the pale
    // tint reads as a glowing object against the dark waterfall face.
    half ambientVisibility = lerp(
        0.06h,
        1.0h,
        terrainVisibility * terrainVisibility);
    fixed3 ambient = _MotuCloudAmbientColor.rgb
        * cloud.ambientTransmittance
        * ambientVisibility
        * 0.72h;
    fixed3 direct = _MotuCloudLightColor.rgb
        * terrainVisibility
        * cloud.directTransmittance
        * 0.72h;
    return tint * (ambient + direct);
}

#endif
