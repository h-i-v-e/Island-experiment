Shader "Hidden/Motu/Island Shelter Land"
{
    Properties
    {
        [HideInInspector] _IslandWorldSize ("Island World Size", Float) = 2000
    }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Cull Off ZWrite Off ZTest Always
        Pass
        {
            HLSLPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #include "../MotuUrp.hlsl"

            float _IslandWorldSize;

            struct Varyings
            {
                float4 position : SV_POSITION;
                float elevation : TEXCOORD0;
            };

            Varyings Vertex(float4 position : POSITION)
            {
                Varyings output;
                // This footprint uses the island root's centred X/Z metres;
                // rendering and camera matrices deliberately do not participate.
                float2 clipPosition = position.xz * (2.0 / max(_IslandWorldSize, 1.0));
                #if UNITY_UV_STARTS_AT_TOP
                    clipPosition.y = -clipPosition.y;
                #endif
                output.position = float4(clipPosition, 0.0, 1.0);
                output.elevation = position.y;
                return output;
            }

            float4 Fragment(Varyings input) : SV_Target
            {
                // The horizon mesh contains seabed too. Only above-sea terrain
                // may cast shelter; keep the coastal sea-mask R/G convention.
                clip(input.elevation - 0.001);
                return float4(1.0, 0.0, 0.0, 1.0);
            }
            ENDHLSL
        }
    }
}
