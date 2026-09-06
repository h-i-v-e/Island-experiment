#ifndef MOTU_WEATHER_WIND_COMMON_INCLUDED
#define MOTU_WEATHER_WIND_COMMON_INCLUDED

// One world-owned contract drives every wind-animated material.
// WeatherWind: XY normalized world X/Z direction, Z metres per second, W
// speed-derived response scale. WindMaterial: X vegetation displacement at the
// reference speed, Y gust size, Z grass normal response.
sampler2D _MotuWindNoise;
float4 _MotuWeatherWind;
float4 _MotuWindMaterial;
float4 _MotuWindOffset;
// XYZ are the tree, reed, and fern flexibility multipliers.
float4 _MotuWindResponse;

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

float MotuTreeWindStrengthMultiplier()
{
    return max(_MotuWindResponse.x, 0.0);
}

float MotuReedWindStrengthMultiplier()
{
    return max(_MotuWindResponse.y, 0.0);
}

float MotuFernWindStrengthMultiplier()
{
    return max(_MotuWindResponse.z, 0.0);
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

    float turningNoise = detailWindNoise.b * 0.70
        + broadWindNoise.g * 0.30
        - 0.5;
    float2 crossWind = float2(-windDirection.y, windDirection.x);
    float2 localDirection = normalize(
        windDirection + crossWind * (turningNoise * 0.75));
    return float3(
        localDirection.x,
        lerp(0.2, 1.0, gust) * max(_MotuWeatherWind.w, 0.0),
        localDirection.y);
}

#endif
