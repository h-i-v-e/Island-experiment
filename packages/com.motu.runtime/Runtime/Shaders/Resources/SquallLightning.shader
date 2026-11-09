Shader "Motu/Squall Lightning"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "Queue"="Transparent+70" "RenderType"="Transparent" }
        Blend One One
        Cull Off ZWrite Off
        Pass
        {
            Tags { "LightMode"="MotuSquallVolume" }
            HLSLPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #include "../MotuUrp.hlsl"
            float _SquallFlash;
            struct Input { float4 vertex : POSITION; float2 uv : TEXCOORD0; };
            struct Varyings { float4 position : SV_POSITION; float2 uv : TEXCOORD0; float height : TEXCOORD1; };
            Varyings Vertex(Input input)
            {
                Varyings output;
                float3 world = TransformObjectToWorld(input.vertex.xyz);
                output.position = TransformWorldToHClip(world);
                output.height = world.y; output.uv = input.uv;
                return output;
            }
            half4 Fragment(Varyings input) : SV_Target
            {
                half width = abs(input.uv.x - .5) * 2;
                half glow = exp(-width * 4) + exp(-width * width * 95) * 2.5;
                return half4(half3(.65,.8,1) * glow * _SquallFlash, 1);
            }
            ENDHLSL
        }
    }
    FallBack Off
}
