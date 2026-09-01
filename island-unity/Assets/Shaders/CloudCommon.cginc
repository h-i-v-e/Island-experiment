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
float _MotuCloudLightActive;
fixed4 _MotuCloudLightColor;
fixed4 _MotuCloudAmbientColor;
float _MotuCloudSunsetStrength;
float _MotuCloudNightStrength;

float3 MotuCloudWorldToLocal(float3 worldPosition)
{
    return mul(_IslandWorldToLocal, float4(worldPosition, 1.0)).xyz;
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
    float layerTop = layerBase + layerThickness;
    float3 viewDirection = normalize(localViewDirection);
    maximumDistance = max(maximumDistance, 0.0);
    float entryDistance = 0.0;
    float exitDistance = -1.0;
    if (abs(viewDirection.y) > 0.0001)
    {
        float distanceToBase = (layerBase - cameraLocalPosition.y)
            / viewDirection.y;
        float distanceToTop = (layerTop - cameraLocalPosition.y)
            / viewDirection.y;
        entryDistance = max(min(distanceToBase, distanceToTop), 0.0);
        exitDistance = min(
            max(distanceToBase, distanceToTop),
            maximumDistance);
    }

    if (exitDistance <= entryDistance)
    {
        // From below the layer, a flat slab cannot reach the geometric sea
        // horizon within the finite sky dome. Represent that distant portion
        // as one bounded coherent bank sampled at the dome itself. This joins
        // clouds to the sea without the unbounded grazing ray that became a
        // bright, repetitive wall for elevated sunset cameras.
        if (cameraLocalPosition.y >= layerBase)
            return result;

        float horizontalLength = length(viewDirection.xz);
        if (horizontalLength <= 0.0001)
            return result;

        float requiredElevation = saturate(
            (layerBase - cameraLocalPosition.y)
            / max(maximumDistance, 1.0));
        float bankUpperElevation = max(
            requiredElevation * 1.15,
            0.035);
        float bankHeight = saturate(
            (max(viewDirection.y, 0.0) + 0.004)
            / (bankUpperElevation + 0.004));
        half baseConnection = smoothstep(
            -0.02h,
            0.004h,
            viewDirection.y);
        // Begin rounding immediately above the sea connection. The previous
        // flat lower plateau repeated one weather-map value vertically and
        // turned every horizontal cloud edge into a tower.
        half topTaper = 1.0h - smoothstep(0.0h, 1.0h, bankHeight);
        half horizonBank = baseConnection * topTaper;
        if (horizonBank <= 0.0001h)
            return result;

        float2 horizontalDirection = viewDirection.xz / horizontalLength;
        float horizonDistance = maximumDistance * lerp(
            1.0,
            0.72,
            bankHeight);
        float2 horizonPosition = cameraLocalPosition.xz
            + horizontalDirection * horizonDistance;
        float2 verticalDrift = bankHeight
            * bankHeight
            * _MotuCloudWorldSize
            * float2(0.11, -0.08);
        half lowerDensity = MotuCloudDensityAtLocalPosition(
            horizonPosition + verticalDrift);
        half upperDensity = MotuCloudDensityAtLocalPosition(
            horizonPosition
            - verticalDrift * 0.37
            + _MotuCloudWorldSize * float2(-0.021, 0.017));
        half horizonDensity = lerp(
            lowerDensity,
            upperDensity,
            smoothstep(0.35h, 0.90h, bankHeight))
            * horizonBank;
        result.transmittance = MotuCloudOpticalTransmittance(
            horizonDensity,
            1.6h);
        result.averageColour = MotuCloudSkyColour(
            horizonDensity,
            viewDirection);
        return result;
    }

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
        float heightFraction = saturate(
            (samplePosition.y - layerBase) / layerThickness);
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
