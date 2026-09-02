Shader "Motu/Waterfall Foot Mist"
{
    Properties
    {
        _TintColor ("Tint", Color) = (0.78, 0.86, 0.89, 0.68)
        _Density ("Density", Range(0.1, 10.0)) = 1.45
        _NoiseScale ("Noise Scale", Range(0.5, 8.0)) = 3.2
        _NoiseOffset ("Noise Offset", Vector) = (0, 0, 0, 0)
    }

    SubShader
    {
        Tags
        {
            "Queue" = "Transparent+20"
            "RenderType" = "Transparent"
            "IgnoreProjector" = "True"
        }

        Cull Front
        Lighting Off
        ZWrite Off
        // The cube back face may sit behind the river bed. The ray march clips
        // every sample against scene depth, so the proxy itself must not be
        // rejected before the fragment shader can integrate its visible half.
        ZTest Always
        Blend SrcAlpha OneMinusSrcAlpha

        Pass
        {
            Tags { "LightMode" = "ForwardBase" }

            CGPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma multi_compile_fwdbase
            #include "UnityCG.cginc"
            #include "AutoLight.cginc"
            #include "WaterfallSprayLighting.cginc"

            struct VertexInput
            {
                float4 position : POSITION;
            };

            struct FragmentInput
            {
                float4 position : SV_POSITION;
                float3 localPosition : TEXCOORD0;
                float4 screenPosition : TEXCOORD1;
                float3 worldPosition : TEXCOORD2;
                SHADOW_COORDS(3)
            };

            fixed4 _TintColor;
            float _Density;
            float _NoiseScale;
            float4 _NoiseOffset;
            UNITY_DECLARE_DEPTH_TEXTURE(_CameraDepthTexture);

            float Hash(float2 coordinate)
            {
                coordinate = frac(coordinate * float2(123.34, 456.21));
                coordinate += dot(coordinate, coordinate + 45.32);
                return frac(coordinate.x * coordinate.y);
            }

            float ValueNoise(float2 coordinate)
            {
                float2 cell = floor(coordinate);
                float2 local = frac(coordinate);
                local = local * local * (3.0 - 2.0 * local);
                return lerp(
                    lerp(Hash(cell), Hash(cell + float2(1.0, 0.0)), local.x),
                    lerp(Hash(cell + float2(0.0, 1.0)), Hash(cell + 1.0), local.x),
                    local.y);
            }

            float VolumeNoise(float3 coordinate)
            {
                return (
                    ValueNoise(coordinate.xy)
                    + ValueNoise(coordinate.yz + 17.31)
                    + ValueNoise(coordinate.xz - 9.17)) / 3.0;
            }

            FragmentInput Vert(VertexInput input)
            {
                FragmentInput output;
                output.position = UnityObjectToClipPos(input.position);
                output.localPosition = input.position.xyz;
                output.screenPosition = ComputeScreenPos(output.position);
                output.worldPosition = mul(
                    unity_ObjectToWorld,
                    input.position).xyz;
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                return output;
            }

            fixed4 Frag(FragmentInput input) : SV_Target
            {
                const int StepCount = 28;
                float3 cameraLocal = mul(
                    unity_WorldToObject,
                    float4(_WorldSpaceCameraPos, 1.0)).xyz;
                float3 rayDirection = normalize(input.localPosition - cameraLocal);
                float3 directionSign = lerp(-1.0, 1.0, step(0.0, rayDirection));
                float3 safeDirection = directionSign * max(abs(rayDirection), 0.00001);
                float3 firstPlane = (-0.5 - cameraLocal) / safeDirection;
                float3 secondPlane = (0.5 - cameraLocal) / safeDirection;
                float3 nearPlanes = min(firstPlane, secondPlane);
                float3 farPlanes = max(firstPlane, secondPlane);
                float nearDistance = max(
                    max(nearPlanes.x, nearPlanes.y),
                    max(nearPlanes.z, 0.0));
                float farDistance = min(farPlanes.x, min(farPlanes.y, farPlanes.z));
                float segmentLength = max(farDistance - nearDistance, 0.0);
                if (segmentLength <= 0.00001)
                {
                    discard;
                }

                float sceneDepth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE_PROJ(
                    _CameraDepthTexture,
                    UNITY_PROJ_COORD(input.screenPosition)));
                float stepLength = segmentLength / StepCount;
                float opticalDepth = 0.0;
                float time = _Time.y;
                for (int stepIndex = 0; stepIndex < StepCount; stepIndex++)
                {
                    float distanceAlongRay = nearDistance + (stepIndex + 0.5) * stepLength;
                    float3 localSample = cameraLocal + rayDirection * distanceAlongRay;
                    float3 worldSample = mul(unity_ObjectToWorld, float4(localSample, 1.0)).xyz;
                    float sampleEyeDepth = -mul(
                        UNITY_MATRIX_V,
                        float4(worldSample, 1.0)).z;
                    if (sampleEyeDepth >= sceneDepth)
                    {
                        continue;
                    }

                    float3 noiseCoordinate =
                        localSample * _NoiseScale
                        + _NoiseOffset.xyz
                        + float3(time * 0.035, time * 0.055, -time * 0.025);
                    float broadNoise = VolumeNoise(noiseCoordinate);
                    float detailNoise = VolumeNoise(noiseCoordinate * 2.13 + 7.1);
                    float3 domainWarp = float3(
                        VolumeNoise(noiseCoordinate * 0.57 + 11.7),
                        VolumeNoise(noiseCoordinate * 0.51 - 5.3),
                        VolumeNoise(noiseCoordinate * 0.63 + 23.1)) - 0.5;
                    float2 warpedFootprint =
                        (localSample.xz + domainWarp.xz * float2(0.055, 0.085)) * 2.0;
                    float footprintField = 1.0
                        - pow(abs(warpedFootprint.x), 6.0)
                        - pow(abs(warpedFootprint.y), 4.0);
                    float horizontalMask = smoothstep(
                        -0.16,
                        0.16,
                        footprintField + (broadNoise - 0.5) * 0.28);

                    float height01 = saturate(localSample.y + 0.5);
                    float3 risingNoiseCoordinate = float3(
                        noiseCoordinate.x * 0.72,
                        noiseCoordinate.z * 0.72,
                        _NoiseOffset.z + time * 0.075);
                    float risingNoise = VolumeNoise(risingNoiseCoordinate);
                    float riseCeiling = lerp(
                        0.32,
                        1.02,
                        smoothstep(0.14, 0.86, risingNoise));
                    float topMask = 1.0 - smoothstep(
                        riseCeiling - 0.15,
                        riseCeiling + 0.035,
                        height01);

                    float coherentDensity =
                        broadNoise * 0.52
                        + detailNoise * 0.25
                        + risingNoise * 0.23;
                    float breakupThreshold = lerp(0.18, 0.61, height01);
                    float risingWisps = smoothstep(
                        breakupThreshold,
                        min(breakupThreshold + 0.24, 0.9),
                        coherentDensity);
                    float lowerBlanket = 1.0 - smoothstep(0.08, 0.34, height01);
                    // Concentrate suspended spray at the impact surface, then
                    // transition into increasingly sparse wisps. A small top
                    // value avoids an artificial horizontal cutoff while the
                    // coherent ceiling still breaks up the silhouette.
                    float verticalDensity = lerp(
                        0.04,
                        1.75,
                        pow(saturate(1.0 - height01), 1.55));
                    float lowerBlanketBoost = lerp(1.0, 1.35, lowerBlanket);
                    float densityVariation = lerp(
                        risingWisps,
                        0.92 + broadNoise * 0.2,
                        lowerBlanket);
                    opticalDepth += horizontalMask
                        * topMask
                        * verticalDensity
                        * lowerBlanketBoost
                        * densityVariation
                        * stepLength
                        * _Density;
                }

                float alpha = saturate((1.0 - exp(-opticalDepth)) * _TintColor.a);
                if (alpha <= 0.002)
                {
                    discard;
                }
                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                return fixed4(
                    MotuWaterfallSprayLighting(
                        _TintColor.rgb,
                        input.worldPosition,
                        shadowAttenuation),
                    alpha);
            }
            ENDCG
        }
    }
}
