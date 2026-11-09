#ifndef MOTU_WORLD_SQUALL_FIELD_INCLUDED
#define MOTU_WORLD_SQUALL_FIELD_INCLUDED

// XY true world X/Z centre; Z/W squared inner/outer radii.
float4 _MotuSquall, _MotuSquall1, _MotuSquall2;
// X strength-weighted wind gain above ambient; Y/Z ambient advection velocity.
float4 _MotuSquallWind, _MotuSquallWind1, _MotuSquallWind2;

#ifndef MOTU_SQUALL_TIME_OFFSET
#define MOTU_SQUALL_TIME_OFFSET 0.0
#endif

float MotuSquallCellWeight(float2 worldPosition, float4 bounds, float4 wind)
{
    float2 center = bounds.xy + wind.yz * MOTU_SQUALL_TIME_OFFSET;
    float2 offset = worldPosition - center;
    float distanceSquared = dot(offset, offset);
    if (bounds.w <= 0.0 || distanceSquared >= bounds.w) return 0.0;
    if (distanceSquared <= bounds.z) return 1.0;
    float t = saturate((distanceSquared - bounds.z) / (bounds.w - bounds.z));
    return 1.0 - t * t * (3.0 - 2.0 * t);
}

float2 MotuSquallCellGradient(float2 worldPosition, float4 bounds, float4 wind)
{
    float2 center = bounds.xy + wind.yz * MOTU_SQUALL_TIME_OFFSET;
    float2 offset = worldPosition - center;
    float distanceSquared = dot(offset, offset);
    if (wind.x <= 0.0 || bounds.w <= bounds.z
        || distanceSquared <= bounds.z || distanceSquared >= bounds.w) return 0.0;
    float span = bounds.w - bounds.z;
    float t = (distanceSquared - bounds.z) / span;
    return -12.0 * wind.x * t * (1.0 - t) * offset / span;
}

float MotuSquallWeightAt(float2 worldPosition)
{
    return max(MotuSquallCellWeight(worldPosition, _MotuSquall, _MotuSquallWind),
        max(MotuSquallCellWeight(worldPosition, _MotuSquall1, _MotuSquallWind1),
            MotuSquallCellWeight(worldPosition, _MotuSquall2, _MotuSquallWind2)));
}

float MotuSquallWindMultiplier(float2 worldPosition)
{
    float gain = _MotuSquallWind.x * MotuSquallCellWeight(worldPosition, _MotuSquall, _MotuSquallWind);
    gain = max(gain, _MotuSquallWind1.x * MotuSquallCellWeight(worldPosition, _MotuSquall1, _MotuSquallWind1));
    gain = max(gain, _MotuSquallWind2.x * MotuSquallCellWeight(worldPosition, _MotuSquall2, _MotuSquallWind2));
    return 1.0 + max(0.0, gain);
}

float2 MotuSquallWindMultiplierGradient(float2 worldPosition)
{
    float gain = _MotuSquallWind.x * MotuSquallCellWeight(worldPosition, _MotuSquall, _MotuSquallWind);
    float4 bounds = _MotuSquall, wind = _MotuSquallWind;
    float candidate = _MotuSquallWind1.x * MotuSquallCellWeight(worldPosition, _MotuSquall1, _MotuSquallWind1);
    if (candidate > gain) { gain = candidate; bounds = _MotuSquall1; wind = _MotuSquallWind1; }
    candidate = _MotuSquallWind2.x * MotuSquallCellWeight(worldPosition, _MotuSquall2, _MotuSquallWind2);
    if (candidate > gain) { bounds = _MotuSquall2; wind = _MotuSquallWind2; }
    return MotuSquallCellGradient(worldPosition, bounds, wind);
}

#endif
