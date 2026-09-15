#ifndef MOTU_OCEAN_DECK_WAVE_CLAMP_INCLUDED
#define MOTU_OCEAN_DECK_WAVE_CLAMP_INCLUDED

// Rendering only: deliberately excluded from OceanWaves.cginc and buoyancy queries.
int _MotuDeckCapsuleCount;
float4 _MotuDeckCapsuleEndpoints[8]; // world XZ of each end centre
float4 _MotuDeckCapsuleParameters[8]; // radius, outside blend distance

float MotuDeckWaveClampWeight(float2 worldXZ)
{
    float weight = 0.0;
    [loop]
    for (int i = 0; i < _MotuDeckCapsuleCount; ++i)
    {
        float4 ends = _MotuDeckCapsuleEndpoints[i];
        float2 segment = ends.zw - ends.xy;
        float along = saturate(dot(worldXZ - ends.xy, segment) / max(dot(segment, segment), 0.000001));
        float distanceToSegment = length(worldXZ - (ends.xy + segment * along));
        float2 settings = _MotuDeckCapsuleParameters[i].xy;
        weight = max(weight, 1.0 - smoothstep(settings.x, settings.x + settings.y, distanceToSegment));
    }
    return weight;
}

float MotuClampDeckWaveHeight(float height, float weight)
{
    // Leave troughs alone. The sea plane remains visible over submerged decks.
    return height - max(height, 0.0) * weight;
}

#endif
