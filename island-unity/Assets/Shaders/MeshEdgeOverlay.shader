Shader "Motu/Mesh Edge Overlay"
{
    Properties
    {
        _Color ("Edge Colour", Color) = (0, 0, 0, 1)
        [Enum(UnityEngine.Rendering.CompareFunction)] _ZTest ("Depth Test", Float) = 4
    }

    SubShader
    {
        Tags
        {
            "Queue" = "Overlay+100"
            "RenderType" = "Opaque"
            "IgnoreProjector" = "True"
        }

        Pass
        {
            Cull Off
            Lighting Off
            ZWrite Off
            ZTest [_ZTest]
            Offset -1, -1
            Blend Off

            CGPROGRAM
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma target 2.0
            #include "UnityCG.cginc"

            struct VertexInput
            {
                float4 position : POSITION;
            };

            struct FragmentInput
            {
                float4 position : SV_POSITION;
            };

            fixed4 _Color;

            FragmentInput Vert(VertexInput input)
            {
                FragmentInput output;
                output.position = UnityObjectToClipPos(input.position);
                return output;
            }

            fixed4 Frag(FragmentInput input) : SV_Target
            {
                return fixed4(_Color.rgb, 1.0);
            }
            ENDCG
        }
    }
}
