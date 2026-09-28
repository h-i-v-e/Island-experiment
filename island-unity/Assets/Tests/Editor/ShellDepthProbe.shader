Shader "Hidden/Motu/Tests/Shell Depth"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "Queue"="AlphaTest" }
        Cull Off ZWrite On ZTest LEqual
        Pass
        {
            Tags { "LightMode"="MotuShell1" }
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment Fragment
            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            struct Attributes { float4 positionOS : POSITION; };
            struct Varyings { float4 positionCS : SV_POSITION; };
            Varyings Vert(Attributes input)
            {
                Varyings output;
                output.positionCS = TransformObjectToHClip(input.positionOS.xyz);
                return output;
            }
            half4 Fragment(Varyings input) : SV_Target { return half4(1, 0, 0, 1); }
            ENDHLSL
        }
    }
}
