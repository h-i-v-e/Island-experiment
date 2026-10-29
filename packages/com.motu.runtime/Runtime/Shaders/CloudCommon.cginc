#ifndef MOTU_CLOUD_COMMON_INCLUDED
#define MOTU_CLOUD_COMMON_INCLUDED

sampler2D _MotuCloudWeatherTex;
float _MotuCloudEnabled;
float _MotuCloudCoverage;
float _MotuCloudDensity;
float _MotuCloudAltitude;
float _MotuCloudWorldSize;
float4 _MotuCloudVolume;
float4 _MotuCloudBroadNoise;
float4 _MotuCloudDetailErosion;
float4 _MotuCloudWindOffset;
fixed4 _MotuCloudDayColor;
fixed4 _MotuCloudSunsetColor;
fixed4 _MotuCloudNightColor;
float _MotuCloudShadowStrength;
float _MotuCloudAmbientShadowStrength;
float _MotuCloudCelestialStrength;
float _MotuCloudLowElevationFade;
float4 _MotuCloudLightDirection;
float4 _MotuEnvironmentWorldOffset;
float _MotuCloudLightActive;
float4 _MotuCloudSecondaryLightDirection;
fixed4 _MotuCloudSecondaryLightColor;
float _MotuCloudSecondaryLightActive;
fixed4 _MotuCloudLightColor;
fixed4 _MotuCloudAmbientColor;
float _MotuCloudSunsetStrength;
float _MotuCloudNightStrength;

sampler3D _MotuCloudShapeTex;
sampler3D _MotuCloudErosionTex;
float4 _MotuCloudShape;
float4 _MotuCloudMotion;
float4 _MotuCloudLighting;
float4 _MotuCloudNoiseOffset;
float _MotuCloudDomeRadius;

// Daytime readability must not become a source of illumination after sunset.
half MotuDaylightAmbientFloor(half floorBrightness)
{
    return floorBrightness * (1.0h - saturate(_MotuCloudNightStrength));
}

float3 MotuCloudWorldToLocal(float3 worldPosition)
{
    return worldPosition + _MotuEnvironmentWorldOffset.xyz;
}

float2 MotuCloudWeatherUv(float2 localPosition)
{
    // Subtract travel so cloud features move towards the configured wind direction.
    return (localPosition - _MotuCloudWindOffset.xy)
        / max(_MotuCloudWorldSize, 1.0);
}

float MotuCloudHash(float2 cell)
{
    float3 value = frac(float3(cell.xyx) * 0.1031);
    value += dot(value, value.yzx + 33.33);
    return frac((value.x + value.y) * value.z);
}

half MotuCloudBroadDensity(float2 localPosition)
{
    float broadWorldSize = max(
        _MotuCloudWorldSize * max(_MotuCloudBroadNoise.x, 2.0),
        1.0);
    float2 position = (localPosition - _MotuCloudWindOffset.zw)
        / broadWorldSize;
    // Avoid the all-zero hash at the world origin creating a permanent clear
    // weather region around the starting ship. The field still drifts freely.
    position += float2(71.3, 97.1);
    float2 cell = floor(position);
    float2 blend = frac(position);
    blend = blend * blend * (3.0 - 2.0 * blend);
    float lower = lerp(
        MotuCloudHash(cell),
        MotuCloudHash(cell + float2(1.0, 0.0)),
        blend.x);
    float upper = lerp(
        MotuCloudHash(cell + float2(0.0, 1.0)),
        MotuCloudHash(cell + float2(1.0, 1.0)),
        blend.x);
    return (half)lerp(lower, upper, blend.y);
}

half MotuCloudDensityAtLocalPosition(float2 localPosition)
{
    if (_MotuCloudEnabled < 0.5)
        return 0.0h;

    // Explicit LOD permits coherent dynamic marching on Metal: implicit
    // texture derivatives would force the nested view/light loops to unroll.
    half4 weather = tex2Dlod(
        _MotuCloudWeatherTex,
        float4(MotuCloudWeatherUv(localPosition), 0.0, 0.0));
    half detailStrength = saturate(_MotuCloudDetailErosion.x);
    half erosionStrength = saturate(_MotuCloudDetailErosion.y);
    half structure = lerp(weather.r, weather.r * 0.58h + weather.g * 0.42h, detailStrength);
    structure -= (weather.b - 0.5h) * erosionStrength * 0.44h;
    // Fractal weather channels cluster around one half. Expand that range so
    // low coverage produces isolated clouds instead of an entirely empty sky.
    structure = smoothstep(0.20h, 0.80h, structure);
    half broadDensity = MotuCloudBroadDensity(localPosition);
    half broadStrength = saturate(_MotuCloudBroadNoise.y);
    half regionalCoverage = saturate(
        _MotuCloudCoverage
        + (weather.a - 0.5h) * 0.34h
        + (broadDensity - 0.5h) * broadStrength * 0.72h);
    half threshold = 1.0h - regionalCoverage;
    half edgeWidth = lerp(0.04h, 0.12h, erosionStrength);
    return smoothstep(threshold - edgeWidth, threshold + edgeWidth, structure);
}

half MotuCloudOpticalTransmittance(half density, half strength)
{
    // 1.442695 converts a natural exponential coefficient to exp2.
    return exp2(-density
        * max(_MotuCloudDensity, 0.0)
        * max(strength, 0.0h)
        * ((_MotuCloudVolume.x > 0.0 ? max(_MotuCloudVolume.x, 25.0) : 280.0) / 280.0)
        * 1.442695h);
}

half MotuCloudSurfaceDensityInDirection(float3 worldPosition, float3 direction)
{
    if (_MotuCloudEnabled < 0.5)
        return 0.0h;

    float3 localPosition = MotuCloudWorldToLocal(worldPosition);
    float3 lightDirection = normalize(direction + float3(0.0, 0.000001, 0.0));
    float heightToCloud = _MotuCloudAltitude - localPosition.y;
    if (heightToCloud <= 0.0 || lightDirection.y <= 0.0001)
        return 0.0h;

    float travel = heightToCloud / lightDirection.y;
    float2 cloudPosition = localPosition.xz + lightDirection.xz * travel;
    half elevationFade = smoothstep(
        0.0,
        max(_MotuCloudLowElevationFade, 0.001),
        lightDirection.y);
    return MotuCloudDensityAtLocalPosition(cloudPosition) * elevationFade;
}

half MotuCloudSurfaceDensity(float3 worldPosition)
{
    return _MotuCloudLightActive < 0.5 ? 0.0h
        : MotuCloudSurfaceDensityInDirection(worldPosition, _MotuCloudLightDirection.xyz);
}

struct MotuCloudLighting
{
    half directTransmittance;
    half ambientTransmittance;
};

MotuCloudLighting MotuCloudSurfaceLightingForDirection(float3 worldPosition, float3 lightDirection)
{
    half density = MotuCloudSurfaceDensityInDirection(worldPosition, lightDirection);
    half optical = MotuCloudOpticalTransmittance(density, 1.0h);
    MotuCloudLighting lighting;
    lighting.directTransmittance = lerp(
        1.0h,
        optical,
        saturate(_MotuCloudShadowStrength));
    lighting.ambientTransmittance = 1.0h
        - density * saturate(_MotuCloudAmbientShadowStrength);
    return lighting;
}

MotuCloudLighting MotuCloudSurfaceLighting(float3 worldPosition)
{
    // Legacy callers shade the URP main light. The controller keeps the primary
    // celestial slot aligned with that source while the secondary stays active.
    return MotuCloudSurfaceLightingForDirection(worldPosition,
        _MotuCloudLightActive > 0.5 ? _MotuCloudLightDirection.xyz : float3(0.0, -1.0, 0.0));
}

// A fixed reference length keeps density a mass-per-metre control. Increasing
// physical thickness must increase extinction instead of cancelling it out.
float MotuCloudLayerThickness()
{
    return _MotuCloudVolume.x > 0.0 ? max(_MotuCloudVolume.x, 25.0) : 280.0;
}

float MotuCloudRadius()
{
    return _MotuCloudDomeRadius > 0.0 ? _MotuCloudDomeRadius : 2000.0;
}

float MotuCloudLayerBaseAt(float2 position, float2 centre, float radius)
{
    float baseHeight = _MotuCloudAltitude - MotuCloudLayerThickness() * 0.5;
    float2 offset = (position - centre) / max(radius, 1.0);
    // Centre the complete volume on sea level at the rim. Its carved base
    // extends below the sea instead of stopping above a clear horizon strip.
    return baseHeight - max(_MotuCloudAltitude, 0.0) * saturate(dot(offset, offset));
}

float MotuCloudHeightFraction(float3 position, float2 centre, float radius)
{
    return (position.y - MotuCloudLayerBaseAt(position.xz, centre, radius)) / MotuCloudLayerThickness();
}

half MotuCloudVolumeDensity(float3 position, float2 centre, float radius)
{
    float height = MotuCloudHeightFraction(position, centre, radius);
    if (height <= 0.0 || height >= 1.0 || _MotuCloudEnabled < 0.5)
        return 0.0h;
    half cumulus = saturate(_MotuCloudShape.z * 2.0);
    half storm = saturate((_MotuCloudShape.z - 0.5) * 2.0);
    half topStart = lerp(lerp(0.86h, 0.48h, cumulus), 0.68h, storm);
    half baseRamp = lerp(lerp(0.055h, 0.42h, cumulus), 0.50h, storm);
    half profile = smoothstep(0.0h, baseRamp, (half)height)
        * (1.0h - smoothstep(topStart, 1.0h, (half)height));
    half weather = MotuCloudDensityAtLocalPosition(position.xz);
    half density = weather * profile;
    half shapeStrength = saturate(_MotuCloudShape.w);
    if (shapeStrength <= 0.0001h || density <= 0.0001h)
        return density;

    float billowSize = max(_MotuCloudShape.x, 50.0);
    float erosionSize = max(_MotuCloudShape.y, 10.0);
    float2 flow = _MotuCloudNoiseOffset.xy;
    float shear = (height - 0.5) * saturate(_MotuCloudMotion.x);
    float evolution = _Time.y * saturate(_MotuCloudMotion.y);
    float3 advected = float3(position.xz - flow * (1.0 + shear), position.y);
    // Cache textures contain three/four billows per tile. The small vertical
    // phase evolves the shape much more slowly than the wind transports it.
    float3 shapeUv = advected / (billowSize * 3.0)
        + evolution * float3(0.0013, -0.0009, 0.003);
    half3 shape = tex3Dlod(_MotuCloudShapeTex, float4(shapeUv, 0)).rgb;
    half billows = smoothstep(0.22h, 0.72h, shape.r * 0.62h + shape.g * 0.48h);
    // Carve the volume itself, including its underside. Blending a carved
    // shape back into an uncarved slab leaves a translucent flat sheet below
    // every billow and removes all relief in fully covered storm weather.
    half carve = (1.0h - billows) * lerp(0.18h, 0.70h, cumulus) * shapeStrength;
    half sculpted = saturate((density - carve) / max(1.0h - carve * 0.4h, 0.25h));
    float3 detailUv = advected / (erosionSize * 4.0)
        + evolution * float3(-0.004, 0.003, 0.005);
    half erosion = 1.0h - tex3Dlod(_MotuCloudErosionTex, float4(detailUv, 0)).r;
    sculpted = saturate(sculpted - erosion * saturate(_MotuCloudDetailErosion.y)
        * shapeStrength * (1.0h - sculpted) * 0.32h);
    return sculpted;
}

// Stable quadratic roots prevent cancellation for almost vertical rays.
bool MotuCloudRoots(float a, float b, float c, out float lower, out float upper)
{
    lower = upper = 0.0;
    float discriminant = b * b - 4.0 * a * c;
    if (discriminant < 0.0) return false;
    float root = sqrt(max(discriminant, 0.0));
    float q = -0.5 * (b + (b >= 0.0 ? root : -root));
    // Callers use the linear path at zero curvature. Keep the divisor finite
    // here too, since shader compilers may evaluate a predicated branch.
    float first = q / max(a, 0.000000000001);
    float second = abs(q) > 0.0000001 ? c / q : first;
    lower = min(first, second); upper = max(first, second);
    return true;
}

// Intersect 0 <= a*t*t + b*t + c <= thickness analytically. A ray can cross
// both near and far sides of a curved layer, so retain both occupied spans.
float4 MotuCloudBandIntervals(float a, float b, float c, float thickness, float minimum, float maximum)
{
    float4 result = 0.0;
    if (maximum <= minimum) return result;
    if (a < 0.00000001)
    {
        if (abs(b) < 0.000001)
        {
            if (c >= 0.0 && c <= thickness) result.xy = float2(minimum, maximum);
            return result;
        }
        float first = -c / b;
        float second = (thickness - c) / b;
        float entry = max(minimum, min(first, second));
        float exit = min(maximum, max(first, second));
        if (exit > entry) result.xy = float2(entry, exit);
        return result;
    }
    float lower, upper;
    if (!MotuCloudRoots(a, b, c - thickness, lower, upper)) return result;
    float entry = max(minimum, lower), exit = min(maximum, upper);
    if (exit <= entry) return result;
    float baseLower, baseUpper;
    if (!MotuCloudRoots(a, b, c, baseLower, baseUpper))
    {
        result.xy = float2(entry, exit);
        return result;
    }
    float nearExit = min(exit, baseLower);
    float farEntry = max(entry, baseUpper);
    if (nearExit > entry) result.xy = float2(entry, nearExit);
    if (exit > farEntry)
    {
        if (result.y > result.x) result.zw = float2(farEntry, exit);
        else result.xy = float2(farEntry, exit);
    }
    return result;
}

float4 MotuCloudRayIntervals(float3 origin, float3 direction, float2 centre, float radius, float maximum)
{
    float thickness = MotuCloudLayerThickness();
    float baseHeight = _MotuCloudAltitude - thickness * 0.5;
    float drop = max(_MotuCloudAltitude, 0.0);
    float2 offset = origin.xz - centre;
    float inverseRadiusSquared = 1.0 / max(radius * radius, 1.0);
    float a = drop * dot(direction.xz, direction.xz) * inverseRadiusSquared;
    float b = direction.y + 2.0 * drop * dot(offset, direction.xz) * inverseRadiusSquared;
    float c = origin.y - baseHeight + drop * dot(offset, offset) * inverseRadiusSquared;
    // Outside the dome footprint the volume remains centred on sea level.
    float radialA = dot(direction.xz, direction.xz);
    float radialB = 2.0 * dot(offset, direction.xz);
    float radialC = dot(offset, offset) - radius * radius;
    float radialLower, radialExit;
    float curvedEnd = maximum;
    if (radialA > 0.000001 && MotuCloudRoots(radialA, radialB, radialC, radialLower, radialExit))
        curvedEnd = clamp(radialExit, 0.0, maximum);
    float4 spans = MotuCloudBandIntervals(a, b, c, thickness, 0.0, curvedEnd);
    if (curvedEnd < maximum)
    {
        float4 continuation = MotuCloudBandIntervals(0.0, direction.y,
            origin.y - (baseHeight - drop), thickness, curvedEnd, maximum);
        if (continuation.y > continuation.x)
        {
            if (spans.y <= spans.x) spans.xy = continuation.xy;
            else if (spans.w > spans.z) spans.w = max(spans.w, continuation.y);
            else if (continuation.x <= spans.y + 0.01) spans.y = continuation.y;
            else spans.zw = continuation.xy;
        }
    }
    return spans;
}

half MotuCloudLightTransmittance(float3 position, float2 centre, float radius, float3 lightDirection, half active)
{
    if (active < 0.5) return 0.0h;
    float3 direction = normalize(lightDirection + float3(0.0, 0.000001, 0.0));
    float4 spans = MotuCloudRayIntervals(position, direction, centre, radius, radius * 2.0);
    int count = _MotuCloudMotion.z < 0.5 ? 2 : (_MotuCloudMotion.z > 1.5 ? 5 : 3);
    float opticalDepth = 0.0;
    [loop]
    for (int span = 0; span < 2; span++)
    {
        float entry = span == 0 ? spans.x : spans.z;
        float exit = span == 0 ? spans.y : spans.w;
        float stepLength = (exit - entry) / count;
        if (stepLength <= 0.0) continue;
        [loop]
        for (int sampleIndex = 0; sampleIndex < 5; sampleIndex++)
        {
            if (sampleIndex >= count) break;
            float distance = entry + (sampleIndex + 0.5) * stepLength;
            opticalDepth += MotuCloudVolumeDensity(position + direction * distance, centre, radius) * stepLength;
        }
    }
    float penetration = _MotuCloudLighting.x > 0.0 ? _MotuCloudLighting.x : 1.0;
    return exp2(-opticalDepth * max(_MotuCloudDensity, 0.0)
        / (280.0 * penetration) * 1.442695);
}

half MotuCloudSunTransmittance(float3 position, float2 centre, float radius, float heightFraction)
{
    return MotuCloudLightTransmittance(position, centre, radius,
        _MotuCloudLightDirection.xyz, _MotuCloudLightActive);
}

fixed3 MotuCloudScatteringFromLight(float3 viewDirection, half sunlight, half column,
    float3 lightDirection, fixed3 lightColour)
{
    lightDirection = normalize(lightDirection + float3(0.0, 0.000001, 0.0));
    half cosine = dot(viewDirection, lightDirection);
    const half anisotropy = 0.55h;
    half phase = (1.0h - anisotropy * anisotropy)
        / pow(max(1.0h + anisotropy * anisotropy - 2.0h * anisotropy * cosine, 0.08h), 1.5h);
    return lightColour * (sunlight * (0.70h + phase * 0.15h)
        + (1.0h - sunlight) * exp2(-column * 0.35h) * 0.16h);
}

fixed3 MotuCloudSkyColour(half density, float3 viewDirection, half sunlight,
    half secondaryLight, half heightFraction)
{
    half night = saturate(_MotuCloudNightStrength);
    half sunset = saturate(_MotuCloudSunsetStrength) * (1.0h - night);
    fixed3 albedo = lerp(_MotuCloudDayColor.rgb, _MotuCloudSunsetColor.rgb, sunset * 0.45h);
    albedo = lerp(albedo, min(_MotuCloudNightColor.rgb * 8.0h, 3.0h), night);
    half column = density * max(_MotuCloudDensity, 0.0)
        * (MotuCloudLayerThickness() / 280.0) * (1.0h - heightFraction);
    half skyVisibility = exp2(-column * 0.45h);
    fixed3 ambient = _MotuCloudAmbientColor.rgb
        * (0.28h + skyVisibility * 0.72h) * lerp(0.65h, 1.0h, heightFraction);
    // Integrate each direction independently. Slot swaps at the horizon do not
    // replace the other body's existing illumination or count ambient twice.
    fixed3 scattered = MotuCloudScatteringFromLight(viewDirection, sunlight, column,
        _MotuCloudLightDirection.xyz, _MotuCloudLightColor.rgb * step(0.5h, (half)_MotuCloudLightActive));
    scattered += MotuCloudScatteringFromLight(viewDirection, secondaryLight, column,
        _MotuCloudSecondaryLightDirection.xyz,
        _MotuCloudSecondaryLightColor.rgb * step(0.5h, (half)_MotuCloudSecondaryLightActive));
    return albedo * (ambient + scattered);
}

struct MotuCloudSkyVolume
{
    half transmittance;
    fixed3 averageColour;
};

MotuCloudSkyVolume MotuCloudSkyVolumeAt(
    float3 cameraLocalPosition,
    float3 localViewDirection,
    float maximumDistance,
    float2 screenPosition)
{
    MotuCloudSkyVolume result;
    result.transmittance = 1.0h;
    result.averageColour = 0.0h;
    if (_MotuCloudEnabled < 0.5 || _MotuCloudDensity <= 0.0)
        return result;
    float3 direction = normalize(localViewDirection);
    float radius = MotuCloudRadius();
    float2 centre = cameraLocalPosition.xz;
    float4 spans = MotuCloudRayIntervals(cameraLocalPosition, direction, centre, radius, max(maximumDistance, 0.0));
    float totalLength = max(0.0, spans.y - spans.x) + max(0.0, spans.w - spans.z);
    if (totalLength <= 0.0) return result;
    int budget = _MotuCloudMotion.z < 0.5 ? 12 : (_MotuCloudMotion.z > 1.5 ? 32 : 20);
    // Static screen-space jitter is stable when the ship stops. Independent
    // 3D noise evolves in world space instead of shuffling strata every frame.
    float phase = frac(52.9829189 * frac(dot(floor(screenPosition), float2(0.06711056, 0.00583715))));
    fixed3 radiance = 0.0h;
    half transmittance = 1.0h;
    [loop]
    for (int span = 0; span < 2; span++)
    {
        float entry = span == 0 ? spans.x : spans.z;
        float exit = span == 0 ? spans.y : spans.w;
        if (exit <= entry) continue;
        int count = max(2, (int)ceil(budget * (exit - entry) / totalLength));
        float stepLength = (exit - entry) / count;
        [loop]
        for (int sampleIndex = 0; sampleIndex < 32; sampleIndex++)
        {
            if (sampleIndex >= count || transmittance < 0.006h) break;
            float distance = entry + (sampleIndex + phase) * stepLength;
            float3 position = cameraLocalPosition + direction * distance;
            half density = MotuCloudVolumeDensity(position, centre, radius);
            if (density <= 0.001h) continue;
            half opacity = 1.0h - exp2(-density * max(_MotuCloudDensity, 0.0)
                * (stepLength / 280.0) * 1.442695h);
            half heightFraction = saturate(MotuCloudHeightFraction(position, centre, radius));
            half sunlight = MotuCloudSunTransmittance(position, centre, radius, heightFraction);
            half secondaryLight = MotuCloudLightTransmittance(position, centre, radius,
                _MotuCloudSecondaryLightDirection.xyz, _MotuCloudSecondaryLightActive);
            fixed3 colour = MotuCloudSkyColour(density, direction, sunlight, secondaryLight, heightFraction);
            radiance += transmittance * opacity * colour;
            transmittance *= 1.0h - opacity;
        }
    }
    result.transmittance = transmittance;
    result.averageColour = radiance / max(1.0h - transmittance, 0.0001h);
    return result;
}

#endif
