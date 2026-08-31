#include "UnityCG.cginc"
#include "Lighting.cginc"
#include "AutoLight.cginc"
#include "TerrainCoverageCommon.cginc"

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
    half3 windLighting : TEXCOORD7;
    half2 environment : TEXCOORD8;
};

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
half _GrassBrightness;
float4 _GrassWindDirection;
float _GrassWindStrength;
float _GrassWindSpeed;
float _GrassWindWorldSize;
half _GrassWindNormalStrength;
float3 _GrassLightDirection;
fixed4 _GrassLightColor;
fixed4 _GrassAmbientColor;
float4x4 _IslandWorldToLocal;

#include "GrassWindCommon.cginc"
#include "CloudCommon.cginc"

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
    output.windLighting = tangentWind * (windSample.y * GRASS_SHELL_LAYER);
    output.environment = input.environment;
    output.material = input.material;
    TRANSFER_SHADOW_WPOS(output, worldPosition);
    UNITY_TRANSFER_FOG(output, output.pos);
    return output;
}

fixed4 GrassFragment(GrassVertexOutput input) : SV_Target
{
    clip(_GrassEnabled - 0.5h);

    float playerDistance = distance(
        input.surfaceWorldPosition.xz,
        _GrassPlayerPosition.xz);
    float fadeWidth = min(max(_GrassFadeWidth, 0.001), _GrassRadius);
    half radialWeight = 1.0h - smoothstep(
        _GrassRadius - fadeWidth,
        _GrassRadius,
        playerDistance);
    float2 radialFadeCell = floor(
        input.islandLocalSurfacePosition.xz * max(_GrassDensity, 0.1));
    clip(radialWeight - GrassHash(radialFadeCell + float2(43.17, 19.73)));

    half3 normal = normalize(input.worldNormal);
    float3 localPosition = input.islandLocalSurfacePosition;
    MotuTerrainCoverage coverage = MotuBuildTerrainCoverage(
        localPosition,
        normal,
        input.material,
        input.environment);
    half eligibleDirt = MotuGrassEligibleDirtWeight(localPosition, coverage);
    half grassBoundary = smoothstep(0.35h, 0.65h, coverage.grass);
    half grassCoverage = grassBoundary
        * eligibleDirt
        * (1.0h - coverage.snow);
    clip(localPosition.y);
    clip(grassCoverage - 0.5h);

    float2 regularCoordinate = localPosition.xz * max(_GrassDensity, 0.1);
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
        lerp(normal, half3(0.0h, 1.0h, 0.0h), 0.35h)
            + input.windLighting * _GrassWindNormalStrength);
    half3 lightDirection = normalize(_GrassLightDirection);
    half diffuse = saturate(dot(lightingNormal, lightDirection));
    UNITY_LIGHT_ATTENUATION(
        shadowAttenuation,
        input,
        input.worldPosition);
    MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
    half3 direct = _GrassLightColor.rgb
        * diffuse
        * shadowAttenuation
        * cloud.directTransmittance;
    half3 ambient = _GrassAmbientColor.rgb * cloud.ambientTransmittance;
    float2 grassColorUv = localPosition.xz
        / max(_GrassColorNoiseWorldSize, 1.0);
    half grassColorNoise = tex2D(_GrassPatchNoise, grassColorUv).b;
    fixed3 grassColor = lerp(
        _GrassColorA.rgb,
        _GrassColorB.rgb,
        smoothstep(0.1h, 0.9h, grassColorNoise));
    grassColor *= lerp(0.72h, 1.18h, GRASS_SHELL_LAYER);
    grassColor *= lerp(0.90h, 1.10h, GrassHash(cell + 17.0));
    grassColor *= _GrassBrightness;
    fixed4 color = fixed4(grassColor * (ambient + direct), 1.0h);
    UNITY_APPLY_FOG(input.fogCoord, color);
    return color;
}
