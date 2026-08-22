Shader "Motu/Sea Water"
{
    Properties
    {
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("Shore Noise", 2D) = "black" {}
        [NoScaleOffset] _SeaMask ("Sea Coast And Silt Mask", 2D) = "black" {}
        _WorldSize ("World Size", Float) = 2000
        _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        _OpacityDepth ("Full Opacity Depth", Float) = 5
        _SiltColor ("Sea Silt Colour", Color) = (0.325, 0.425, 0.445, 1)
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.65
        _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.8
        _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.008
        _ShoreWaveStrength ("Shore Wave Strength", Range(0, 1)) = 0.35
        _ShoreWaveSpacing ("Shore Wave Spacing (metres)", Float) = 0.55
        _ShoreWaveSpeed ("Shore Wave Speed (metres/second)", Float) = 0.35
        _ShoreWaveDepth ("Shore Wave Depth (metres)", Float) = 2.5
        _ShoreWaveNoiseWorldSize ("Shore Wave Noise World Size", Float) = 5
    }

    SubShader
    {
        Tags { "Queue"="Transparent" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        Cull Off

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
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
                float4 position : SV_POSITION;
                float brightness : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float4 screenPosition : TEXCOORD2;
                float surfaceEyeDepth : TEXCOORD3;
                float3 worldPosition : TEXCOORD4;
                UNITY_FOG_COORDS(5)
                float3 islandLocalPosition : TEXCOORD6;
            };

            sampler2D _NoiseTex;
            sampler2D _SeaMask;
            float _WorldSize;
            fixed4 _SiltColor;
            half _ShoreWaveStrength;
            float _ShoreWaveSpacing;
            float _ShoreWaveSpeed;
            float _ShoreWaveDepth;
            float _ShoreWaveNoiseWorldSize;
            static const float SeaMaskDepthMetres = 5.0;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.position = UnityObjectToClipPos(input.vertex);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.worldNormal = normal;
                output.screenPosition = ComputeScreenPos(output.position);
                output.surfaceEyeDepth = -UnityObjectToViewPos(input.vertex).z;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                output.brightness = 0.72
                    + 0.28 * saturate(dot(
                        normal,
                        normalize(float3(0.3, 1.0, 0.2))));
                UNITY_TRANSFER_FOG(output, output.position);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 worldNormal = MotuFacingWaterNormal(
                    input.worldNormal,
                    viewDirection);
                float waterDepth = MotuWaterDepth(
                    input.screenPosition,
                    input.surfaceEyeDepth);

                float2 seaMaskUv = saturate(
                    input.islandLocalPosition.xz / max(_WorldSize, 0.001) + 0.5);
                half2 seaMask = tex2D(_SeaMask, seaMaskUv).rg;
                half coastWaveWeight = seaMask.r;
                half seaSilt = seaMask.g;

                // The generated red channel is one at sea level and reaches
                // zero at five metres of seabed depth. Reconstruct metres from
                // that stable field so wave contours no longer depend on the
                // camera depth buffer or view angle.
                float shoreDepth = (1.0h - coastWaveWeight)
                    * SeaMaskDepthMetres;
                float shoreSpacing = max(_ShoreWaveSpacing, 0.001);
                float shoreRange = max(_ShoreWaveDepth, shoreSpacing);
                float2 shoreNoiseUv = input.islandLocalPosition.xz
                    / max(_ShoreWaveNoiseWorldSize, 0.001);
                half shoreNoise = tex2D(_NoiseTex, shoreNoiseUv).r - 0.5h;
                float shorePhase = (shoreDepth
                    + _Time.y * _ShoreWaveSpeed
                    + shoreNoise * shoreSpacing * 0.45) / shoreSpacing;
                half shoreCrest = smoothstep(
                    0.72h,
                    0.98h,
                    0.5h + 0.5h * cos(shorePhase * 6.2831853));
                float contactStart = shoreRange * 0.01;
                float contactEnd = max(
                    shoreRange * 0.05,
                    contactStart + 0.0001);
                half contactFade = smoothstep(
                    contactStart,
                    contactEnd,
                    shoreDepth);
                half deepFade = 1.0h - smoothstep(
                    max(shoreRange - shoreSpacing, shoreSpacing),
                    shoreRange,
                    shoreDepth);
                half horizontalSurface = smoothstep(
                    0.65h,
                    0.96h,
                    abs(worldNormal.y));
                half shoreWave = saturate(
                    shoreCrest
                    * contactFade
                    * deepFade
                    * horizontalSurface
                    * _ShoreWaveStrength
                    * (1.0h - seaSilt));

                fixed3 waterBody = lerp(
                    _Color.rgb * input.brightness,
                    _SiltColor.rgb,
                    seaSilt);
                float2 reflectionNoiseUv = input.islandLocalPosition.xz / 8.0;
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
                    1.0h);
                half waterOpacity = MotuWaterOpacity(
                    waterDepth,
                    _OpacityDepth);
                fixed4 color = fixed4(
                    lerp(water, fixed3(1.0, 1.0, 1.0), shoreWave),
                    saturate(waterOpacity + shoreWave * 0.08h));
                color.a = lerp(color.a, _SiltColor.a, seaSilt);
                UNITY_APPLY_FOG(input.fogCoord, color);
                return color;
            }
            ENDCG
        }
    }
}
