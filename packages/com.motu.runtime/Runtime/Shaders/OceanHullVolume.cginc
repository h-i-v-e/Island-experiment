#ifndef MOTU_OCEAN_HULL_VOLUME_INCLUDED
#define MOTU_OCEAN_HULL_VOLUME_INCLUDED

// Render geometry only. OceanSurfaceQuery/buoyancy deliberately never include this.
int _MotuHullCount;
float4 _MotuHullPlanes[512];
float4 _MotuHullMinimum[4]; // XYZ world AABB, W face count
float4 _MotuHullMaximum[4]; // XYZ world AABB, W unused

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
        // World-space plane arithmetic loses sub-millimetre precision far
        // from the origin. Keep grazing intersections at the narrow hull tips.
        if (bottom > top + .002) return false;
    }
    top = max(top, bottom);
    return true;
}

float MotuClampToConvexHulls(float3 position, float seaPlaneHeight)
{
    // Compare with the undisplaced ocean plane, not a fixed world-space zero.
    // Preserve troughs and vertices exactly on the plane.
    if (position.y <= seaPlaneHeight) return position.y;
    float height = position.y;
    [loop]
    for (int hull = 0; hull < _MotuHullCount; hull++)
    {
        float bottom, top;
        if (MotuHullVerticalInterval(position.xz, hull, bottom, top))
            // Project a downward ray onto the underside, never onto the deck.
            // Only crests above the mean sea plane reach this projection. This is a dry-ship visual,
            // not a simulation of waves overtopping and flooding the deck.
            height = min(height, bottom + min(.03, (top - bottom) * .5));
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
    // Also remove triangle interiors crossing the dry hull volume.
    if (MotuInsideConvexHull(position)) clip(-1);
}
#endif
