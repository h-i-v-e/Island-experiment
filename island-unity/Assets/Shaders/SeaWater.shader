Shader "Motu/Sea Water"
{
    Properties
    {
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("Shore Noise", 2D) = "black" {}
        [NoScaleOffset] _SeaMask ("Sea Depth And Land Distance", 2D) = "black" {}
        _WorldSize ("World Size", Float) = 2000
        _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        _OpacityDepth ("Full Opacity Depth", Float) = 5
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.65
        _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.8
        _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        _RefractionStrength ("Underwater Distortion", Range(0, 0.03)) = 0.02
        _RefractionDepth ("Full Distortion Depth (metres)", Float) = 0.6
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.008
        _ShoreWaveStrength ("Shore Wave Strength", Range(0, 1)) = 0.35
        _ShoreWaveSpacing ("Shore Wave Spacing (metres)", Float) = 0.55
        _ShoreWaveSpeed ("Shore Wave Speed (metres/second)", Float) = 0.35
        _ShoreWaveDepth ("Shore Wave Depth (metres)", Float) = 2.5
        _ShoreWaveNoiseWorldSize ("Shore Wave Noise World Size", Float) = 5
        _ShoreWaveIncomingStrength ("Incoming Shore Wave Strength", Range(0, 1)) = 0.65
        _ShoreWaveEchoStrength ("Reverse Shore Echo Strength", Range(0, 1)) = 0.45
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
                float4 position : SV_POSITION;
                float brightness : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float4 screenPosition : TEXCOORD2;
                float surfaceEyeDepth : TEXCOORD3;
                float3 worldPosition : TEXCOORD4;
                UNITY_FOG_COORDS(5)
                float3 islandLocalPosition : TEXCOORD6;
                float4 grabPosition : TEXCOORD7;
            };

            sampler2D _NoiseTex;
            sampler2D _SeaMask;
            float _WorldSize;
            half _ShoreWaveStrength;
            float _ShoreWaveSpacing;
            float _ShoreWaveSpeed;
            float _ShoreWaveDepth;
            float _ShoreWaveNoiseWorldSize;
            half _ShoreWaveIncomingStrength;
            half _ShoreWaveEchoStrength;
            static const float SeaMaskDepthMetres = 5.0;
            static const float SeaMaskLandDistanceMetres = 16.0;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.position = UnityObjectToClipPos(input.vertex);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.worldNormal = normal;
                output.screenPosition = ComputeScreenPos(output.position);
                output.grabPosition = ComputeGrabScreenPos(output.position);
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

                // Mix depth and land proximity for incoming waves to break up
                // broad, uniformly shallow shelves. Land distance separately
                // drives a weaker echo travelling back offshore.
                half incomingProximity = (seaMask.r + (1.0h - seaMask.g)) * 0.5h;
                float incomingDistance = (1.0h - incomingProximity)
                    * SeaMaskDepthMetres;
                float landDistance = seaMask.g * SeaMaskLandDistanceMetres;
                float shoreSpacing = max(_ShoreWaveSpacing, 0.001);
                float shoreRange = max(_ShoreWaveDepth, shoreSpacing);
                float echoRange = max(SeaMaskLandDistanceMetres, shoreSpacing);
                float echoSpacing = shoreSpacing * echoRange / shoreRange;
                float2 shoreNoiseUv = input.islandLocalPosition.xz
                    / max(_ShoreWaveNoiseWorldSize, 0.001);
                half shoreNoise = tex2D(_NoiseTex, shoreNoiseUv).r - 0.5h;
                float incomingPhase = (incomingDistance
                    + _Time.y * _ShoreWaveSpeed
                    + shoreNoise * shoreSpacing * 0.45) / shoreSpacing;
                float echoPhase = (landDistance
                    - _Time.y * _ShoreWaveSpeed
                    + shoreNoise * echoSpacing * 0.45) / echoSpacing;
                half incomingCrest = smoothstep(
                    0.72h,
                    0.98h,
                    0.5h + 0.5h * cos(incomingPhase * 6.2831853));
                half echoCrest = smoothstep(
                    0.72h,
                    0.98h,
                    0.5h + 0.5h * cos(echoPhase * 6.2831853));
                float contactStart = shoreRange * 0.01;
                float contactEnd = max(
                    shoreRange * 0.05,
                    contactStart + 0.0001);
                half incomingContactFade = smoothstep(
                    contactStart,
                    contactEnd,
                    incomingDistance);
                half incomingDeepFade = 1.0h - smoothstep(
                    max(shoreRange - shoreSpacing, shoreSpacing),
                    shoreRange,
                    incomingDistance);
                float echoContactStart = echoRange * 0.01;
                float echoContactEnd = max(
                    echoRange * 0.05,
                    echoContactStart + 0.0001);
                half echoContactFade = smoothstep(
                    echoContactStart,
                    echoContactEnd,
                    landDistance);
                half echoDeepFade = 1.0h - smoothstep(
                    max(echoRange - echoSpacing, echoSpacing),
                    echoRange,
                    landDistance);
                half horizontalSurface = smoothstep(
                    0.65h,
                    0.96h,
                    abs(worldNormal.y));
                half incomingWave = incomingCrest
                    * incomingContactFade
                    * incomingDeepFade
                    * _ShoreWaveIncomingStrength;
                half echoWave = echoCrest
                    * echoContactFade
                    * echoDeepFade
                    * _ShoreWaveEchoStrength;
                half shoreWave = saturate(
                    (incomingWave + echoWave)
                    * horizontalSurface
                    * _ShoreWaveStrength);

                fixed3 waterBody = _Color.rgb * input.brightness;
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
                fixed3 surface = lerp(
                    water,
                    fixed3(1.0, 1.0, 1.0),
                    shoreWave);
                fixed4 foggedSurface = fixed4(surface, 1.0h);
                UNITY_APPLY_FOG(input.fogCoord, foggedSurface);
                fixed3 refractedScene = MotuRefractScene(
                    input.grabPosition,
                    waterDepth,
                    worldNormal,
                    viewDirection,
                    reflectionRipple);
                half surfaceOpacity = saturate(
                    waterOpacity + shoreWave * 0.08h);
                return fixed4(
                    lerp(refractedScene, foggedSurface.rgb, surfaceOpacity),
                    1.0h);
            }
            ENDCG
        }
    }
}
