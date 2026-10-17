#ifndef MOTU_OCEAN_HULL_VOLUME_INCLUDED
#define MOTU_OCEAN_HULL_VOLUME_INCLUDED

// Render geometry only. OceanSurfaceQuery/buoyancy deliberately never include this.
int _MotuHullCount;
float4 _MotuHullPlanes[512];
float4 _MotuHullMinimum[4]; // XYZ world AABB, W face count
float4 _MotuHullMaximum[4]; // XYZ world AABB, W surface inset

bool MotuHullVerticalInterval(float2 xz, int hull, out float bottom, out float top)
{
    float4 lo = _MotuHullMinimum[hull];
    float4 hi = _MotuHullMaximum[hull];
    bottom = lo.y;
    top = hi.y;
    [branch]
    if (any(xz < lo.xz) || any(xz > hi.xz)) return false;
    int count = (int)lo.w;
    [loop]
    for (int face = 0; face < count; face++)
    {
        float4 plane = _MotuHullPlanes[hull * 128 + face];
        float distance = dot(plane.xz, xz) + plane.w;
        if (abs(plane.y) < .00001)
        {
            if (distance > 0) return false;
        }
        else
        {
            float height = -distance / plane.y;
            if (plane.y < 0) bottom = max(bottom, height);
            else top = min(top, height);
        }
        if (bottom > top) return false;
    }
    return bottom <= top;
}

float MotuClampToConvexHulls(float3 position)
{
    float height = position.y;
    [loop]
    for (int hull = 0; hull < _MotuHullCount; hull++)
    {
        float bottom, top;
        if (MotuHullVerticalInterval(position.xz, hull, bottom, top))
            // Project a downward ray onto the underside, never onto the deck.
            // Leave troughs already below the hull alone. This is a dry-ship visual,
            // not a simulation of waves overtopping and flooding the deck.
            height = min(height, bottom + min(_MotuHullMaximum[hull].w, (top - bottom) * .5));
    }
    return height;
}

bool MotuInsideConvexHull(float3 position)
{
    [loop]
    for (int hull = 0; hull < _MotuHullCount; hull++)
    {
        float4 lo = _MotuHullMinimum[hull];
        float4 hi = _MotuHullMaximum[hull];
        [branch]
        if (any(position < lo.xyz) || any(position > hi.xyz)) continue;
        bool inside = true;
        [loop]
        for (int face = 0; face < (int)lo.w; face++)
        {
            float4 plane = _MotuHullPlanes[hull * 128 + face];
            if (dot(plane.xyz, position) + plane.w > 0) { inside = false; break; }
        }
        if (inside) return true;
    }
    return false;
}

void MotuClipHullInterior(float3 position)
{
    // Vertex clamping cannot prevent triangle interiors crossing a convex hull.
    if (MotuInsideConvexHull(position)) clip(-1);
}
#endif
