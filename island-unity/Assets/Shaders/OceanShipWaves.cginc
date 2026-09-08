#ifndef MOTU_OCEAN_SHIP_WAVES_INCLUDED
#define MOTU_OCEAN_SHIP_WAVES_INCLUDED
sampler2D _MotuShipWaveField;
float4 _MotuShipWaveRect;
float _MotuShipWaveEnabled;

float3 MotuShipWaveField(float2 worldXZ)
{
    if (_MotuShipWaveEnabled < .5) return 0;
    float2 uv = (worldXZ - _MotuShipWaveRect.xy) * _MotuShipWaveRect.zw;
    if (any(uv < 0) || any(uv > 1)) return 0;
    // Fade the edge of the moving 512 m field, rather than leave a visible seam.
    float edge = min(min(uv.x, uv.y), min(1-uv.x, 1-uv.y));
    return tex2Dlod(_MotuShipWaveField, float4(uv, 0, 0)).rgb * smoothstep(0, .03, edge);
}

float MotuShipWaveHeight(float waveHeight, float3 field, float capsule)
{
    float height = waveHeight + field.y;
    return height - max(height, 0) * max(capsule, saturate(field.x));
}

void MotuShipWaveShading(float2 worldXZ, float waveHeight, float capsule,
    inout float3 normal, inout float foam)
{
    float3 field = MotuShipWaveField(worldXZ);
    float suppression = max(capsule, saturate(field.x));
    if (_MotuShipWaveEnabled > .5 && any(field > 0))
    {
        // Differentiate the modified surface, including the edge of black areas.
        const float stepMetres = .5;
        float2 slope = -normal.xz / max(normal.y, .15);
        float left = MotuShipWaveHeight(waveHeight - slope.x * stepMetres,
            MotuShipWaveField(worldXZ - float2(stepMetres, 0)), capsule);
        float right = MotuShipWaveHeight(waveHeight + slope.x * stepMetres,
            MotuShipWaveField(worldXZ + float2(stepMetres, 0)), capsule);
        float back = MotuShipWaveHeight(waveHeight - slope.y * stepMetres,
            MotuShipWaveField(worldXZ - float2(0, stepMetres)), capsule);
        float front = MotuShipWaveHeight(waveHeight + slope.y * stepMetres,
            MotuShipWaveField(worldXZ + float2(0, stepMetres)), capsule);
        normal = normalize(float3(left - right, 2 * stepMetres, back - front));
    }
    else if (waveHeight > 0)
        normal = normalize(lerp(normal, float3(0, 1, 0), capsule));
    foam = max(foam, saturate(field.z)) * (1 - suppression);
}
#endif
