Shader "Motu/Sea Water"
{
    Properties
    {
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("Ocean Ripple Noise", 2D) = "black" {}
        _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        _OpacityDepth ("Full Opacity Depth", Float) = 5
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.65
        _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.8
        _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        [HideInInspector] _WaterSkyExposure ("Water Sky Exposure", Range(0, 1)) = 1
        _RefractionStrength ("Underwater Distortion", Range(0, 0.03)) = 0.02
        _RefractionDepth ("Full Distortion Depth (metres)", Float) = 0.6
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.008
    }

    SubShader
    {
        Tags { "Queue"="Transparent" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        Cull Off

        GrabPass { "_MotuWaterBackground" }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile_fog
            #pragma multi_compile_fwdbase

            #include "WaterCommon.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float brightness : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float4 screenPosition : TEXCOORD2;
                float surfaceEyeDepth : TEXCOORD3;
                float3 worldPosition : TEXCOORD4;
                UNITY_FOG_COORDS(5)
                float4 grabPosition : TEXCOORD6;
                SHADOW_COORDS(7)
            };

            sampler2D _NoiseTex;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.pos = UnityObjectToClipPos(input.vertex);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.worldNormal = normal;
                output.screenPosition = ComputeScreenPos(output.pos);
                output.grabPosition = ComputeGrabScreenPos(output.pos);
                output.surfaceEyeDepth = -UnityObjectToViewPos(input.vertex).z;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.brightness = 0.72
                    + 0.28 * saturate(dot(
                        normal,
                        normalize(float3(0.3, 1.0, 0.2))));
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 worldNormal = MotuFacingWaterNormal(
                    input.worldNormal,
                    viewDirection);
                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                float waterDepth = MotuWaterDepth(
                    input.screenPosition,
                    input.surfaceEyeDepth);

                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                fixed3 waterIllumination = MotuWaterIllumination(
                    worldNormal,
                    input.worldPosition,
                    shadowAttenuation,
                    0.12h,
                    cloud);
                fixed3 waterBody = _Color.rgb
                    * input.brightness
                    * waterIllumination;
                float2 reflectionNoiseUv = MotuCloudWorldToLocal(
                    input.worldPosition).xz / 8.0;
                half2 reflectionRipple = half2(
                    tex2D(
                        _NoiseTex,
                        reflectionNoiseUv + float2(_Time.y * 0.025, 0.0)).r,
                    tex2D(
                        _NoiseTex,
                        reflectionNoiseUv.yx + float2(0.0, _Time.y * 0.02)).g)
                    - 0.5h;
                fixed3 water = MotuShadeWater(
                    waterBody,
                    worldNormal,
                    viewDirection,
                    input.worldPosition,
                    reflectionRipple,
                    1.0h,
                    shadowAttenuation,
                    cloud);
                half waterOpacity = MotuWaterOpacity(
                    waterDepth,
                    _OpacityDepth);
                fixed4 foggedSurface = fixed4(water, 1.0h);
                UNITY_APPLY_FOG(input.fogCoord, foggedSurface);
                fixed3 refractedScene = MotuRefractScene(
                    input.grabPosition,
                    waterDepth,
                    worldNormal,
                    viewDirection,
                    reflectionRipple);
                return fixed4(
                    lerp(refractedScene, foggedSurface.rgb, waterOpacity),
                    1.0h);
            }
            ENDCG
        }
    }
}
