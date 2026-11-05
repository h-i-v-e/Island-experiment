#ifndef MOTU_REED_CLUMP_COMMON_INCLUDED
#define MOTU_REED_CLUMP_COMMON_INCLUDED

// Canonical clumps use compact per-instance values; baked tile meshes keep
// their vertex colours and normalized island-root UV unchanged.
#if defined(MOTU_REED_CLUMPS)
UNITY_INSTANCING_BUFFER_START(MotuReedClumps)
    UNITY_DEFINE_INSTANCED_PROP(float4, _ReedInstanceData)
    UNITY_DEFINE_INSTANCED_PROP(float4, _ReedGround)
UNITY_INSTANCING_BUFFER_END(MotuReedClumps)
#endif

float4 MotuReedData(float4 vertexData)
{
#if defined(MOTU_REED_CLUMPS)
    return UNITY_ACCESS_INSTANCED_PROP(MotuReedClumps, _ReedInstanceData);
#else
    return vertexData;
#endif
}

float3 MotuReedRootWorldPosition(float2 root)
{
#if defined(MOTU_REED_CLUMPS)
    return TransformObjectToWorld(float3(0, 0, 0));
#else
    float2 rootLocal = (root - 0.5) * _WorldSize;
    return TransformObjectToWorld(float3(rootLocal.x, 0, rootLocal.y));
#endif
}

float3 MotuReedWorldPosition(float3 vertex, float2 uv, float2 root, float4 data)
{
#if defined(MOTU_REED_CLUMPS)
    float2 slope = UNITY_ACCESS_INSTANCED_PROP(MotuReedClumps, _ReedGround).xy;
    // Each base edge follows the bank plane, blending back to an upright tip.
    vertex.y += dot(vertex.xz, slope) * (1.0 - uv.y);
#endif
    float3 worldPosition = TransformObjectToWorld(vertex);
    float2 rootWorld = MotuReedRootWorldPosition(root).xz;
    float3 wind = MotuWindSample(rootWorld);
    float flexibility = lerp(1.0, 0.35, saturate(data.z));
    worldPosition.xz += wind.xz * (MotuWindDisplacementStrength()
        * wind.y * flexibility * uv.y * uv.y);
    return worldPosition;
}

float MotuReedHash(float value)
{
    return frac(sin(value * 91.3458 + 17.13) * 47453.5453);
}

float MotuReedSilhouette(float2 uv, float4 data)
{
    float silhouette = 0.0;
    [unroll]
    for (int stem = 0; stem < 7; stem++)
    {
        float seed = stem + data.w * 19.0 + data.y * 37.0 + data.x * 11.0;
        float centre = (stem + 0.5) / 7.0 + (MotuReedHash(seed) - 0.5) * 0.075;
        // Keep enough unused card height for a complete tapered tip and
        // seed head. Reaching uv.y == 1 exposes the card's flat top edge.
        float stemHeight = lerp(0.58, 0.88, MotuReedHash(seed + 2.7));
        stemHeight *= lerp(1.0, 0.78, data.x);
        float lean = (MotuReedHash(seed + 7.1) - 0.5) * 0.12 * uv.y;
        float stemWidth = lerp(0.010, 0.025, MotuReedHash(seed + 4.2));
        stemWidth *= lerp(0.55, 1.0, data.x);
        float heightProgress = saturate(uv.y / max(stemHeight, 0.01));
        float taper = lerp(
            1.0,
            0.03,
            smoothstep(0.55, 1.0, heightProgress));
        float stemMask = 1.0 - smoothstep(
            stemWidth * taper,
            stemWidth * taper + fwidth(uv.x) * 1.5,
            abs(uv.x - centre - lean));
        stemMask *= 1.0 - smoothstep(0.92, 1.0, heightProgress);
        stemMask *= 1.0 - smoothstep(stemHeight, stemHeight + fwidth(uv.y) * 2.0, uv.y);
        stemMask *= step(0.015, uv.y);
        silhouette = max(silhouette, stemMask);

        // Tall inner-bank reeds receive small seed heads; shorter outer
        // rushes remain blade-only.
        float2 headDelta = float2(
            (uv.x - centre - lean) / 0.035,
            (uv.y - stemHeight + 0.015) / 0.075);
        float head = 1.0 - smoothstep(0.70, 1.0, dot(headDelta, headDelta));
        silhouette = max(silhouette, head * (1.0 - data.x));
    }
    return silhouette;
}

#endif
