Shader "Hidden/Motu/Squall Composite"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            Blend One OneMinusSrcAlpha
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment Fragment
            #pragma target 3.5
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/DeclareDepthTexture.hlsl"
            #include "Packages/com.unity.render-pipelines.core/Runtime/Utilities/Blit.hlsl"
            float4 _MotuSquallRenderSize;

            float EyeDepth(float2 uv)
            {
                float raw = SampleSceneDepth(uv);
                float ortho = raw;
                #if UNITY_REVERSED_Z
                    ortho = 1 - ortho;
                #endif
                return lerp(LinearEyeDepth(raw, _ZBufferParams),
                    lerp(_ProjectionParams.y, _ProjectionParams.z, ortho), unity_OrthoParams.w);
            }

            half4 Fragment(Varyings input) : SV_Target
            {
                UNITY_SETUP_STEREO_EYE_INDEX_POST_VERTEX(input);
                float2 uv = input.texcoord;
                float2 pixel = uv * _MotuSquallRenderSize.xy - .5;
                float2 corner = (floor(pixel) + .5) * _MotuSquallRenderSize.zw;
                float2 blend = frac(pixel);
                float depth = EyeDepth(uv);
                float tolerance = max(.1, depth * .01);
                half4 colour = 0;
                float total = 0;
                // Bilateral weights stop the soft volume bleeding over sharp
                // deck, mast and shoreline silhouettes at full resolution.
                [unroll] for (int y = 0; y < 2; ++y)
                [unroll] for (int x = 0; x < 2; ++x)
                {
                    float2 sampleUv = corner + float2(x,y) * _MotuSquallRenderSize.zw;
                    float weight = (x == 0 ? 1 - blend.x : blend.x) * (y == 0 ? 1 - blend.y : blend.y);
                    weight *= exp2(-4 * abs(EyeDepth(sampleUv) - depth) / tolerance);
                    colour += SAMPLE_TEXTURE2D_X_LOD(_BlitTexture, sampler_PointClamp, sampleUv, 0) * weight;
                    total += weight;
                }
                return colour / max(total, .0001);
            }
            ENDHLSL
        }
    }
    FallBack Off
}
