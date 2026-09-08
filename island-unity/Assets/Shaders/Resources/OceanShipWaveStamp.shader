Shader "Hidden/Motu/Ocean Ship Wave Stamp"
{
    Properties { _StampTexture ("Linear displacement stamp", 2D) = "gray" {} }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            Blend One One
            BlendOp Max
            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #include "UnityCG.cginc"
            sampler2D _StampTexture;
            float4 _StampStrength;
            float4 _MotuShipWaveRect;
            struct Output { float4 position : SV_POSITION; float2 uv : TEXCOORD0; };
            Output Vertex(appdata_base input)
            {
                Output output;
                float2 worldXZ = mul(unity_ObjectToWorld, input.vertex).xz;
                output.position = float4(((worldXZ - _MotuShipWaveRect.xy) * _MotuShipWaveRect.zw) * 2.0 - 1.0, 0, 1);
                #if UNITY_UV_STARTS_AT_TOP
                output.position.y = -output.position.y;
                #endif
                output.uv = input.texcoord.xy;
                return output;
            }
            float4 Fragment(Output input) : SV_Target
            {
                float signedHeight = tex2D(_StampTexture, input.uv).r * 2.0 - 1.0;
                // Both 127 and 128 are neutral in an 8-bit authored texture.
                float amount = max(abs(signedHeight) - 1.0 / 255.0, 0.0) / (254.0 / 255.0);
                float border = min(min(input.uv.x, input.uv.y), min(1-input.uv.x, 1-input.uv.y));
                amount *= smoothstep(0, .015, border);
                float down = signedHeight < 0 ? amount : 0;
                float up = signedHeight > 0 ? amount : 0;
                return float4(down * _StampStrength.x, up * _StampStrength.y, up * _StampStrength.z, 0);
            }
            ENDCG
        }
    }
}
