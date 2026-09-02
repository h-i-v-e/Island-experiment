#ifndef MOTU_CLOUD_COMMON_INCLUDED
#define MOTU_CLOUD_COMMON_INCLUDED

sampler2D _MotuCloudWeatherTex;
float _MotuCloudEnabled;
float _MotuCloudCoverage;
float _MotuCloudDensity;
float _MotuCloudAltitude;
float _MotuCloudWorldSize;
float4 _MotuCloudVolume;
float4 _MotuCloudBroadNoise;
float4 _MotuCloudDetailErosion;
float4 _MotuCloudWindOffset;
fixed4 _MotuCloudDayColor;
fixed4 _MotuCloudSunsetColor;
fixed4 _MotuCloudNightColor;
float _MotuCloudShadowStrength;
float _MotuCloudAmbientShadowStrength;
float _MotuCloudCelestialStrength;
float _MotuCloudLowElevationFade;
float4 _MotuCloudLightDirection;
float4 _MotuEnvironmentWorldOffset;
float _MotuCloudLightActive;
fixed4 _MotuCloudLightColor;
fixed4 _MotuCloudAmbientColor;
float _MotuCloudSunsetStrength;
float _MotuCloudNightStrength;

float3 MotuCloudWorldToLocal(float3 worldPosition)
{
    return worldPosition + _MotuEnvironmentWorldOffset.xyz;
}

float2 MotuCloudWeatherUv(float2 localPosition)
{
    return (localPosition + _MotuCloudWindOffset.xy)
        / max(_MotuCloudWorldSize, 1.0);
}

float MotuCloudHash(float2 cell)
{
    float3 value = frac(float3(cell.xyx) * 0.1031);
    value += dot(value, value.yzx + 33.33);
    return frac((value.x + value.y) * value.z);
}

half MotuCloudBroadDensity(float2 localPosition)
{
    float broadWorldSize = max(
        _MotuCloudWorldSize * max(_MotuCloudBroadNoise.x, 2.0),
        1.0);
    float2 position = (localPosition + _MotuCloudWindOffset.zw)
        / broadWorldSize;
    float2 cell = floor(position);
    float2 blend = frac(position);
    blend = blend * blend * (3.0 - 2.0 * blend);
    float lower = lerp(
        MotuCloudHash(cell),
        MotuCloudHash(cell + float2(1.0, 0.0)),
        blend.x);
    float upper = lerp(
        MotuCloudHash(cell + float2(0.0, 1.0)),
        MotuCloudHash(cell + float2(1.0, 1.0)),
        blend.x);
    return (half)lerp(lower, upper, blend.y);
}

half MotuCloudDensityAtLocalPosition(float2 localPosition)
{
    if (_MotuCloudEnabled < 0.5)
        return 0.0h;

    half4 weather = tex2D(
        _MotuCloudWeatherTex,
        MotuCloudWeatherUv(localPosition));
    half detailStrength = saturate(_MotuCloudDetailErosion.x);
    half erosionStrength = saturate(_MotuCloudDetailErosion.y);
    half structure = lerp(weather.r, weather.r * 0.58h + weather.g * 0.42h, detailStrength);
    structure -= (weather.b - 0.5h) * erosionStrength * 0.44h;
    half broadDensity = MotuCloudBroadDensity(localPosition);
    half broadStrength = saturate(_MotuCloudBroadNoise.y);
    half regionalCoverage = saturate(
        _MotuCloudCoverage
        + (weather.a - 0.5h) * 0.34h
        + (broadDensity - 0.5h) * broadStrength * 0.72h);
    half threshold = 1.0h - regionalCoverage;
    half edgeWidth = lerp(0.08h, 0.20h, erosionStrength);
    return smoothstep(threshold - edgeWidth, threshold + edgeWidth, structure);
}

half MotuCloudOpticalTransmittance(half density, half strength)
{
    // 1.442695 converts a natural exponential coefficient to exp2.
    return exp2(-density
        * max(_MotuCloudDensity, 0.0)
        * max(strength, 0.0h)
        * 1.442695h);
}

half MotuCloudSurfaceDensity(float3 worldPosition)
{
    if (_MotuCloudEnabled < 0.5 || _MotuCloudLightActive < 0.5)
        return 0.0h;

    float3 localPosition = MotuCloudWorldToLocal(worldPosition);
    float3 lightDirection = normalize(_MotuCloudLightDirection.xyz);
    float heightToCloud = _MotuCloudAltitude - localPosition.y;
    if (heightToCloud <= 0.0 || lightDirection.y <= 0.0001)
        return 0.0h;

    float travel = heightToCloud / lightDirection.y;
    float2 cloudPosition = localPosition.xz + lightDirection.xz * travel;
    half elevationFade = smoothstep(
        0.0,
        max(_MotuCloudLowElevationFade, 0.001),
        lightDirection.y);
    return MotuCloudDensityAtLocalPosition(cloudPosition) * elevationFade;
}

struct MotuCloudLighting
{
    half directTransmittance;
    half ambientTransmittance;
};

MotuCloudLighting MotuCloudSurfaceLighting(float3 worldPosition)
{
    half density = MotuCloudSurfaceDensity(worldPosition);
    half optical = MotuCloudOpticalTransmittance(density, 1.0h);
    MotuCloudLighting lighting;
    lighting.directTransmittance = lerp(
        1.0h,
        optical,
        saturate(_MotuCloudShadowStrength));
    lighting.ambientTransmittance = 1.0h
        - density * saturate(_MotuCloudAmbientShadowStrength);
    return lighting;
}

fixed3 MotuCloudSkyColour(half density, float3 localViewDirection)
{
    half sunset = saturate(_MotuCloudSunsetStrength)
        * (1.0h - saturate(_MotuCloudNightStrength));
    fixed3 cloudAlbedo = lerp(
        _MotuCloudDayColor.rgb,
        _MotuCloudSunsetColor.rgb,
        sunset);
    float3 lightDirection = normalize(_MotuCloudLightDirection.xyz);
    half forwardLight = pow(
        saturate(dot(normalize(localViewDirection), lightDirection)),
        10.0h);
    half lightElevation = saturate(lightDirection.y * 2.0h);
    fixed3 illumination = _MotuCloudAmbientColor.rgb * 0.95h;
    illumination += _MotuCloudLightColor.rgb
        * lerp(0.28h, 0.48h, lightElevation);
    illumination += _MotuCloudLightColor.rgb
        * forwardLight
        * (1.0h - density)
        * 0.32h;
    illumination += _MotuCloudSunsetColor.rgb * sunset * 0.22h;
    fixed3 litColour = cloudAlbedo * illumination;
    fixed3 nightFloor = _MotuCloudNightColor.rgb
        * saturate(_MotuCloudNightStrength)
        * 0.35h;
    return max(litColour, nightFloor);
}

struct MotuCloudSkyVolume
{
    half transmittance;
    fixed3 averageColour;
};

MotuCloudSkyVolume MotuCloudSkyVolumeAt(
    float3 cameraLocalPosition,
    float3 localViewDirection,
    float maximumDistance,
    float2 screenPosition)
{
    MotuCloudSkyVolume result;
    result.transmittance = 1.0h;
    result.averageColour = fixed3(0.0, 0.0, 0.0);
    if (_MotuCloudEnabled < 0.5)
        return result;

    float layerThickness = max(_MotuCloudVolume.x, 25.0);
    float layerBase = _MotuCloudAltitude - layerThickness * 0.5;
    float3 viewDirection = normalize(localViewDirection);
    maximumDistance = max(maximumDistance, 0.0);
    float domeRadius = max(maximumDistance, 1.0);
    // The visual cloud layer is a shallow viewer-centred dome. Its lower
    // surface begins at the configured world altitude overhead and descends
    // continuously to sea level at the sky-dome rim. Cloud shadows remain at
    // their physical world altitude; this curvature is the distant visual
    // continuation that prevents a flat slab from ending in a visible line.
    float edgeLayerBase = min(layerBase, 0.0);
    const int IntersectionSampleCount = 24;
    float intersectionStep = maximumDistance / IntersectionSampleCount;
    float entryDistance = maximumDistance;
    float exitDistance = -1.0;
    // Locate the portion of this ray that lies inside the curved layer using a
    // cheap geometry-only scan. The denser integration below still performs
    // the same number of weather-map samples as before.
    for (int intersectionIndex = 0;
        intersectionIndex <= IntersectionSampleCount;
        intersectionIndex++)
    {
        float intersectionDistance = intersectionIndex * intersectionStep;
        float3 intersectionPosition = cameraLocalPosition
            + viewDirection * intersectionDistance;
        float radialFraction = saturate(
            length(intersectionPosition.xz - cameraLocalPosition.xz)
            / domeRadius);
        float domeSag = radialFraction * radialFraction;
        float curvedLayerBase = lerp(layerBase, edgeLayerBase, domeSag);
        float curvedHeightFraction =
            (intersectionPosition.y - curvedLayerBase) / layerThickness;
        if (curvedHeightFraction >= 0.0 && curvedHeightFraction <= 1.0)
        {
            entryDistance = min(
                entryDistance,
                max(intersectionDistance - intersectionStep, 0.0));
            exitDistance = max(
                exitDistance,
                min(intersectionDistance + intersectionStep, maximumDistance));
        }
    }

    if (exitDistance <= entryDistance)
        return result;

    const int VolumeSampleCount = 12;
    float sampleLength = (exitDistance - entryDistance) / VolumeSampleCount;
    // Interleaved-gradient noise phases the samples in camera space. Without
    // this, the same fractional step on every ray maps back to a constant
    // world height and exposes the individual horizontal strata.
    float samplePhase = frac(52.9829189 * frac(dot(
        floor(screenPosition),
        float2(0.06711056, 0.00583715))));
    fixed3 accumulatedColour = fixed3(0.0, 0.0, 0.0);
    half accumulatedTransmittance = 1.0h;
    for (int sampleIndex = 0; sampleIndex < VolumeSampleCount; sampleIndex++)
    {
        float distanceAlongRay = entryDistance
            + (sampleIndex + samplePhase) * sampleLength;
        float3 samplePosition = cameraLocalPosition
            + viewDirection * distanceAlongRay;
        float radialFraction = saturate(
            length(samplePosition.xz - cameraLocalPosition.xz)
            / domeRadius);
        float domeSag = radialFraction * radialFraction;
        float curvedLayerBase = lerp(layerBase, edgeLayerBase, domeSag);
        float heightFraction = saturate(
            (samplePosition.y - curvedLayerBase) / layerThickness);
        half baseShape = smoothstep(0.0h, 0.18h, heightFraction);
        half topShape = 1.0h - smoothstep(0.68h, 1.0h, heightFraction);
        half verticalShape = baseShape * topShape;
        float2 heightOffset = (heightFraction - 0.5)
            * _MotuCloudWorldSize
            * float2(0.035, -0.027);
        half sampleDensity = MotuCloudDensityAtLocalPosition(
            samplePosition.xz + heightOffset)
            * verticalShape;
        half opticalDepth = sampleDensity
            * max(_MotuCloudDensity, 0.0)
            * sampleLength
            / layerThickness;
        half sampleTransmittance = exp2(-opticalDepth * 1.442695h);
        half sampleOpacity = 1.0h - sampleTransmittance;
        fixed3 sampleColour = MotuCloudSkyColour(
            sampleDensity,
            viewDirection);
        half heightLighting = lerp(0.70h, 1.10h, heightFraction);
        half interiorLighting = lerp(
            1.0h,
            0.76h,
            1.0h - accumulatedTransmittance);
        sampleColour *= heightLighting * interiorLighting;
        accumulatedColour += accumulatedTransmittance
            * sampleOpacity
            * sampleColour;
        accumulatedTransmittance *= sampleTransmittance;
    }

    half accumulatedOpacity = 1.0h - accumulatedTransmittance;
    result.transmittance = accumulatedTransmittance;
    result.averageColour = accumulatedColour / max(accumulatedOpacity, 0.0001h);
    return result;
}

#endif
