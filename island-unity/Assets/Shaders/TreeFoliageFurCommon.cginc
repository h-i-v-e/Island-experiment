#include "UnityCG.cginc"
#include "Lighting.cginc"
#include "AutoLight.cginc"
#include "TreeSurfaceNoise.cginc"
#include "TreeWindCommon.cginc"

struct FoliageFurVertexInput
{
    float4 vertex : POSITION;
    float3 normal : NORMAL;
    float4 treeData : COLOR;
    float2 windData : TEXCOORD0;
    UNITY_VERTEX_INPUT_INSTANCE_ID
};

struct FoliageFurVertexOutput
{
    float4 pos : SV_POSITION;
    float3 worldPosition : TEXCOORD0;
    float3 surfaceWorldPosition : TEXCOORD1;
    half3 worldNormal : TEXCOORD2;
    float3 islandLocalSurfacePosition : TEXCOORD3;
    SHADOW_COORDS(4)
    UNITY_FOG_COORDS(5)
    UNITY_VERTEX_INPUT_INSTANCE_ID
    UNITY_VERTEX_OUTPUT_STEREO
};

fixed4 _BaseColor;
fixed4 _LightColor;
fixed4 _TranslucencyColor;
half _FoliageTranslucency;
half _FoliageAmbientFloor;
half _CanopyCoverage;
half _CanopyEdgeSoftness;
half _AlphaCutoff;
float _FoliageFurHeight;
float _FoliageLeafWorldSize;
half _FoliageLeafCoverage;
half _FoliageLeafEdgeSoftness;
float3 _GrassPlayerPosition;
float _GrassRadius;
float _GrassFadeWidth;

FoliageFurVertexOutput FoliageFurVertex(FoliageFurVertexInput input)
{
    FoliageFurVertexOutput output;
    UNITY_SETUP_INSTANCE_ID(input);
    UNITY_INITIALIZE_OUTPUT(FoliageFurVertexOutput, output);
    UNITY_TRANSFER_INSTANCE_ID(input, output);
    UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
    float3 originalWorldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
    float3 islandLocalPosition = mul(
        _IslandWorldToLocal,
        float4(originalWorldPosition, 1.0)).xyz;
    float3 windOffset = MotuTreeWindOffsetAtHeight(
        originalWorldPosition,
        islandLocalPosition,
        input.treeData,
        input.windData.x);
    half3 worldNormal = MotuTreeWindNormal(
        normalize(UnityObjectToWorldNormal(input.normal)),
        windOffset);
    float3 surfaceWorldPosition = originalWorldPosition + windOffset;
    float3 worldPosition = surfaceWorldPosition
        + worldNormal * (_FoliageFurHeight * FOLIAGE_SHELL_LAYER);
    output.pos = UnityWorldToClipPos(worldPosition);
    output.worldPosition = worldPosition;
    output.surfaceWorldPosition = surfaceWorldPosition;
    output.worldNormal = worldNormal;
    output.islandLocalSurfacePosition = islandLocalPosition;
    TRANSFER_SHADOW_WPOS(output, worldPosition);
    UNITY_TRANSFER_FOG(output, output.pos);
    return output;
}

fixed4 FoliageFurFragment(FoliageFurVertexOutput input) : SV_Target
{
    UNITY_SETUP_INSTANCE_ID(input);

    // Match the fur grass transition exactly: every shell fades together over
    // the final band of the player radius rather than losing random fragments.
    float playerDistance = distance(
        input.surfaceWorldPosition.xz,
        _GrassPlayerPosition.xz);
    float fadeWidth = min(max(_GrassFadeWidth, 0.001), _GrassRadius);
    half radialWeight = 1.0 - smoothstep(
        _GrassRadius - fadeWidth,
        _GrassRadius,
        playerDistance);
    clip(radialWeight - 0.001h);

    MotuTreeNoiseSample surfaceNoise = MotuSampleTreeNoise(
        input.islandLocalSurfacePosition);
    half3 normal = MotuPerturbTreeNormal(
        normalize(input.worldNormal),
        surfaceNoise);
    half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
    half diffuse = saturate(dot(normal, lightDirection));
    UNITY_LIGHT_ATTENUATION(
        attenuation,
        input,
        input.worldPosition);
    half lightBlend = saturate(
        0.18h + diffuse * 0.72h + surfaceNoise.broad.g * 0.10h);
    half materialAlpha = lerp(_BaseColor.a, _LightColor.a, lightBlend);

    // Keep the original column-aligned canopy holes open through all shells.
    half canopyAlpha = MotuTreeCanopyAlpha(
        input.islandLocalSurfacePosition,
        _CanopyCoverage,
        _CanopyEdgeSoftness,
        materialAlpha);
    clip(canopyAlpha - _AlphaCutoff);

    // A low-frequency three-dimensional field forms broad leaf clusters. The
    // threshold rises through the shell stack so each cluster tapers naturally
    // instead of becoming a solid inflated copy of the canopy.
    float3 leafPosition = input.islandLocalSurfacePosition
        / max(_FoliageLeafWorldSize, 0.05);
    half3 broadLeaves = tex3D(
        _CliffNoise3D,
        leafPosition + float3(0.41, 0.17, 0.73)).rgb;
    half3 detailLeaves = tex3D(
        _CliffNoise3D,
        leafPosition * 1.85 + float3(0.13, 0.79, 0.37)).rgb;
    half leafNoise = saturate(
        broadLeaves.r * 0.50h
            + broadLeaves.g * 0.25h
            + detailLeaves.b * 0.25h);
    half rootThreshold = 1.0h - saturate(_FoliageLeafCoverage);
    half shellThreshold = lerp(
        rootThreshold,
        0.78h,
        pow((half)FOLIAGE_SHELL_LAYER, 1.25h));
    half edgeSoftness = max(
        _FoliageLeafEdgeSoftness,
        max((half)fwidth(leafNoise), 0.001h));
    half leafAlpha = smoothstep(
        shellThreshold - edgeSoftness,
        shellThreshold + edgeSoftness,
        leafNoise);
    clip(leafAlpha - 0.01h);

    fixed3 albedo = MotuRotateTreeHue(
        lerp(_BaseColor.rgb, _LightColor.rgb, lightBlend),
        surfaceNoise.hue);
    albedo *= lerp(0.88h, 1.12h, FOLIAGE_SHELL_LAYER);
    fixed4 result = fixed4(
        MotuShadeFoliage(
            albedo,
            normal,
            lightDirection,
            _LightColor0.rgb,
            attenuation,
            _TranslucencyColor.rgb,
            _FoliageTranslucency,
            _FoliageAmbientFloor),
        radialWeight * leafAlpha * materialAlpha);
    UNITY_APPLY_FOG(input.fogCoord, result);
    return result;
}
