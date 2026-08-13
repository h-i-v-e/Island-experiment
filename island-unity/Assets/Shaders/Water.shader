Shader "Motu/Water"
{
    Properties
    {
        _Color ("Color", Color) = (0.03, 0.3, 0.65, 0.65)
    }
    SubShader
    {
        Tags { "Queue"="Transparent" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        Cull Off

        Pass
        {
            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma multi_compile_fog
            #include "UnityCG.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
            };

            struct VertexOutput
            {
                float4 position : SV_POSITION;
                float brightness : TEXCOORD0;
                UNITY_FOG_COORDS(1)
            };

            fixed4 _Color;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.position = UnityObjectToClipPos(input.vertex);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.brightness = 0.72 + 0.28 * saturate(dot(normal, normalize(float3(0.3, 1.0, 0.2))));
                UNITY_TRANSFER_FOG(output, output.position);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                fixed4 color = fixed4(_Color.rgb * input.brightness, _Color.a);
                UNITY_APPLY_FOG(input.fogCoord, color);
                return color;
            }
            ENDCG
        }
    }
}
