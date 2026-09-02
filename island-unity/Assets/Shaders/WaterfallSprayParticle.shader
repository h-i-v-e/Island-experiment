Shader "Motu/Waterfall Spray Particle"
{
    Properties
    {
        _TintColor ("Tint", Color) = (0.86, 0.93, 0.96, 0.72)
    }

    SubShader
    {
        Tags
        {
            "Queue" = "Transparent+25"
            "RenderType" = "Transparent"
            "IgnoreProjector" = "True"
        }

        Pass
        {
            Tags { "LightMode" = "ForwardBase" }
            Cull Off
            ZWrite Off
            ZTest LEqual
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #pragma target 3.5
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma multi_compile_fog
            #pragma multi_compile_fwdbase
            #include "UnityCG.cginc"
            #include "AutoLight.cginc"
            #include "WaterfallSprayLighting.cginc"

            fixed4 _TintColor;

            struct VertexInput
            {
                float4 vertex : POSITION;
                fixed4 color : COLOR;
                float2 uv : TEXCOORD0;
            };

            struct FragmentInput
            {
                float4 position : SV_POSITION;
                fixed4 color : COLOR;
                float2 uv : TEXCOORD0;
                float3 worldPosition : TEXCOORD1;
                UNITY_FOG_COORDS(2)
                SHADOW_COORDS(3)
            };

            FragmentInput Vert(VertexInput input)
            {
                FragmentInput output;
                output.position = UnityObjectToClipPos(input.vertex);
                output.color = input.color;
                output.uv = input.uv;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                UNITY_TRANSFER_FOG(output, output.position);
                return output;
            }

            fixed4 Frag(FragmentInput input) : SV_Target
            {
                float2 centred = input.uv * 2.0 - 1.0;
                half softDroplet = 1.0h - smoothstep(
                    0.18h,
                    1.0h,
                    dot(centred, centred));
                half alpha = softDroplet * input.color.a * _TintColor.a;
                if (alpha <= 0.003h)
                {
                    discard;
                }

                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                fixed3 colour = MotuWaterfallSprayLighting(
                    input.color.rgb * _TintColor.rgb,
                    input.worldPosition,
                    shadowAttenuation);
                fixed4 result = fixed4(colour, alpha);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }
    }
}
