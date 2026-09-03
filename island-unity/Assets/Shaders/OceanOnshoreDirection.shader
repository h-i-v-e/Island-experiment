Shader "Hidden/Motu/Ocean Onshore Direction"
{
    Properties
    {
        [NoScaleOffset] _MainTex ("Wave Attenuation", 2D) = "white" {}
    }

    SubShader
    {
        Tags { "RenderType" = "Opaque" }
        Cull Off
        ZWrite Off
        ZTest Always

        Pass
        {
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.5

            #include "UnityCG.cginc"

            sampler2D _MainTex;
            float4 _MainTex_TexelSize;

            fixed4 Fragment(v2f_img input) : SV_Target
            {
                float2 texelX = float2(_MainTex_TexelSize.x, 0.0);
                float2 texelY = float2(0.0, _MainTex_TexelSize.y);
                float centre = tex2D(_MainTex, input.uv).g;
                float riverAllowance = tex2D(_MainTex, input.uv).a;
                float left = tex2D(_MainTex, input.uv - texelX).g;
                float right = tex2D(_MainTex, input.uv + texelX).g;
                float down = tex2D(_MainTex, input.uv - texelY).g;
                float up = tex2D(_MainTex, input.uv + texelY).g;

                // The guide averages depth allowance and distance-to-shore;
                // its negative gradient points towards shallower coastal water.
                float2 offshoreGradient = float2(right - left, up - down);
                float gradientLength = length(offshoreGradient);
                float2 onshoreDirection = gradientLength > 1.0e-5
                    ? -offshoreGradient / gradientLength
                    : float2(0.0, 0.0);

                // Begin beyond the old depth-only band, then fade out again
                // immediately before the surface becomes fully flat.
                float coastalBand = smoothstep(0.02, 0.16, centre)
                    * (1.0 - smoothstep(0.72, 0.98, centre));
                float influence = coastalBand
                    * saturate(gradientLength * 24.0)
                    * smoothstep(0.15, 0.85, riverAllowance);
                return fixed4(
                    onshoreDirection * 0.5 + 0.5,
                    influence,
                    centre);
            }
            ENDCG
        }
    }
}
