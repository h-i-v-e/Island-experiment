Shader "Hidden/Motu/Pack Dual Material Masks"
{
    Properties
    {
        [NoScaleOffset] _MainTex ("First Mask (R Height, G Occlusion)", 2D) = "gray" {}
        [NoScaleOffset] _SecondMask ("Second Mask (R Height, G Occlusion)", 2D) = "gray" {}
    }

    SubShader
    {
        Cull Off
        ZWrite Off
        ZTest Always

        Pass
        {
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 2.0

            #include "UnityCG.cginc"

            sampler2D _MainTex;
            sampler2D _SecondMask;

            fixed4 Fragment(v2f_img input) : SV_Target
            {
                fixed2 first = tex2D(_MainTex, input.uv).rg;
                fixed2 second = tex2D(_SecondMask, input.uv).rg;
                return fixed4(first, second);
            }
            ENDCG
        }
    }

    Fallback Off
}
