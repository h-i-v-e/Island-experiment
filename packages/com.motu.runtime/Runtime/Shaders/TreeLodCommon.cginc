#ifndef MOTU_TREE_LOD_COMMON_INCLUDED
#define MOTU_TREE_LOD_COMMON_INCLUDED
float _MotuTreeLodEnabled;
float4 _MotuTreeLodRanges;
float3 _MotuTreeLodViewer;

// Each instance occupies an interval in the same dither field. Adjacent
// stages share their boundary exactly, in colour, shadows and reflections.
void MotuTreeLodClip(float2 pixelPosition)
{
    if (_MotuTreeLodEnabled < 0.5) return;
    float3 root = mul(GetObjectToWorldMatrix(), float4(0, 0, 0, 1)).xyz;
    float range = distance(root, _MotuTreeLodViewer);
    float incoming = smoothstep(_MotuTreeLodRanges.x, _MotuTreeLodRanges.y, range);
    float outgoing = smoothstep(_MotuTreeLodRanges.z, _MotuTreeLodRanges.w, range);
    float threshold = frac(52.9829189 * frac(dot(floor(pixelPosition), float2(0.06711056, 0.00583715))));
    threshold = clamp(threshold, 0.0001, 0.9999);
    clip(incoming - threshold);
    clip(threshold - outgoing);
}
#endif
