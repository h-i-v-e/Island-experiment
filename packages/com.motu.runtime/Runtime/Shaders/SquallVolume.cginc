#ifndef MOTU_SQUALL_VOLUME_INCLUDED
#define MOTU_SQUALL_VOLUME_INCLUDED

// CloudCommon is included by MotuUrp. This finite volume uses exactly the sky's
// sculpting and scattering kernels, with a local clock and material settings.
float4 _SquallOrigin;
float4 _SquallBounds; // cloud diameter including fringe, full height, bottom, rain/core diameter
float4 _SquallLayer;  // cloud base, cloud thickness, rain intensity, extinction
float4 _SquallFlow;   // internal cloud advection X/Z
float4 _SquallRainVelocity; // downwind rain velocity X/Z, downward fall speed, unused
float _SquallTime, _SquallSeed, _SquallStrength, _SquallFlash, _SquallDarkness, _SquallDistanceOpacity;
float _SquallSeaLevel;

float MotuSquallCoreDiameter() { return _SquallBounds.w > 0 ? _SquallBounds.w : _SquallBounds.x; }

half MotuSquallCore(float2 position)
{
    return 1 - smoothstep(.65, 1.05, length(position) / max(MotuSquallCoreDiameter() * .5, 1));
}

float MotuSquallFootprint(float2 position)
{
    float2 radial = position / max(MotuSquallCoreDiameter() * .5, 1);
    // Match nearby droplet emission across the entire rain-core radius.
    return smoothstep(0, 1, 1 - length(radial));
}

half MotuSquallCloudDensity(float3 position)
{
    half core = MotuSquallCore(position.xz);
    float height = (position.y - _SquallLayer.x - (1 - core) * _SquallLayer.y * .12)
        / (_SquallLayer.y * lerp(.64, 1, core));
    float radial = length(position.xz) / max(_SquallBounds.x * .5, 1);
    half footprint = 1 - smoothstep(.65, 1, radial);
    if (height <= 0 || height >= 1 || footprint <= .001h) return 0;
    // Rescale the same sky-noise domain for the smaller physical storm.
    // Its rounded billows remain 3D, with several readable cells per cloud.
    const float domainScale = 1.8;
    float billow = max(50, MotuSquallCoreDiameter() * .20);
    float erosion = max(10, MotuSquallCoreDiameter() * .035);
    float3 noisePosition = position + float3(_SquallSeed * 23.6, _SquallSeed * 11.4, _SquallSeed * 37.2);
    float2 flow = _SquallFlow.xy * _SquallTime * .22 * domainScale;
    // Lower coverage in the broad fringe leaves disconnected, translucent
    // billows around the dark rain core rather than a hard circular outline.
    return MotuCloudSculptedDensity(noisePosition * domainScale, height, footprint * lerp(.42h, .86h, core),
        float4(billow, erosion, .9, 1), float4(1,1,0,0), float4(.28,1,1,0),
        flow, _SquallTime * 3);
}

half MotuSquallRainDensity(float3 position, float sampleSpacing)
{
    float fallSpeed = max(1, _SquallRainVelocity.z);
    // Trace back up the slanted column to its source in the cloud. Lower
    // rain is farther downwind; changing time carries its noise down this
    // same trajectory instead of merely scrolling an upright cylinder.
    float fallTime = max(0, _SquallLayer.x - position.y) / fallSpeed;
    float2 source = position.xz - _SquallRainVelocity.xy * fallTime;
    half footprint = MotuSquallFootprint(source);
    // Rain reaches far into the carved underside and continues below the
    // deepest rendered trough. The ray is terminated by scene/water depth.
    float top = _SquallLayer.x + _SquallLayer.y * .82;
    if (footprint <= .001h || position.y >= top) return 0;
    float overlap = 1 - smoothstep(_SquallLayer.x + _SquallLayer.y * .35, top, position.y);
    // Broad, vertically stretched shafts occupy the moving storm, independent
    // of the camera-local droplet layer. Positive height/time motion falls down.
    float scale = clamp(MotuSquallCoreDiameter() * .65, 160, 360);
    float3 flow = float3(source.x, (position.y + _SquallTime * fallSpeed) / 3, source.y) / scale
        + float3(_SquallSeed, 0, _SquallSeed * .37);
    float shapeMip = clamp(log2(max(1, sampleSpacing * 64 / scale)), 0, 5);
    half3 noise = tex3Dlod(_MotuCloudShapeTex, float4(flow, shapeMip)).rgb;
    half shafts = smoothstep(.35h, .68h, noise.r + (noise.g - .5h) * .2h);
    // Filter detail to the ray's footprint rather than aliasing sub-metre
    // streaks over long paths. The nearest particles supply those fine streaks.
    float3 detailUv = float3(source.x, (position.y + _SquallTime * fallSpeed) / 3, source.y) / 72;
    float detailMip = clamp(log2(max(1, sampleSpacing * 32 / 72)), 0, 4);
    half detail = tex3Dlod(_MotuCloudErosionTex, float4(detailUv + _SquallSeed, detailMip)).r;
    half threads = smoothstep(.32h, .72h, detail) * (1 - smoothstep(6, 24, sampleSpacing));
    // Horizon sinking moves the cloud proxy, but spray remains at the real sea.
    float seaHeight = position.y + _SquallOrigin.y - _SquallSeaLevel;
    half surfaceMist = exp2(-abs(seaHeight) * .13) * (.12h + noise.g * .28h);
    return footprint * overlap * (.12h + shafts * 1.6h + threads * .35h + surfaceMist) * _SquallLayer.z;
}

half MotuSquallLightTransmittance(float3 position, float3 lightDirection, half active)
{
    if (active < .5h) return 0;
    float3 direction = normalize(lightDirection + float3(0,.000001,0));
    float3 lo = float3(-_SquallBounds.x * .5, _SquallLayer.x, -_SquallBounds.x * .5);
    float3 hi = float3(_SquallBounds.x * .5, _SquallLayer.x + _SquallLayer.y, _SquallBounds.x * .5);
    float3 exitDistances = max((lo - position) / (direction + 1e-7), (hi - position) / (direction + 1e-7));
    float exitDistance = max(0, min(exitDistances.x, min(exitDistances.y, exitDistances.z)));
    float stepLength = exitDistance / 5;
    float opticalDepth = 0;
    [loop] for (int lightSample = 0; lightSample < 5; lightSample++)
        opticalDepth += MotuSquallCloudDensity(position + direction * ((lightSample + .5) * stepLength)) * stepLength;
    return exp2(-opticalDepth * _SquallLayer.w / 280 * 1.442695);
}

half3 MotuSquallCloudColour(float3 position, half density, float3 direction)
{
    half height = saturate((position.y - _SquallLayer.x) / _SquallLayer.y);
    half sunlight = MotuSquallLightTransmittance(position, _MotuCloudLightDirection.xyz, _MotuCloudLightActive);
    half secondary = MotuSquallLightTransmittance(position,
        _MotuCloudSecondaryLightDirection.xyz, _MotuCloudSecondaryLightActive);
    half column = (density + (1 - max(sunlight, secondary)) * .65h)
        * _SquallLayer.w * (_SquallLayer.y / 280) * (1 - height);
    half3 lighting = MotuCloudSkyColourForColumn(column, direction, sunlight, secondary, height);
    // Dark material albedo, not a replacement light model. Self-shadowing and
    // both celestial sources remain the same as the main volumetric clouds.
    half albedo = lerp(.72h, lerp(.55h, .19h, _SquallDarkness), MotuSquallCore(position.xz));
    return lighting * albedo
        + half3(.48,.62,1) * _SquallFlash * (.12h + height * .08h);
}

#endif
