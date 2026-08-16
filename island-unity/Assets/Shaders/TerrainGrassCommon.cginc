#include "UnityCG.cginc"
#include "Lighting.cginc"
#include "AutoLight.cginc"

struct GrassVertexInput
{
    float4 vertex : POSITION;
    float3 normal : NORMAL;
    float4 material : COLOR;
};

struct GrassVertexOutput
{
    float4 pos : SV_POSITION;
    float3 worldPosition : TEXCOORD0;
    half3 worldNormal : TEXCOORD1;
    half3 material : TEXCOORD2;
    float3 surfaceWorldPosition : TEXCOORD3;
    SHADOW_COORDS(4)
    UNITY_FOG_COORDS(5)
};

sampler3D _CliffNoise3D;
sampler2D _GrassPatchNoise;
half _GrassEnabled;
float3 _GrassPlayerPosition;
float _GrassRadius;
float _GrassFadeWidth;
float _GrassHeight;
float _GrassDensity;
half _GrassBladeWidth;
fixed4 _GrassRootColor;
fixed4 _GrassTipColor;
float _GrassPatchNoiseWorldSize;
half _GrassBrightness;
float3 _GrassLightDirection;
fixed4 _GrassLightColor;
fixed4 _GrassAmbientColor;
float _SnowLine;
float _SnowEdgeNoiseMetres;
float _SnowMacroNoiseMetres;
float _BeachMaximumElevation;
float _SandPatchNoiseWorldSize;
half _RiverEdgeNoiseStrength;
half _RiverEdgeBlendWidth;
half _CliffNormalCutoff;
half _CliffBoundaryNoiseStrength;
half _RockBoundaryNoiseStrength;
half _SandRockSlopeThreshold;
float _CliffNoisePeriod;
half _RockPatchNoiseDetailScale;

float GrassHash(float2 cell)
{
    float3 value = frac(float3(cell.xyx) * 0.1031);
    value += dot(value, value.yzx + 33.33);
    return frac((value.x + value.y) * value.z);
}

float GrassValueNoise(float2 position)
{
    float2 cell = floor(position);
    float2 blend = frac(position);
    blend = blend * blend * blend * (blend * (blend * 6.0 - 15.0) + 10.0);
    float bottom = lerp(
        GrassHash(cell),
        GrassHash(cell + float2(1.0, 0.0)),
        blend.x);
    float top = lerp(
        GrassHash(cell + float2(0.0, 1.0)),
        GrassHash(cell + 1.0),
        blend.x);
    return lerp(bottom, top, blend.y);
}

half GrassAntialiasedMask(float signedDistance)
{
    float transitionWidth = max(fwidth(signedDistance), 1.0e-4);
    return smoothstep(-transitionWidth, transitionWidth, signedDistance);
}

GrassVertexOutput GrassVertex(GrassVertexInput input)
{
    GrassVertexOutput output;
    half3 worldNormal = normalize(UnityObjectToWorldNormal(input.normal));
    float3 worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
    worldPosition += worldNormal * (_GrassHeight * GRASS_SHELL_LAYER);
    output.pos = UnityWorldToClipPos(worldPosition);
    output.worldPosition = worldPosition;
    output.surfaceWorldPosition = worldPosition
        - worldNormal * (_GrassHeight * GRASS_SHELL_LAYER);
    output.worldNormal = worldNormal;
    output.material = input.material.rgb;
    TRANSFER_SHADOW_WPOS(output, worldPosition);
    UNITY_TRANSFER_FOG(output, output.pos);
    return output;
}

fixed4 GrassFragment(GrassVertexOutput input) : SV_Target
{
    clip(_GrassEnabled - 0.5);

    float playerDistance = distance(
        input.surfaceWorldPosition.xz,
        _GrassPlayerPosition.xz);
    float fadeWidth = min(max(_GrassFadeWidth, 0.001), _GrassRadius);
    half radialWeight = 1.0 - smoothstep(
        _GrassRadius - fadeWidth,
        _GrassRadius,
        playerDistance);

    half3 normal = normalize(input.worldNormal);
    float elevation = input.surfaceWorldPosition.y;
    float noisePeriod = max(_CliffNoisePeriod, 1.0);
    half3 broadNoise = tex3D(
        _CliffNoise3D,
        input.surfaceWorldPosition / noisePeriod).rgb * 2.0 - 1.0;
    half3 macroNoise = tex3D(
        _CliffNoise3D,
        input.surfaceWorldPosition / (noisePeriod * 3.0)
            + float3(0.23, 0.71, 0.41)).rgb * 2.0 - 1.0;
    // Match the ground shader exactly so grass shells end on the same noisy,
    // world-space material boundary instead of exposing mesh edges.
    half cliffBoundaryNoise = clamp(
        broadNoise.r * 0.65 + macroNoise.g * 0.35,
        -1.0,
        1.0);
    half cutoffNormal = normal.y
        - cliffBoundaryNoise * _CliffBoundaryNoiseStrength;
    half cliffWeight = GrassAntialiasedMask(_CliffNormalCutoff - cutoffNormal);
    half forcedRockCoverage = GrassAntialiasedMask(input.material.r - 1.5);
    half hardness = saturate(input.material.r);
    half looseCover = saturate(input.material.g);
    half riverBed = saturate(input.material.b);
    half slope = 1.0 - saturate(normal.y);
    half rockBoundaryNoise = clamp(
        broadNoise.b * 0.55 + macroNoise.b * 0.45,
        -1.0,
        1.0);
    half riverNoise = clamp(
        dot(broadNoise, half3(0.577, -0.577, 0.577)),
        -1.0,
        1.0);
    half riverThreshold = 0.5 + riverNoise * _RiverEdgeNoiseStrength;
    half riverDistance = riverBed - riverThreshold;
    half riverTransition = max(_RiverEdgeBlendWidth, fwidth(riverDistance));
    half riverCoverage = smoothstep(
        -riverTransition,
        riverTransition,
        riverDistance);
    half sandAltitudeRichness = saturate(
        (_BeachMaximumElevation - elevation)
            / max(_BeachMaximumElevation, 0.1));
    half sandRichness = looseCover
        * sandAltitudeRichness
        * (1.0 - riverCoverage);
    float2 sandPatchUv = input.surfaceWorldPosition.xz
        / max(_SandPatchNoiseWorldSize, 0.1)
        + float2(0.37, 0.73);
    half2 sandPatchLayers = tex2D(_GrassPatchNoise, sandPatchUv).rg;
    half sandPatchNoise = sandPatchLayers.r * 0.40
        + sandPatchLayers.g * 0.60;
    half beachCandidateCoverage = GrassAntialiasedMask(
        sandPatchNoise - (1.0 - sandRichness))
        * step(1.0e-4, sandRichness);

    half geologyRockWeight = saturate(slope * lerp(1.3, 3.0, hardness));
    half3 rockPatchLayers = tex3D(
        _CliffNoise3D,
        input.surfaceWorldPosition / noisePeriod * _RockPatchNoiseDetailScale
            + float3(0.67, 0.31, 0.91)).rgb;
    half rockPatchNoise = rockPatchLayers.r * 0.65
        + rockPatchLayers.g * 0.35;
    // Grass is a discrete surface class. Use the same coherent rock field as
    // the ground shader's soft blend, but retain only pixel-width
    // anti-aliasing so fur and the hard grass/rock ground boundary coincide.
    half rockMaskDistance = rockPatchNoise
        - (1.0 - geologyRockWeight);
    half geologyRockCoverage = GrassAntialiasedMask(rockMaskDistance)
        * step(1.0e-4, geologyRockWeight);
    half sandRockThreshold = _SandRockSlopeThreshold
        + rockBoundaryNoise * _RockBoundaryNoiseStrength * 0.25;
    half sandRockCoverage = beachCandidateCoverage
        * GrassAntialiasedMask(slope - sandRockThreshold);
    geologyRockCoverage = max(geologyRockCoverage, sandRockCoverage);
    geologyRockCoverage = max(geologyRockCoverage, forcedRockCoverage);
    half exposedRockCoverage = max(geologyRockCoverage, cliffWeight);
    half beachCoverage = beachCandidateCoverage
        * (1.0 - exposedRockCoverage);
    float noisySnowLine = _SnowLine
        + macroNoise.r * _SnowMacroNoiseMetres
        + broadNoise.g * _SnowEdgeNoiseMetres;
    half snowCoverage = GrassAntialiasedMask(elevation - noisySnowLine);
    // Clip each classification at its visible midpoint. Multiplying the masks
    // and clipping the result at a low threshold left a thin fur fringe after
    // the ground had already become cliff, river, beach, or snow.
    clip(elevation);
    clip(0.5 - exposedRockCoverage);
    clip(0.5 - beachCoverage);
    clip(0.5 - riverCoverage);
    clip(0.5 - snowCoverage);

    // Deposit thickness is the soil-richness signal. A dedicated fine,
    // coherent texture removes every blade at zero richness, then lowers the
    // noise threshold continuously until rich soil has complete coverage.
    float2 patchUv = input.surfaceWorldPosition.xz
        / max(_GrassPatchNoiseWorldSize, 0.1);
    half2 patchLayers = tex2D(_GrassPatchNoise, patchUv).rg;
    half patchNoise = patchLayers.r * 0.65 + patchLayers.g * 0.35;
    clip(looseCover - 1.0e-4);
    clip(patchNoise - (1.0 - looseCover));

    float2 regularCoordinate = input.surfaceWorldPosition.xz
        * max(_GrassDensity, 0.1);
    float2 warpCoordinate = regularCoordinate * 0.18;
    float2 domainWarp = float2(
        GrassValueNoise(warpCoordinate + float2(13.7, 4.3)),
        GrassValueNoise(warpCoordinate + float2(2.1, 19.4))) - 0.5;
    float2 grassCoordinate = regularCoordinate + domainWarp * 1.65;
    float2 cell = floor(grassCoordinate);
    float2 cellOffset = float2(
        GrassHash(cell + 11.3),
        GrassHash(cell + 29.7)) - 0.5;
    float2 cellPosition = frac(grassCoordinate) - 0.5
        - cellOffset * 0.25;
    float leanAngle = GrassHash(cell + float2(7.9, 37.1)) * 6.2831853;
    float2 leanDirection = float2(cos(leanAngle), sin(leanAngle));
    cellPosition -= leanDirection
        * (GRASS_SHELL_LAYER * GRASS_SHELL_LAYER * 0.18);
    // Fade whole blades with stable per-cell dithering. Removing individual
    // shell layers here produces visible rings centred on the player.
    float fadeRandom = GrassHash(cell + float2(41.7, 19.3));
    clip(radialWeight - fadeRandom - 0.001);
    float randomHeight = lerp(0.55, 1.0, GrassHash(cell));
    clip(randomHeight - GRASS_SHELL_LAYER);
    float bladeRadius = lerp(
        _GrassBladeWidth,
        _GrassBladeWidth * 0.55,
        GRASS_SHELL_LAYER);
    float edgeNoise = GrassValueNoise(
        grassCoordinate * 3.5
            + float2(GRASS_SHELL_LAYER * 17.3, GRASS_SHELL_LAYER * 31.7));
    bladeRadius *= lerp(0.72, 1.18, edgeNoise);
    clip(bladeRadius - length(cellPosition));

    half3 lightingNormal = normalize(lerp(normal, half3(0.0, 1.0, 0.0), 0.35));
    half3 lightDirection = normalize(_GrassLightDirection);
    half diffuse = saturate(dot(lightingNormal, lightDirection));
    UNITY_LIGHT_ATTENUATION(
        shadowAttenuation,
        input,
        input.worldPosition);
    half3 direct = _GrassLightColor.rgb * diffuse * shadowAttenuation;
    half3 ambient = _GrassAmbientColor.rgb;
    fixed3 grassColor = lerp(
        _GrassRootColor.rgb,
        _GrassTipColor.rgb,
        GRASS_SHELL_LAYER);
    // Keep the spatial variation neutral on average so the fur has the same
    // mean albedo as the terrain's base grass colour.
    grassColor *= lerp(0.90, 1.10, GrassHash(cell + 17.0));
    grassColor *= _GrassBrightness;
    fixed4 color = fixed4(grassColor * (ambient + direct), 1.0);
    UNITY_APPLY_FOG(input.fogCoord, color);
    return color;
}
