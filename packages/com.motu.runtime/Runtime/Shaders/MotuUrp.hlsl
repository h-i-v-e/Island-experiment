#ifndef MOTU_URP_INCLUDED
#define MOTU_URP_INCLUDED

#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Lighting.hlsl"

// Keep the procedural material functions independent of pipeline plumbing.
#define fixed half
#define fixed2 half2
#define fixed3 half3
#define fixed4 half4
#define sampler2D_float sampler2D
#define UNITY_INITIALIZE_OUTPUT(type, name) name = (type)0;
#define UnityObjectToClipPos(v) TransformObjectToHClip((v).xyz)
#define UnityWorldToClipPos(v) TransformWorldToHClip((v).xyz)
#define UnityObjectToWorldNormal(v) TransformObjectToWorldNormal(v)
#define UnityObjectToWorldDir(v) TransformObjectToWorldDir(v)
#define UnityObjectToViewPos(v) TransformWorldToView(TransformObjectToWorld((v).xyz))
#define UnityWorldSpaceViewDir(v) GetWorldSpaceViewDir(v)
// The no-argument GetMainLight() reports no shadow. Read the directional light
// from the uniforms so a later shadowed sample keeps its attenuation.
#define UnityWorldSpaceLightDir(v) _MainLightPosition.xyz
#define _LightColor0 half4(_MainLightColor.rgb, 1)
#define ShadeSH9(v) SampleSH((v).xyz)
#undef UNITY_LIGHTMODEL_AMBIENT
#define UNITY_LIGHTMODEL_AMBIENT half4(SampleSH(half3(0,1,0)), 1)
#define UNITY_PROJ_COORD(v) (v)
#define UNITY_DECLARE_DEPTH_TEXTURE(name) sampler2D name
#undef SAMPLE_DEPTH_TEXTURE
#define SAMPLE_DEPTH_TEXTURE(name, uv) tex2D(name, uv).r
#define SAMPLE_DEPTH_TEXTURE_PROJ(name, uv) tex2Dproj(name, uv).r
#define UNITY_DECLARE_TEX2DARRAY(name) TEXTURE2D_ARRAY(name); SAMPLER(sampler##name)
#define UNITY_SAMPLE_TEX2DARRAY(name, uv) SAMPLE_TEXTURE2D_ARRAY(name, sampler##name, (uv).xy, (uv).z)

float LinearEyeDepth(float depth) { return LinearEyeDepth(depth, _ZBufferParams); }
float Linear01Depth(float depth) { return Linear01Depth(depth, _ZBufferParams); }

// Shadows are sampled from world position in the fragment stage, including cascades.
#define SHADOW_COORDS(index)
#define UNITY_LIGHTING_COORDS(a,b)
#define TRANSFER_SHADOW(output)
#define TRANSFER_SHADOW_WPOS(output, position)
#define UNITY_TRANSFER_LIGHTING(output, uv)
#define UNITY_LIGHT_ATTENUATION(name, input, position) half name = GetMainLight(TransformWorldToShadowCoord(position), position, half4(1, 1, 1, 1)).shadowAttenuation;

// Island geometry is thousands of metres from the origin. Interpolating that
// absolute position drops the bits that separate shadow texels, and the error
// grows as the view becomes grazing, so the sample can read another cascade
// tile. Interpolate the offset from the camera, then put the camera back
// before URP's own cascade transform.
float3 MotuCameraRelativePosition(float3 positionWS)
{
    return positionWS - GetCameraPositionWS();
}

half MotuMainLightShadowAttenuation(float3 cameraRelativePosition)
{
    float3 positionWS = cameraRelativePosition + GetCameraPositionWS();
    return GetMainLight(TransformWorldToShadowCoord(positionWS), positionWS, half4(1, 1, 1, 1)).shadowAttenuation;
}
#define UNITY_FOG_COORDS(index) float fogCoord : TEXCOORD##index;
#define UNITY_TRANSFER_FOG(output, clip) output.fogCoord = ComputeFogFactor((clip).z);
#define UNITY_APPLY_FOG(coord, colour) colour.rgb = MixFog(colour.rgb, coord);
#define UNITY_APPLY_FOG_COLOR(coord, colour, fog) colour.rgb = MixFogColor(colour.rgb, (fog).rgb, coord);
#define UNITY_CALC_FOG_FACTOR_RAW(distance) float unityFogFactor = ComputeFogIntensity(ComputeFogFactorZ0ToFar(distance));

float3 _LightDirection;
float3 _LightPosition;
float4 MotuShadowPosition(float3 worldPosition, float3 worldNormal)
{
    float3 direction = _LightDirection;
    #if defined(_CASTING_PUNCTUAL_LIGHT_SHADOW)
    direction = normalize(_LightPosition - worldPosition);
    #endif
    float4 clip = TransformWorldToHClip(ApplyShadowBias(worldPosition, worldNormal, direction));
    #if UNITY_REVERSED_Z
    clip.z = min(clip.z, UNITY_NEAR_CLIP_VALUE * clip.w);
    #else
    clip.z = max(clip.z, UNITY_NEAR_CLIP_VALUE * clip.w);
    #endif
    return clip;
}
#define V2F_SHADOW_CASTER float4 pos : SV_POSITION
#define TRANSFER_SHADOW_CASTER_NORMALOFFSET(output) output.pos = MotuShadowPosition(TransformObjectToWorld(v.vertex.xyz), TransformObjectToWorldNormal(v.normal));
#define SHADOW_CASTER_FRAGMENT(input) return 0;

half3 MotuAdditionalLighting(float3 worldPosition, half3 normal)
{
    half3 result = 0;
    #if defined(_ADDITIONAL_LIGHTS)
    uint count = GetAdditionalLightsCount();
    for (uint index = 0; index < count; ++index)
    {
        Light light = GetAdditionalLight(index, worldPosition, half4(1,1,1,1));
        result += light.color * saturate(dot(normal, light.direction))
            * light.distanceAttenuation * light.shadowAttenuation;
    }
    #endif
    return result;
}

struct appdata_base { float4 vertex : POSITION; float3 normal : NORMAL; float4 texcoord : TEXCOORD0; };
struct appdata_img { float4 vertex : POSITION; float2 texcoord : TEXCOORD0; UNITY_VERTEX_INPUT_INSTANCE_ID };
struct v2f_img { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; UNITY_VERTEX_OUTPUT_STEREO };
v2f_img vert_img(appdata_img input)
{
    v2f_img output = (v2f_img)0;
    UNITY_SETUP_INSTANCE_ID(input);
    UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
    output.pos = TransformObjectToHClip(input.vertex.xyz);
    output.uv = input.texcoord;
    return output;
}
#endif
