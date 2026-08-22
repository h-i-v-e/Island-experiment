#ifndef MOTU_GRASS_WIND_COMMON_INCLUDED
#define MOTU_GRASS_WIND_COMMON_INCLUDED

float3 MotuGrassWindSample(float2 worldPosition)
{
    float2 configuredDirection = _GrassWindDirection.xz;
    float directionLengthSquared = dot(configuredDirection, configuredDirection);
    float2 windDirection = configuredDirection
        * rsqrt(max(directionLengthSquared, 1.0e-4));
    windDirection = lerp(
        float2(1.0, 0.0),
        windDirection,
        step(1.0e-4, directionLengthSquared));

    float windWorldSize = max(_GrassWindWorldSize, 1.0);
    // The generated texture's blue channel has eight coherent cells per
    // repeat. Scale the repeat so one blue cell matches the requested gust
    // size; the denser red and green channels then provide natural detail.
    float windTextureWorldSize = windWorldSize * 8.0;
    float2 advectedPosition = (
        worldPosition - windDirection * (_Time.y * _GrassWindSpeed))
        / windTextureWorldSize;
    // Reuse the generated grass texture as a moving wind field. Independent
    // channels supply broad gusts, finer pulses, and lateral direction noise.
    half3 broadWindNoise = tex2Dlod(
        _GrassPatchNoise,
        float4(advectedPosition, 0.0, 0.0)).rgb;
    half3 detailWindNoise = tex2Dlod(
        _GrassPatchNoise,
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
    return float3(localDirection.x, lerp(0.2, 1.0, gust), localDirection.y);
}

#endif
