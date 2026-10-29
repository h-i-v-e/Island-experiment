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
    // Terrain shadows block direct sunlight, not the diffuse sky illumination
    // scattered by the droplets. Keep the environment's ambient colour so
    // shaded spray remains visible while still darkening naturally at night.
    fixed3 ambient = _MotuCloudAmbientColor.rgb
        * cloud.ambientTransmittance
        * 0.72h;
    fixed3 direct = _MotuCloudLightColor.rgb
        * terrainVisibility
        * cloud.directTransmittance
        * 0.72h;
#if defined(_ADDITIONAL_LIGHTS)
    uint count = GetAdditionalLightsCount();
    for (uint index = 0; index < count; ++index)
    {
        Light light = GetAdditionalLight(index, worldPosition, half4(1, 1, 1, 1));
        direct += light.color * light.distanceAttenuation * light.shadowAttenuation
            * MotuAdditionalLightCloudTransmittance(index, worldPosition, light.direction) * 0.72h;
    }
#endif
    return tint * (ambient + direct);
}

#endif
