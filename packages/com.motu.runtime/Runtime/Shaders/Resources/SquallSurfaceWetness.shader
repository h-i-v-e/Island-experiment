Shader "Hidden/Motu/Squall Surface Wetness"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE _MAIN_LIGHT_SHADOWS_SCREEN
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile_fog
            #include "../MotuUrp.hlsl"
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/DeclareDepthTexture.hlsl"
            #include "Packages/com.unity.render-pipelines.core/Runtime/Utilities/Blit.hlsl"
            float4 _MotuWetness; // amount, rainfall, animation time, sea level
            float _MotuWetFogEnabled;
            half4 _MotuWetAmbient;

            float3 SurfacePosition(float2 uv)
            {
                float depth = SampleSceneDepth(uv);
                #if !UNITY_REVERSED_Z
                    depth = lerp(UNITY_NEAR_CLIP_VALUE, 1, depth);
                #endif
                return ComputeWorldSpacePosition(uv, depth, UNITY_MATRIX_I_VP);
            }

            float3 SurfaceNormal(float2 uv, float3 centre)
            {
                float2 texel = _CameraDepthTexture_TexelSize.xy;
                float3 left = centre - SurfacePosition(uv - float2(texel.x, 0));
                float3 right = SurfacePosition(uv + float2(texel.x, 0)) - centre;
                float3 down = centre - SurfacePosition(uv - float2(0, texel.y));
                float3 up = SurfacePosition(uv + float2(0, texel.y)) - centre;
                float3 dx = dot(left,left) < dot(right,right) ? left : right;
                float3 dy = dot(down,down) < dot(up,up) ? down : up;
                float3 normal = normalize(cross(dy, dx));
                return dot(normal, GetWorldSpaceViewDir(centre)) >= 0 ? normal : -normal;
            }

            float Noise(float2 p)
            {
                float2 cell = floor(p), f = frac(p);
                f = f*f*(3-2*f);
                float4 h = frac(sin(float4(dot(cell,float2(127.1,311.7)), dot(cell+float2(1,0),float2(127.1,311.7)),
                    dot(cell+float2(0,1),float2(127.1,311.7)), dot(cell+1,float2(127.1,311.7))))*43758.5453);
                return lerp(lerp(h.x,h.y,f.x),lerp(h.z,h.w,f.x),f.y);
            }

            half4 Fragment(Varyings input) : SV_Target
            {
                UNITY_SETUP_STEREO_EYE_INDEX_POST_VERTEX(input);
                float2 uv = input.texcoord;
                half4 source = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, uv);
                float depth = SampleSceneDepth(uv);
                #if UNITY_REVERSED_Z
                    if (depth <= .000001) return source;
                #else
                    if (depth >= .999999) return source;
                #endif
                float3 position = SurfacePosition(uv);
                if (position.y < _MotuWetness.w - .05) return source;
                float3 normal = SurfaceNormal(uv, position);
                float3 stable = MotuCloudWorldToLocal(position);
                float3 view = GetWorldSpaceNormalizeViewDir(position);
                float facing = saturate(normal.y * 2 + .3);
                float patch = lerp(.55, 1, Noise(stable.xz * .7));
                float fog = _MotuWetFogEnabled > .5
                    ? ComputeFogIntensity(InitializeInputDataFog(float4(position, 1), 0)) : 1;
                float wet = saturate(_MotuWetness.x * facing * patch * fog);
                float ripple = saturate(_MotuWetness.y) * saturate(normal.y) * .04;
                float2 rippleFlow = stable.xz * 1.7 + _MotuWetness.z * float2(.6, -.45);
                normal = normalize(normal + float3(Noise(rippleFlow) - .5, 0,
                    Noise(rippleFlow + 19.7) - .5) * ripple);
                Light sun = GetMainLight(TransformWorldToShadowCoord(position), position, half4(1,1,1,1));
                float3 halfway = normalize(sun.direction + view + float3(0,.0001,0));
                float highlight = pow(saturate(dot(normal, halfway)), 48)
                    * saturate(dot(normal, sun.direction)) * sun.shadowAttenuation;
                float grazing = pow(1 - saturate(dot(normal, view)), 2);
                float skyPatch = smoothstep(.35, .8, Noise(reflect(-view, normal).xz * 6 + 7.3));
                float skyReflection = lerp(.08, 1.1, grazing) * lerp(.45, 1, skyPatch);
                MotuCloudLighting cloud = MotuCloudSurfaceLightingForDirection(position, sun.direction);
                // Fullscreen draws have no per-object light/probe data. The
                // directional sun has no distance falloff; use the weather's
                // scene ambient colour instead of an object's SH probe.
                half3 sky = _MotuWetAmbient.rgb * cloud.ambientTransmittance;
                source.rgb *= 1 - wet * .30;
                source.rgb += wet * (sky * skyReflection * 1.5
                    + sun.color * highlight * 1.1 * cloud.directTransmittance);
                return source;
            }
            ENDHLSL
        }
    }
}
