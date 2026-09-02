Shader "Motu/Coastal Water Overlay"
{
    Properties
    {
        _Color ("Shallow Water Tint", Color) = (0.03, 0.28, 0.55, 1)
        _FoamColor ("Shore Foam", Color) = (0.92, 0.97, 1.0, 1)
        [NoScaleOffset] _NoiseTex ("Shore Noise", 2D) = "black" {}
        [NoScaleOffset] _SeaMask ("Sea Depth And Land Distance", 2D) = "black" {}
        _WorldSize ("Island World Size", Float) = 2000
        _CoastalOpacity ("Shallow Tint Opacity", Range(0, 1)) = 0.16
        _FoamOpacity ("Foam Opacity", Range(0, 1)) = 0.72
        _EdgeFadeMetres ("Patch Edge Fade", Float) = 24
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
        Tags
        {
            "Queue" = "Transparent+5"
            "RenderType" = "Transparent"
            "IgnoreProjector" = "True"
        }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        ZTest LEqual
        Cull Off
        Offset -1, -1

        Pass
        {
            Tags { "LightMode" = "ForwardBase" }

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
                float3 worldPosition : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float3 islandLocalPosition : TEXCOORD2;
                UNITY_FOG_COORDS(3)
                SHADOW_COORDS(4)
            };

            sampler2D _NoiseTex;
            sampler2D _SeaMask;
            float _WorldSize;
            fixed4 _FoamColor;
            half _CoastalOpacity;
            half _FoamOpacity;
            float _EdgeFadeMetres;
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
                output.pos = UnityObjectToClipPos(input.vertex);
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.worldNormal = UnityObjectToWorldNormal(input.normal);
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float2 seaMaskUv =
                    input.islandLocalPosition.xz / max(_WorldSize, 0.001) + 0.5;
                float nearestPatchEdge = min(
                    min(seaMaskUv.x, 1.0 - seaMaskUv.x),
                    min(seaMaskUv.y, 1.0 - seaMaskUv.y));
                half patchFade = smoothstep(
                    0.0,
                    max(_EdgeFadeMetres / max(_WorldSize, 0.001), 0.0001),
                    nearestPatchEdge);
                if (patchFade <= 0.0001h)
                    discard;

                half2 seaMask = tex2D(_SeaMask, saturate(seaMaskUv)).rg;
                // Incoming waves use both shallow depth and proximity to land;
                // the weaker echo travels back offshore using land distance.
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
                half incomingContactFade = smoothstep(
                    shoreRange * 0.01,
                    max(shoreRange * 0.05, shoreRange * 0.01 + 0.0001),
                    incomingDistance);
                half incomingDeepFade = 1.0h - smoothstep(
                    max(shoreRange - shoreSpacing, shoreSpacing),
                    shoreRange,
                    incomingDistance);
                half echoContactFade = smoothstep(
                    echoRange * 0.01,
                    max(echoRange * 0.05, echoRange * 0.01 + 0.0001),
                    landDistance);
                half echoDeepFade = 1.0h - smoothstep(
                    max(echoRange - echoSpacing, echoSpacing),
                    echoRange,
                    landDistance);
                half shoreWave = saturate((
                    incomingCrest
                        * incomingContactFade
                        * incomingDeepFade
                        * _ShoreWaveIncomingStrength
                    + echoCrest
                        * echoContactFade
                        * echoDeepFade
                        * _ShoreWaveEchoStrength)
                    * _ShoreWaveStrength);

                half shallowTint = seaMask.r
                    * lerp(0.35h, 1.0h, 1.0h - seaMask.g);
                half alpha = patchFade * max(
                    shallowTint * _CoastalOpacity,
                    shoreWave * _FoamOpacity);
                if (alpha <= 0.001h)
                    discard;

                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 worldNormal = MotuFacingWaterNormal(
                    input.worldNormal,
                    viewDirection);
                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                fixed3 illumination = MotuWaterIllumination(
                    worldNormal,
                    input.worldPosition,
                    shadowAttenuation,
                    0.20h,
                    cloud);
                fixed3 shallowColour = _Color.rgb * illumination;
                fixed3 foamColour = _FoamColor.rgb * illumination;
                fixed4 result = fixed4(
                    lerp(shallowColour, foamColour, saturate(shoreWave * 1.4h)),
                    alpha);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }
    }
}
