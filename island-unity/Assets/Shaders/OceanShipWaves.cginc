#ifndef MOTU_OCEAN_SHIP_WAVES_INCLUDED
#define MOTU_OCEAN_SHIP_WAVES_INCLUDED
sampler2D _MotuShipWaveField;
float4 _MotuShipWaveRect;
float _MotuShipWaveEnabled;
float _BoatDisplacementFoamStrength;

float3 MotuShipWaveField(float2 worldXZ)
{
    if (_MotuShipWaveEnabled < .5) return 0;
    float2 uv = (worldXZ - _MotuShipWaveRect.xy) * _MotuShipWaveRect.zw;
    if (any(uv < 0) || any(uv > 1)) return 0;
    // Fade the edge of the moving 512 m field, rather than leave a visible seam.
    float edge = min(min(uv.x, uv.y), min(1-uv.x, 1-uv.y));
    return tex2Dlod(_MotuShipWaveField, float4(uv, 0, 0)).rgb * smoothstep(0, .03, edge);
}

float MotuShipWaveHeight(float waveHeight, float3 field, float capsule, float maximumWaveHeight)
{
    float height = waveHeight + field.y;
    float mask = saturate(max(capsule, field.x));
    if (mask <= 0.0) return height;
    // The footprint defines a ceiling, not a multiplier on each passing wave.
    // Full black caps at the sea plane; the feather lifts the ceiling towards
    // the maximum wave envelope. Waves already below it remain untouched.
    float ceiling = (max(maximumWaveHeight, 0.0) + max(field.y, 0.0)) * (1.0 - mask);
    return min(height, ceiling);
}

float MotuShipDisplacementFoam(float displacementMetres)
{
    return saturate(max(displacementMetres, 0.0) * max(_BoatDisplacementFoamStrength, 0.0));
}

void MotuShipWaveShading(float2 worldXZ, float waveHeight, float capsule, float maximumWaveHeight,
    inout float3 normal, inout float foam)
{
    float3 field = MotuShipWaveField(worldXZ);
    float originalHeight = waveHeight + field.y;
    float modifiedHeight = MotuShipWaveHeight(waveHeight, field, capsule, maximumWaveHeight);
    float suppression = originalHeight > 0.0001
        ? saturate((originalHeight - modifiedHeight) / originalHeight) : 0.0;
    if (_MotuShipWaveEnabled > .5 && any(field > 0))
    {
        // Differentiate the modified surface, including the edge of black areas.
        const float stepMetres = .5;
        float2 slope = -normal.xz / max(normal.y, .15);
        float left = MotuShipWaveHeight(waveHeight - slope.x * stepMetres,
            MotuShipWaveField(worldXZ - float2(stepMetres, 0)), capsule, maximumWaveHeight);
        float right = MotuShipWaveHeight(waveHeight + slope.x * stepMetres,
            MotuShipWaveField(worldXZ + float2(stepMetres, 0)), capsule, maximumWaveHeight);
        float back = MotuShipWaveHeight(waveHeight - slope.y * stepMetres,
            MotuShipWaveField(worldXZ - float2(0, stepMetres)), capsule, maximumWaveHeight);
        float front = MotuShipWaveHeight(waveHeight + slope.y * stepMetres,
            MotuShipWaveField(worldXZ + float2(0, stepMetres)), capsule, maximumWaveHeight);
        normal = normalize(float3(left - right, 2 * stepMetres, back - front));
    }
    else if (modifiedHeight < waveHeight)
        normal = float3(0, 1, 0);
    foam = max(foam, saturate(field.z)) * (1 - suppression);
    // Translucency already uses the final height; do not dim an untrimmed wave.
}
#endif
