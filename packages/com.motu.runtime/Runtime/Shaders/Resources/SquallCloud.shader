Shader "Motu/Squall Cloud"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "Queue"="Transparent+50" "RenderType"="Transparent" }
        Blend One OneMinusSrcAlpha
        Cull Front ZWrite Off ZTest Always
        Pass
        {
            // The renderer feature copies the real water/opaque depth first.
            Tags { "LightMode"="MotuSquallVolume" }
            HLSLPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #include "../MotuUrp.hlsl"
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/DeclareDepthTexture.hlsl"
            #include "../SquallVolume.cginc"
            float4 _MotuIslandHorizonColour;
            struct Input { float4 vertex : POSITION; };
            struct Varyings { float4 position : SV_POSITION; float3 world : TEXCOORD0; };
            Varyings Vertex(Input input)
            {
                Varyings output;
                output.world = TransformObjectToWorld(input.vertex.xyz);
                output.position = TransformWorldToHClip(output.world);
                return output;
            }
            float SceneDistance(float2 screenUv, float3 ray, float3 forward)
            {
                float rawDepth = SampleSceneDepth(screenUv);
                float orthographicDepth = rawDepth;
                #if UNITY_REVERSED_Z
                orthographicDepth = 1 - orthographicDepth;
                #endif
                float depth = lerp(LinearEyeDepth(rawDepth, _ZBufferParams),
                    lerp(_ProjectionParams.y, _ProjectionParams.z, orthographicDepth), unity_OrthoParams.w);
                return depth / max(.001, dot(ray, forward));
            }
            half4 Fragment(Varyings input) : SV_Target
            {
                float3 forward = -UNITY_MATRIX_V[2].xyz;
                float3 ray = normalize(lerp(normalize(input.world - _WorldSpaceCameraPos), forward, unity_OrthoParams.w));
                float3 rayOrigin = lerp(_WorldSpaceCameraPos,
                    input.world - forward * dot(input.world - _WorldSpaceCameraPos, forward), unity_OrthoParams.w);
                float3 origin = TransformWorldToObject(rayOrigin);
                float3 direction = mul((float3x3)unity_WorldToObject, ray);
                float3 inverse = rcp(direction + float3(1e-8, 1e-8, 1e-8));
                float3 t0 = (-.5 - origin) * inverse, t1 = (.5 - origin) * inverse;
                float3 low = min(t0, t1), high = max(t0, t1);
                float enter = max(0, max(low.x, max(low.y, low.z)));
                float leave = min(high.x, min(high.y, high.z));
                leave = min(leave, SceneDistance(input.position.xy / _ScaledScreenParams.xy, ray, forward));
                if (leave <= enter) return 0;
                const int budget = 48;
                float stepLength = (leave - enter) / budget;
                float phase = frac(52.9829189 * frac(dot(floor(input.position.xy), float2(.06711056,.00583715))));
                half transmittance = 1;
                half3 radiance = 0;
                half3 rainLight = _MotuCloudAmbientColor.rgb * .45h
                    + _MotuCloudLightColor.rgb * _MotuCloudLightActive * .15h
                    + _MotuCloudSecondaryLightColor.rgb * _MotuCloudSecondaryLightActive * .15h;
                half3 rainColour = rainLight * half3(.38,.43,.48)
                    + half3(.3,.4,.7) * _SquallFlash * .18h;
                [loop] for (int sampleIndex = 0; sampleIndex < budget; sampleIndex++)
                {
                    if (transmittance < .006h) break;
                    float travel = enter + (sampleIndex + phase) * stepLength;
                    float3 position = rayOrigin + ray * travel - _SquallOrigin.xyz;
                    half cloud = MotuSquallCloudDensity(position);
                    float pixelFootprint = lerp(2 * travel / max(1, _ScaledScreenParams.y * abs(UNITY_MATRIX_P[1][1])),
                        2 * unity_OrthoParams.y / _ScaledScreenParams.y, unity_OrthoParams.w);
                    half rain = MotuSquallRainDensity(position, max(stepLength, pixelFootprint));
                    float cloudExtinction = cloud * _SquallLayer.w / 280;
                    // Keep the storm-sized curtain readable behind nearby drops.
                    float rainExtinction = rain * .036;
                    float extinction = cloudExtinction + rainExtinction;
                    if (extinction <= .00001) continue;
                    half opacity = 1 - exp2(-extinction * stepLength * _SquallStrength * 1.442695);
                    half3 cloudColour = 0;
                    [branch] if (cloud > .001h) cloudColour = MotuSquallCloudColour(position, cloud, ray);
                    half3 colour = (cloudColour * cloudExtinction + rainColour * rainExtinction) / extinction;
                    half haze = 1 - exp2(-max(0, travel - 250) / 2200);
                    colour = lerp(colour, _MotuIslandHorizonColour.rgb, haze);
                    radiance += transmittance * opacity * colour;
                    transmittance *= 1 - opacity;
                }
                // Fade premultiplied colour AND opacity after integration:
                // even an optically thick distant core blends into the sky.
                return half4(radiance, 1 - transmittance) * _SquallDistanceOpacity;
            }
            ENDHLSL
        }
    }
    FallBack Off
}
