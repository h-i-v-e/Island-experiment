#ifndef MOTU_SQUALL_LIGHTING_INCLUDED
#define MOTU_SQUALL_LIGHTING_INCLUDED

float4 _MotuSquallCloud0, _MotuSquallCloud1, _MotuSquallCloud2;
float4 _MotuSquallShade0, _MotuSquallShade1, _MotuSquallShade2;

float MotuSquallShadowNoise(float2 p)
{
    float2 cell = floor(p), f = frac(p);
    f = f * f * (3 - 2 * f);
    float4 h = frac(sin(float4(dot(cell, float2(127.1, 311.7)), dot(cell + float2(1, 0), float2(127.1, 311.7)),
        dot(cell + float2(0, 1), float2(127.1, 311.7)), dot(cell + 1, float2(127.1, 311.7)))) * 43758.5453);
    return lerp(lerp(h.x, h.y, f.x), lerp(h.z, h.w, f.x), f.y);
}

float MotuSquallCloudCellTransmittance(float3 position, float3 lightDirection, float4 cloud, float4 shade)
{
    if (cloud.w <= 0 || shade.y <= 0 || lightDirection.y <= .02 || position.y >= cloud.z) return 1;
    float2 projected = position.xz + lightDirection.xz * ((cloud.z - position.y) / lightDirection.y);
    float2 relative = projected - cloud.xy;
    float outer = cloud.w * (1 + shade.x);
    float edge = saturate((outer - length(relative)) / max(outer - cloud.w * .65, 1));
    if (edge <= 0) return 1;
    float density = edge * edge * (3 - 2 * edge);
    float2 flow = relative / max(cloud.w * .28, 55) + shade.w + shade.z * float2(.035, -.024);
    density *= lerp(.65, 1, MotuSquallShadowNoise(flow));
    return exp(-4 * shade.y * density * smoothstep(.02, .12, lightDirection.y));
}

float MotuSquallCloudTransmittance(float3 position, float3 lightDirection)
{
    float result = MotuSquallCloudCellTransmittance(position, lightDirection, _MotuSquallCloud0, _MotuSquallShade0);
    result *= MotuSquallCloudCellTransmittance(position, lightDirection, _MotuSquallCloud1, _MotuSquallShade1);
    result *= MotuSquallCloudCellTransmittance(position, lightDirection, _MotuSquallCloud2, _MotuSquallShade2);
    return max(.05, result);
}
#endif
