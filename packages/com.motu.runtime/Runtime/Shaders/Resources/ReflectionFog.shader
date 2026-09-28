Shader "Hidden/Motu/Reflection Fog"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Cull Off ZWrite Off ZTest Always
        Pass
        {
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment Fragment
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/DeclareDepthTexture.hlsl"
            #include "Packages/com.unity.render-pipelines.core/Runtime/Utilities/Blit.hlsl"

            float4 _PlanarReflectionViewerPosition;
            float4 _MotuReflectionFogSettings;
            float4 _MotuReflectionFogColor;

            half4 Fragment(Varyings input) : SV_Target
            {
                float2 uv = input.texcoord;
                half4 colour = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, uv);
                float depth = SampleSceneDepth(uv);
                #if UNITY_REVERSED_Z
                    if (depth <= 0.00001) return colour;
                #else
                    if (depth >= 0.99999) return colour;
                #endif

                float3 position = ComputeWorldSpacePosition(uv, depth, UNITY_MATRIX_I_VP);
                float distanceMetres = distance(position, _PlanarReflectionViewerPosition.xyz);
                float mode = _MotuReflectionFogSettings.w;
                float density = max(_MotuReflectionFogSettings.x, 0);
                float visibility = 1;
                if (mode > 0.5 && mode < 1.5)
                {
                    float scaled = density * distanceMetres;
                    visibility = exp(-scaled * scaled);
                }
                else if (mode > 1.5 && mode < 2.5)
                {
                    visibility = exp(-density * distanceMetres);
                }
                else if (mode > 2.5)
                {
                    visibility = saturate((_MotuReflectionFogSettings.z - distanceMetres)
                        / max(_MotuReflectionFogSettings.z - _MotuReflectionFogSettings.y, 0.001));
                }
                colour.rgb = lerp(_MotuReflectionFogColor.rgb, colour.rgb, visibility);
                return colour;
            }
            ENDHLSL
        }
    }
    FallBack Off
}
