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
            float4 _StampClampShape;
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
            float ClampSample(float2 uv)
            {
                float signedHeight = tex2D(_StampTexture, saturate(uv)).r * 2.0 - 1.0;
                float down = max(-signedHeight - 1.0 / 255.0, 0.0) / (254.0 / 255.0);
                float border = min(min(uv.x, uv.y), min(1-uv.x, 1-uv.y));
                return down * smoothstep(0, .015, border);
            }

            float HullClamp(float2 uv)
            {
                float2 centre = (uv - .5) / max(_StampClampShape.x, .5) + .5;
                float2 radius = _StampClampShape.yz;
                if (max(radius.x, radius.y) <= .00001) return ClampSample(centre);
                // A separable tent filter softens the inward-scaled footprint.
                // Only black/clamping samples participate, preserving authored bow heights.
                float down = 4 * ClampSample(centre);
                down += 2 * (ClampSample(centre + float2(radius.x, 0))
                    + ClampSample(centre - float2(radius.x, 0))
                    + ClampSample(centre + float2(0, radius.y))
                    + ClampSample(centre - float2(0, radius.y)));
                down += ClampSample(centre + radius) + ClampSample(centre - radius)
                    + ClampSample(centre + float2(radius.x, -radius.y))
                    + ClampSample(centre + float2(-radius.x, radius.y));
                return down / 16.0;
            }

            float4 Fragment(Output input) : SV_Target
            {
                float signedHeight = tex2D(_StampTexture, input.uv).r * 2.0 - 1.0;
                // Both 127 and 128 are neutral in an 8-bit authored texture.
                float amount = max(abs(signedHeight) - 1.0 / 255.0, 0.0) / (254.0 / 255.0);
                float border = min(min(input.uv.x, input.uv.y), min(1-input.uv.x, 1-input.uv.y));
                amount *= smoothstep(0, .015, border);
                float down = HullClamp(input.uv);
                float up = signedHeight > 0 ? amount : 0;
                return float4(down * _StampStrength.x, up * _StampStrength.y, up * _StampStrength.z, 0);
            }
            ENDCG
        }
    }
}
