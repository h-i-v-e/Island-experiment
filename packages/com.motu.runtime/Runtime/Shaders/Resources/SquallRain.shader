Shader "Motu/Squall Rain"
{
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "Queue"="Transparent+60" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        Cull Off ZWrite Off ZTest LEqual
        Pass
        {
            Tags { "LightMode"="MotuSquallVolume" }
            HLSLPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #include "../MotuUrp.hlsl"
            float _SquallFlash;
            struct Input { float4 vertex : POSITION; float2 uv : TEXCOORD0; half4 colour : COLOR; };
            struct Varyings { float4 position : SV_POSITION; float2 uv : TEXCOORD0; half4 colour : COLOR; };
            Varyings Vertex(Input input)
            {
                Varyings output;
                output.position = TransformObjectToHClip(input.vertex.xyz);
                output.uv = input.uv; output.colour = input.colour;
                return output;
            }
            half4 Fragment(Varyings input) : SV_Target
            {
                half width = 1 - smoothstep(.12, .5, abs(input.uv.x - .5));
                half ends = smoothstep(0, .08, input.uv.y) * (1 - smoothstep(.65, 1, input.uv.y));
                half daylight = lerp(1, .12, saturate(_MotuCloudNightStrength));
                return half4(input.colour.rgb * daylight + _SquallFlash * .15, input.colour.a * width * ends * .65);
            }
            ENDHLSL
        }
    }
    FallBack Off
}
