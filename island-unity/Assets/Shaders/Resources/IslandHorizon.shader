Shader "Motu/Island Horizon"
{
    SubShader
    {
        Tags { "Queue"="Geometry" "RenderType"="Opaque" }
        Cull Back ZWrite On
        Pass
        {
            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #include "UnityCG.cginc"
            float4 _MotuIslandHorizonColour;
            float4 Vertex(float4 position : POSITION) : SV_POSITION
            {
                return UnityObjectToClipPos(position);
            }
            float4 Fragment() : SV_Target
            {
                return float4(_MotuIslandHorizonColour.rgb, 1);
            }
            ENDCG
        }
    }
}
