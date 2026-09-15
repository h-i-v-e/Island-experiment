#ifndef MOTU_RIVER_SURFACE_INCLUDED
#define MOTU_RIVER_SURFACE_INCLUDED

sampler2D _NoiseTex;
float4 _NoiseTex_TexelSize;
float _CoarseNoiseWorldSize, _FineNoiseWorldSize;
float _CoarseFlowSpeed, _FineFlowSpeed;
float _RippleStrength, _RapidRoughness;
float _FoamWorldSize, _FoamStretch;
float _WaterfallFlowSpeed;

float2 MotuRiverNoiseSlope(float2 uv, bool fineBand)
{
    float2 texel = max(abs(_NoiseTex_TexelSize.xy), 1.0 / 4096.0);
    float2 dx = ddx(uv) / texel, dy = ddy(uv) / texel;
    float lod = max(1, .5 * log2(max(max(dot(dx, dx), dot(dy, dy)), 1)));
    float2 stepUv = texel * 2;
    float2 u = tex2Dlod(_NoiseTex, float4(uv + float2(stepUv.x, 0), 0, lod)).rg
        - tex2Dlod(_NoiseTex, float4(uv - float2(stepUv.x, 0), 0, lod)).rg;
    float2 v = tex2Dlod(_NoiseTex, float4(uv + float2(0, stepUv.y), 0, lod)).rg
        - tex2Dlod(_NoiseTex, float4(uv - float2(0, stepUv.y), 0, lod)).rg;
    return fineBand ? float2(u.g, v.g) : float2(u.r, v.r);
}

float3 MotuRiverDetailNormal(float3 position, float3 baseNormal, float2 riverMetres,
    float timeSeconds, float rapids, out float roughness)
{
    float3 normal = normalize(baseNormal);
    float3 dx = ddx(position), dy = ddy(position);
    float2 du = ddx(riverMetres), dv = ddy(riverMetres);
    float3 across = cross(dy, normal), along = cross(normal, dx);
    float determinant = dot(dx, across);
    // Cotangent gradients preserve downstream direction through bends, mirrored
    // bank-distance coordinates and vertical waterfall faces. No global wind.
    float3 uGradient = across * du.x + along * dv.x;
    float3 vGradient = across * du.y + along * dv.y;
    float gradientScale = max(max(length(uGradient), length(vGradient)), 1.0e-12);
    float orientation = determinant >= 0 ? 1 : -1;
    uGradient *= orientation / gradientScale;
    vGradient *= orientation / gradientScale;
    float coarseSize = max(_CoarseNoiseWorldSize, .1);
    float fineSize = max(_FineNoiseWorldSize, .1);
    // The river noise contains 32 cells per repeat: scales are cell sizes in m.
    float2 coarseUv = (riverMetres - float2(0, timeSeconds * _CoarseFlowSpeed)) / (coarseSize * 32);
    float2 fineUv = (riverMetres - float2(0, timeSeconds * _FineFlowSpeed)) / (fineSize * 32) + float2(.371, .619);
    float footprint = max(length(dx), length(dy));
    float2 resolved = 1 - smoothstep(float2(coarseSize, fineSize) * .3,
        float2(coarseSize, fineSize) * 2, footprint);
    float2 slope = MotuRiverNoiseSlope(coarseUv, false) * resolved.x
        + MotuRiverNoiseSlope(fineUv, true) * resolved.y * .25;
    float strength = max(_RippleStrength, 0) * lerp(1, 1.5, rapids);
    normal = normalize(normal - (uGradient * slope.x + vGradient * slope.y) * strength * 1.5);
    float variance = max(dot(ddx(normal), ddx(normal)), dot(ddy(normal), ddy(normal)));
    roughness = clamp(sqrt(pow(max(_SurfaceRoughness, .04) + rapids * max(_RapidRoughness, 0), 2)
        + strength * strength * (1 - resolved.y * resolved.y) * .5 + min(variance, .2)), .04, .8);
    return normal;
}

float2 MotuRiverWaterfallNoise(float2 riverMetres, float timeSeconds)
{
    // Restore the original fine, fast falling-water pattern. These are full
    // texture repeats (32 noise cells), separate from calm-river wavelengths.
    float speed = _WaterfallFlowSpeed;
    float2 coarseUv = (riverMetres - float2(0, timeSeconds * speed * .25)) / 6;
    float2 fineUv = (riverMetres - float2(0, timeSeconds * speed)) / 3;
    return float2(tex2D(_NoiseTex, coarseUv).r, tex2D(_NoiseTex, fineUv).g);
}

float2 MotuRiverDepthDistortion(float2 riverMetres, float timeSeconds, float rapids)
{
    // Refraction needs appreciable moving offsets even on a gently lit surface.
    // Keep this independent of the subtle normal perturbation used for glints.
    float2 coarseUv = (riverMetres - float2(0, timeSeconds * _CoarseFlowSpeed))
        / (max(_CoarseNoiseWorldSize, .1) * 32);
    float2 fineUv = (riverMetres - float2(0, timeSeconds * _FineFlowSpeed))
        / (max(_FineNoiseWorldSize, .1) * 32) + float2(.371, .619);
    float2 calm = float2(tex2D(_NoiseTex, coarseUv).r, tex2D(_NoiseTex, fineUv).g);
    return lerp(calm, MotuRiverWaterfallNoise(riverMetres, timeSeconds), rapids) - .5;
}

float MotuRiverFoam(float2 riverMetres, float timeSeconds, float rapids)
{
    // Long patches travel along the generated channel coordinates; the weaker
    // faster band breaks their edges up without flashing random pixels.
    float span = max(_FoamWorldSize, .1) * 32;
    float2 uv = (riverMetres - float2(0, timeSeconds * _CoarseFlowSpeed))
        / float2(span, span * max(_FoamStretch, 1));
    float2 detailUv = (riverMetres - float2(0, timeSeconds * _FineFlowSpeed))
        / (span * .65) + float2(.173, .419);
    float patch = tex2D(_NoiseTex, uv).r;
    float detail = tex2D(_NoiseTex, detailUv).g;
    float foam = smoothstep(.43, .72, patch * .8 + detail * .2);
    float2 fallingNoise = MotuRiverWaterfallNoise(riverMetres, timeSeconds);
    float fallingFoam = smoothstep(.42, .68, fallingNoise.x) * .03
        + smoothstep(.24, .80, fallingNoise.y) * .65;
    return lerp(foam * .025, max(foam * .55, fallingFoam), rapids);
}
#endif
