#ifndef MOTU_CLOUD_COMMON_INCLUDED
#define MOTU_CLOUD_COMMON_INCLUDED

sampler2D _MotuCloudWeatherTex;
float _MotuCloudEnabled;
float _MotuCloudCoverage;
float _MotuCloudDensity;
float _MotuCloudAltitude;
float _MotuCloudWorldSize;
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

half MotuCloudSkyDensity(
    float3 cameraLocalPosition,
    float3 localViewDirection)
{
    if (_MotuCloudEnabled < 0.5)
        return 0.0h;

    float3 viewDirection = normalize(localViewDirection);
    float heightToCloud = _MotuCloudAltitude - cameraLocalPosition.y;
    if (heightToCloud <= 0.0 || viewDirection.y <= 0.0001)
        return 0.0h;

    float travel = heightToCloud / viewDirection.y;
    float2 cloudPosition = cameraLocalPosition.xz + viewDirection.xz * travel;
    half horizonFade = smoothstep(0.005h, 0.035h, viewDirection.y);
    return MotuCloudDensityAtLocalPosition(cloudPosition) * horizonFade;
}

half MotuCloudCelestialTransmittance(half density)
{
    return MotuCloudOpticalTransmittance(
        density,
        max(_MotuCloudCelestialStrength, 0.0));
}

fixed3 MotuCloudSkyColour(half density, float3 localViewDirection)
{
    half sunset = saturate(_MotuCloudSunsetStrength)
        * (1.0h - saturate(_MotuCloudNightStrength));
    fixed3 daylight = lerp(
        _MotuCloudDayColor.rgb,
        _MotuCloudSunsetColor.rgb,
        sunset);
    fixed3 colour = lerp(
        daylight,
        _MotuCloudNightColor.rgb,
        saturate(_MotuCloudNightStrength));
    float3 lightDirection = normalize(_MotuCloudLightDirection.xyz);
    half forwardLight = pow(
        saturate(dot(normalize(localViewDirection), lightDirection)),
        10.0h);
    half lighting = lerp(0.58h, 1.0h, saturate(lightDirection.y * 2.0h));
    lighting += forwardLight * (1.0h - density) * 0.28h;
    return colour * lighting;
}

#endif
