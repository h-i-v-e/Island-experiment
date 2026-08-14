Shader "Motu/River Spray Particle"
{
    Properties
    {
        _TintColor ("Tint", Color) = (1, 1, 1, 1)
    }

    SubShader
    {
        Tags
        {
            "Queue" = "Transparent"
            "RenderType" = "Transparent"
            "IgnoreProjector" = "True"
            "PreviewType" = "Plane"
        }

        Cull Off
        Lighting Off
        ZWrite Off
        Blend SrcAlpha OneMinusSrcAlpha

        Pass
        {
            CGPROGRAM
            #pragma vertex Vert
            #pragma fragment Frag
            #include "UnityCG.cginc"

            struct VertexInput
            {
                float4 position : POSITION;
                fixed4 color : COLOR;
                float2 uv : TEXCOORD0;
            };

            struct FragmentInput
            {
                float4 position : SV_POSITION;
                fixed4 color : COLOR;
                float2 uv : TEXCOORD0;
            };

            fixed4 _TintColor;

            FragmentInput Vert(VertexInput input)
            {
                FragmentInput output;
                output.position = UnityObjectToClipPos(input.position);
                output.color = input.color * _TintColor;
                output.uv = input.uv;
                return output;
            }

            fixed4 Frag(FragmentInput input) : SV_Target
            {
                float radius = length(input.uv * 2.0 - 1.0);
                float fade = saturate((1.0 - radius) / 0.6);
                fade = fade * fade * (3.0 - 2.0 * fade);
                return fixed4(input.color.rgb, input.color.a * fade);
            }
            ENDCG
        }
    }
}
