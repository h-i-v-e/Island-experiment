#ifndef MOTU_OCEAN_WATER_IMPACTS_INCLUDED
#define MOTU_OCEAN_WATER_IMPACTS_INCLUDED

// Project circular water contacts directly onto the final displaced ocean.
// The CPU bounds the active stamps; most ocean fragments skip the loop entirely.
int _MotuWaterImpactCount;
float4 _MotuWaterImpactRings[32]; // world XZ, expanding radius, band width
float4 _MotuWaterImpactShapes[32]; // fading foam opacity, normal strength
float4 _MotuWaterImpactBounds;

void MotuOceanWaterImpacts(float2 position, float footprint, out float foam, out float2 slope)
{
    foam = 0;
    slope = 0;
    [branch]
    if (_MotuWaterImpactCount <= 0 || any(position < _MotuWaterImpactBounds.xy)
        || any(position > _MotuWaterImpactBounds.zw)) return;
    [loop]
    for (int i = 0; i < min(_MotuWaterImpactCount, 32); i++)
    {
        float4 ring = _MotuWaterImpactRings[i];
        float2 delta = position - ring.xy;
        float distance = length(delta);
        if (distance > ring.z + ring.w * 4) continue;
        float width = max(ring.w, footprint * .65);
        float band = (distance - ring.z) / max(width, .001);
        float inner = (distance - ring.z * .64) / max(width * .8, .001);
        float outerRing = exp2(-band * band * 2);
        float innerRing = exp2(-inner * inner * 2) * .45;
        float filter = ring.w / max(width, .001);
        foam = max(foam, (outerRing + innerRing) * _MotuWaterImpactShapes[i].x * filter);
        slope += delta / max(distance, .001) * (band * outerRing + inner * innerRing)
            * _MotuWaterImpactShapes[i].y * filter;
    }
    foam = saturate(foam);
}
#endif
