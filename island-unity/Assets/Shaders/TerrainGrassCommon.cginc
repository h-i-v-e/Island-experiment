#include "UnityCG.cginc"
#include "Lighting.cginc"
#include "AutoLight.cginc"

struct GrassVertexInput
{
    float4 vertex : POSITION;
    float3 normal : NORMAL;
    float2 environment : TEXCOORD1;
    float4 material : COLOR;
};

struct GrassVertexOutput
{
    float4 pos : SV_POSITION;
    float3 worldPosition : TEXCOORD0;
    half3 worldNormal : TEXCOORD1;
    half4 material : TEXCOORD2;
    float3 surfaceWorldPosition : TEXCOORD3;
    SHADOW_COORDS(4)
    UNITY_FOG_COORDS(5)
    float3 islandLocalSurfacePosition : TEXCOORD6;
    half4 windLightingAndForestFloor : TEXCOORD7;
    half stones : TEXCOORD8;
};

sampler3D _CliffNoise3D;
sampler2D _GrassPatchNoise;
sampler2D _ForestStonesMaskMap;
half _GrassEnabled;
float3 _GrassPlayerPosition;
float _GrassRadius;
float _GrassFadeWidth;
float _GrassHeight;
float _GrassDensity;
half _GrassBladeWidth;
fixed4 _GrassColorA;
fixed4 _GrassColorB;
float _GrassColorNoiseWorldSize;
float _GrassPatchNoiseWorldSize;
half _GrassBrightness;
float4 _GrassWindDirection;
float _GrassWindStrength;
float _GrassWindSpeed;
float _GrassWindWorldSize;
half _GrassWindNormalStrength;
float3 _GrassLightDirection;
fixed4 _GrassLightColor;
fixed4 _GrassAmbientColor;
float _SnowLine;
float _SnowEdgeNoiseMetres;
float _SnowMacroNoiseMetres;
float _SandPatchNoiseWorldSize;
half _RiverEdgeNoiseStrength;
half _RiverEdgeBlendWidth;
half _CliffNormalCutoff;
half _CliffBoundaryNoiseStrength;
half _RockBoundaryNoiseStrength;
half _SandRockSlopeThreshold;
float _CliffNoisePeriod;
half _RockPatchNoiseDetailScale;
float _ForestFloorTextureWorldSize;
half _ForestFloorHeightBlendStrength;
half _ForestFloorEdgeNoiseStrength;
half _ForestFloorEdgeBlendWidth;
float _StonesTextureWorldSize;
half _StonesHeightBlendStrength;
half _StonesEdgeNoiseStrength;
half _StonesEdgeBlendWidth;
half _TopTextureFadeOutSlope;
float4x4 _IslandWorldToLocal;

#include "GrassWindCommon.cginc"

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

half GrassHeightModulatedTextureWeight(
    half slopeWeight,
    half sampledHeight,
    half influence)
{
    half transitionWindow = 4.0h
        * slopeWeight
        * (1.0h - slopeWeight);
    half centredHeight = sampledHeight * 2.0h - 1.0h;
    return saturate(
        slopeWeight
            + centredHeight * influence * transitionWindow * 0.5h);
}

GrassVertexOutput GrassVertex(GrassVertexInput input)
{
    GrassVertexOutput output;
    half3 worldNormal = normalize(UnityObjectToWorldNormal(input.normal));
    float3 surfaceWorldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
    float3 windSample = MotuGrassWindSample(surfaceWorldPosition.xz);
    float3 horizontalWind = float3(windSample.x, 0.0, windSample.z);
    float3 tangentWind = horizontalWind
        - worldNormal * dot(horizontalWind, worldNormal);
    tangentWind *= rsqrt(max(dot(tangentWind, tangentWind), 1.0e-4));
    float shellCurve = GRASS_SHELL_LAYER * GRASS_SHELL_LAYER;
    float3 worldPosition = surfaceWorldPosition
        + worldNormal * (_GrassHeight * GRASS_SHELL_LAYER)
        + tangentWind * (_GrassWindStrength * windSample.y * shellCurve);
    output.pos = UnityWorldToClipPos(worldPosition);
    output.worldPosition = worldPosition;
    output.surfaceWorldPosition = surfaceWorldPosition;
    output.islandLocalSurfacePosition = mul(
        _IslandWorldToLocal,
        float4(surfaceWorldPosition, 1.0)).xyz;
    output.worldNormal = worldNormal;
    output.windLightingAndForestFloor = half4(
        tangentWind * (windSample.y * GRASS_SHELL_LAYER),
        input.environment.x);
    output.stones = input.environment.y;
    output.material = input.material;
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
    float2 radialFadeCell = floor(
        input.islandLocalSurfacePosition.xz * max(_GrassDensity, 0.1));
    // Alpha-tested shells can write camera depth and receive Unity's
    // screen-space shadows. Fade complete blade columns with stable world-space
    // stippling so all shells in a tuft appear and disappear together.
    clip(radialWeight - GrassHash(radialFadeCell + float2(43.17, 19.73)));
    half3 normal = normalize(input.worldNormal);
    float elevation = input.islandLocalSurfacePosition.y;
    float noisePeriod = max(_CliffNoisePeriod, 1.0);
    float3 noisePosition = input.islandLocalSurfacePosition / noisePeriod;
    half3 broadNoise = tex3D(
        _CliffNoise3D,
        noisePosition).rgb * 2.0 - 1.0;
    half3 macroNoise = tex3D(
        _CliffNoise3D,
        noisePosition * (1.0 / 3.0)
            + float3(0.23, 0.71, 0.41)).rgb * 2.0 - 1.0;
    half3 rockPatchLayers = tex3D(
        _CliffNoise3D,
        noisePosition * _RockPatchNoiseDetailScale
            + float3(0.67, 0.31, 0.91)).rgb;
    half3 bankDetailNoise = tex3D(
        _CliffNoise3D,
        noisePosition * (_RockPatchNoiseDetailScale * 4.0)
            + float3(0.11, 0.83, 0.47)).rgb * 2.0 - 1.0;
    // Match the ground shader exactly so grass shells end on the same noisy,
    // world-space material boundary instead of exposing mesh edges.
    half cliffBoundaryNoise = clamp(
        broadNoise.r * 0.65 + macroNoise.g * 0.35,
        -1.0,
        1.0);
    half cutoffNormal = normal.y
        - cliffBoundaryNoise * _CliffBoundaryNoiseStrength;
    half cliffWeight = GrassAntialiasedMask(_CliffNormalCutoff - cutoffNormal);
    half rockBoundaryNoise = clamp(
        broadNoise.b * 0.55 + macroNoise.b * 0.45,
        -1.0,
        1.0);
    half forcedRockBoundaryNoise = clamp(
        (rockPatchLayers.b * 2.0h - 1.0h) * 0.35h
            + bankDetailNoise.r * 0.65h,
        -1.0,
        1.0);
    // Match the ground shader's finer coherent river-bank boundary exactly.
    half forcedRockBlendWidth = max(_RiverEdgeBlendWidth, 0.001h);
    half forcedRockBlendStart = saturate(
        1.0h
            - forcedRockBlendWidth
            + forcedRockBoundaryNoise * forcedRockBlendWidth * 0.75h);
    half forcedRockCoverage = smoothstep(
        forcedRockBlendStart,
        1.0h,
        input.material.r);
    half hardness = saturate(input.material.r);
    half looseCover = saturate(input.material.g);
    half slope = 1.0 - saturate(normal.y);
    half riverBed = saturate(input.material.b);
    half riverNoise = clamp(
        dot(broadNoise, half3(0.577h, -0.577h, 0.577h)),
        -1.0h,
        1.0h);
    half riverThreshold = 0.5h + riverNoise * _RiverEdgeNoiseStrength;
    half riverDistance = riverBed - riverThreshold;
    half riverTransition = max(
        _RiverEdgeBlendWidth,
        fwidth(riverDistance));
    half riverCoverage = smoothstep(
        -riverTransition,
        riverTransition,
        riverDistance);
    half seaProximity = saturate(input.material.a);
    half sandAltitudeWeight = 1.0h - smoothstep(
        2.0h,
        4.0h,
        elevation);
    half sandRichness = looseCover
        * seaProximity
        * sandAltitudeWeight
        * (1.0h - riverCoverage);
    float2 sandPatchUv = input.islandLocalSurfacePosition.xz
        / max(_SandPatchNoiseWorldSize, 0.1)
        + float2(0.37, 0.73);
    half2 sandPatchLayers = tex2D(_GrassPatchNoise, sandPatchUv).rg;
    half sandPatchNoise = sandPatchLayers.r * 0.40
        + sandPatchLayers.g * 0.60;
    half beachCandidateCoverage = GrassAntialiasedMask(
        sandPatchNoise - (1.0 - sandRichness))
        * step(1.0e-4, sandRichness);

    half geologyRockWeight = saturate(slope * lerp(1.3, 3.0, hardness));
    half rockPatchNoise = rockPatchLayers.r * 0.65
        + rockPatchLayers.g * 0.35;
    half rockMaskDistance = rockPatchNoise
        - (1.0 - geologyRockWeight);
    // Match the distant terrain's broad visual rock coverage exactly, then
    // treat that coverage as a hard physical exclusion for the fur below.
    half rockBlendWidth = max(
        0.20h,
        fwidth(rockMaskDistance));
    half geologyRockCoverage = smoothstep(
        -rockBlendWidth,
        rockBlendWidth,
        rockMaskDistance)
        * smoothstep(0.0h, 0.20h, geologyRockWeight);
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

    // Use the terrain shader's authored forest-floor projection and height
    // shaping before excluding fur. The previous raw-mask stipple let grass
    // columns appear over the visible material blend as the close grass pass
    // faded in, because it did not describe the rendered boundary.
    half topTextureFadeOutUpNormal = cos(radians(
        _TopTextureFadeOutSlope));
    half localUpNormal = saturate(normal.y);
    half simpleStoneBlendNoise = clamp(
        macroNoise.r * 0.20h
            + broadNoise.g * 0.25h
            + (rockPatchLayers.g * 2.0h - 1.0h) * 0.55h,
        -1.0h,
        1.0h);
    half noisyTextureUpNormal = saturate(
        localUpNormal
            + simpleStoneBlendNoise
                * 0.16h
                * (1.0h - localUpNormal));
    half topTextureBlendStart = saturate(
        topTextureFadeOutUpNormal - 0.20h);
    half forestFloorProjectionWeight = smoothstep(
        topTextureBlendStart,
        1.0h,
        noisyTextureUpNormal);
    float2 forestFloorUv = input.islandLocalSurfacePosition.xz
        / max(_ForestFloorTextureWorldSize, 0.01);
    half forestFloorHeight = tex2D(
        _ForestStonesMaskMap,
        forestFloorUv).r;
    half forestFloorTextureWeight = GrassHeightModulatedTextureWeight(
        forestFloorProjectionWeight,
        forestFloorHeight,
        _ForestFloorHeightBlendStrength);
    half forestFloorBoundaryNoise = clamp(
        bankDetailNoise.g * 0.70h
            + bankDetailNoise.b * 0.30h,
        -1.0h,
        1.0h);
    half forestFloorDistance = saturate(
        input.windLightingAndForestFloor.w)
        - (0.5h
            + forestFloorBoundaryNoise
                * _ForestFloorEdgeNoiseStrength);
    half forestFloorTransition = max(
        _ForestFloorEdgeBlendWidth,
        fwidth(forestFloorDistance));
    half forestFloorSource = smoothstep(
        -forestFloorTransition,
        forestFloorTransition,
        forestFloorDistance);
    half forestFloorCoverage = forestFloorSource
        * forestFloorTextureWeight
        * (1.0h - exposedRockCoverage)
        * (1.0h - beachCoverage)
        * (1.0h - riverCoverage)
        * (1.0h - snowCoverage)
        * step(0.0h, elevation);
    half stonesBoundaryNoise = clamp(
        bankDetailNoise.r * 0.45h
            + bankDetailNoise.b * 0.55h,
        -1.0h,
        1.0h);
    half stonesDistance = saturate(input.stones)
        - (0.5h + stonesBoundaryNoise * _StonesEdgeNoiseStrength);
    half stonesTransition = max(
        _StonesEdgeBlendWidth,
        fwidth(stonesDistance));
    half stonesSource = smoothstep(
        -stonesTransition,
        stonesTransition,
        stonesDistance);
    half stonesCoverage = stonesSource
        * (1.0h - exposedRockCoverage)
        * (1.0h - beachCoverage)
        * (1.0h - riverCoverage)
        * (1.0h - snowCoverage)
        * (1.0h - forestFloorCoverage)
        * step(0.0h, elevation);
    clip(elevation);
    clip(0.01h - exposedRockCoverage);
    clip(0.5 - beachCoverage);
    clip(0.5 - snowCoverage);
    // Cut the fur at the same coherent midpoint used by the visible surface.
    // The narrow antialiased colour band remains underneath, without sparse
    // blades or flat green extending onto the forest-floor side.
    clip(0.5h - forestFloorCoverage);
    clip(0.5h - stonesCoverage);

    // Deposit thickness is the soil-richness signal. A dedicated fine,
    // coherent texture removes every blade at zero richness, then lowers the
    // noise threshold continuously until rich soil has complete coverage.
    float2 patchUv = input.islandLocalSurfacePosition.xz
        / max(_GrassPatchNoiseWorldSize, 0.1);
    half2 patchLayers = tex2D(_GrassPatchNoise, patchUv).rg;
    half patchNoise = patchLayers.r * 0.65 + patchLayers.g * 0.35;
    clip(looseCover - 1.0e-4);
    clip(patchNoise - (1.0 - looseCover));

    float2 regularCoordinate = input.islandLocalSurfacePosition.xz
        * max(_GrassDensity, 0.1);
    float2 warpCoordinate = regularCoordinate * 0.18;
    float2 domainWarp = float2(
        GrassValueNoise(warpCoordinate + float2(13.7, 4.3)),
        GrassValueNoise(warpCoordinate + float2(2.1, 19.4))) - 0.5;
    float2 grassCoordinate = regularCoordinate + domainWarp * 1.65;
    float2 cell = floor(grassCoordinate);
    half riverFadeRandom = GrassHash(cell + float2(73.1, 11.9));
    clip((1.0h - riverCoverage) - riverFadeRandom - 0.001h);
    float2 cellOffset = float2(
        GrassHash(cell + 11.3),
        GrassHash(cell + 29.7)) - 0.5;
    float2 cellPosition = frac(grassCoordinate) - 0.5
        - cellOffset * 0.25;
    float leanAngle = GrassHash(cell + float2(7.9, 37.1)) * 6.2831853;
    float2 leanDirection = float2(cos(leanAngle), sin(leanAngle));
    cellPosition -= leanDirection
        * (GRASS_SHELL_LAYER * GRASS_SHELL_LAYER * 0.18);
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

    half3 lightingNormal = normalize(
        lerp(normal, half3(0.0, 1.0, 0.0), 0.35)
            + input.windLightingAndForestFloor.xyz * _GrassWindNormalStrength);
    half3 lightDirection = normalize(_GrassLightDirection);
    half diffuse = saturate(dot(lightingNormal, lightDirection));
    UNITY_LIGHT_ATTENUATION(
        shadowAttenuation,
        input,
        input.worldPosition);
    half3 direct = _GrassLightColor.rgb * diffuse * shadowAttenuation;
    half3 ambient = _GrassAmbientColor.rgb;
    float2 grassColorUv = input.islandLocalSurfacePosition.xz
        / max(_GrassColorNoiseWorldSize, 1.0);
    half grassColorNoise = tex2D(_GrassPatchNoise, grassColorUv).b;
    fixed3 grassColor = lerp(
        _GrassColorA.rgb,
        _GrassColorB.rgb,
        smoothstep(0.1h, 0.9h, grassColorNoise));
    // Preserve darker roots and sunlit tips without changing the spatial
    // palette selected by the broad coherent colour field.
    grassColor *= lerp(0.72h, 1.18h, GRASS_SHELL_LAYER);
    // Keep the spatial variation neutral on average so the fur has the same
    // mean albedo as the terrain's base grass colour.
    grassColor *= lerp(0.90, 1.10, GrassHash(cell + 17.0));
    grassColor *= _GrassBrightness;
    fixed4 color = fixed4(
        grassColor * (ambient + direct),
        1.0);
    UNITY_APPLY_FOG(input.fogCoord, color);
    return color;
}
