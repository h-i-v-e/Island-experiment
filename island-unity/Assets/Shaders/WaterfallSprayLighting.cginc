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
    // Mist is translucent, but it is not emissive. Attenuate even the ambient
    // sky contribution to zero in a full terrain shadow; retaining a minimum
    // ambient floor makes dense pale mist and droplets appear self-lit.
    half ambientVisibility = terrainVisibility * terrainVisibility;
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
