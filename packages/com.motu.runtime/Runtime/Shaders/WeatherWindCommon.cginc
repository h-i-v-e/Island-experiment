#ifndef MOTU_WEATHER_WIND_COMMON_INCLUDED
#define MOTU_WEATHER_WIND_COMMON_INCLUDED

#include "WorldSquallField.cginc"

// One world-owned contract drives every wind-animated material.
// WeatherWind: XY normalized world X/Z direction, Z metres per second, W
// speed-derived response scale. WindMaterial: X vegetation displacement at the
// reference speed, Y gust size, Z grass normal response.
sampler2D _MotuWindNoise;
float4 _MotuWeatherWind;
float4 _MotuWindMaterial;
float4 _MotuWindOffset;

float2 MotuWindDirection()
{
    float2 configuredDirection = _MotuWeatherWind.xy;
    float directionLengthSquared = dot(configuredDirection, configuredDirection);
    float2 windDirection = configuredDirection
        * rsqrt(max(directionLengthSquared, 1.0e-4));
    return lerp(
        float2(1.0, 0.0),
        windDirection,
        step(1.0e-4, directionLengthSquared));
}

float MotuWindSpeedAt(float2 worldPosition)
{
    return max(_MotuWeatherWind.z, 0.0) * MotuSquallWindMultiplier(worldPosition);
}

float MotuWindResponseGain(float2 worldPosition)
{
    float maximumGain = max(1.0, 2.5 / max(_MotuWeatherWind.w, 0.001));
    return min(sqrt(MotuSquallWindMultiplier(worldPosition)), maximumGain);
}

float2 MotuWindResponseGainGradient(float2 worldPosition)
{
    float windGain = sqrt(MotuSquallWindMultiplier(worldPosition));
    float maximumGain = max(1.0, 2.5 / max(_MotuWeatherWind.w, 0.001));
    if (windGain >= maximumGain) return 0.0;
    return MotuSquallWindMultiplierGradient(worldPosition) * (0.5 / max(windGain, 0.001));
}

float MotuWindResponseScale(float2 worldPosition)
{
    return max(_MotuWeatherWind.w, 0.0) * MotuWindResponseGain(worldPosition);
}

float MotuWindDisplacementStrength()
{
    return max(_MotuWindMaterial.x, 0.0);
}

float MotuWindNormalStrength()
{
    return saturate(_MotuWindMaterial.z);
}

float2 MotuWindAdvectedPosition(float2 worldPosition)
{
    return worldPosition - _MotuWindOffset.xy;
}

float3 MotuWindSample(float2 worldPosition)
{
    float2 windDirection = MotuWindDirection();
    float gustWorldSize = max(_MotuWindMaterial.y, 1.0);
    // The generated texture's blue channel has eight coherent cells per
    // repeat. Scale the repeat so one blue cell matches the configured gust
    // size; the denser red and green channels provide natural detail.
    float windTextureWorldSize = gustWorldSize * 8.0;
    float2 advectedPosition = MotuWindAdvectedPosition(worldPosition)
        / windTextureWorldSize;
    half3 broadWindNoise = tex2Dlod(
        _MotuWindNoise,
        float4(advectedPosition, 0.0, 0.0)).rgb;
    half3 detailWindNoise = tex2Dlod(
        _MotuWindNoise,
        float4(
            advectedPosition * 1.73 + float2(0.31, 0.67),
            0.0,
            0.0)).rgb;
    float gust = smoothstep(
        0.12,
        0.88,
        broadWindNoise.b * 0.75 + detailWindNoise.r * 0.25);

    // Gusts vary force, never the world-space heading chosen by the driver.
    // Vegetation retains its authored ambient displacement and culling envelope;
    // ships and ocean waves sample the local squall response explicitly.
    return float3(
        windDirection.x,
        lerp(0.2, 1.0, gust) * max(_MotuWeatherWind.w, 0.0),
        windDirection.y);
}

#endif
