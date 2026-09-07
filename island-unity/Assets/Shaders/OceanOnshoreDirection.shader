Shader "Hidden/Motu/Ocean Onshore Direction"
{
    Properties
    {
        [NoScaleOffset] _MainTex ("Wave Attenuation", 2D) = "white" {}
        [HideInInspector] _CompositionWorldRect ("Composition World Rect", Vector) = (0, 0, 1024, 1024)
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
            #include "SeaMaskCommon.cginc"

            sampler2D _MainTex;
            float4 _MainTex_TexelSize;
            float4 _CompositionWorldRect;

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

                // Convert the normalized distance differences into metres per
                // world metre, so range and texture resolution do not weaken waves.
                float2 sampleSpanMetres = max(
                    2.0 * abs(_MainTex_TexelSize.xy) * _CompositionWorldRect.zw,
                    float2(0.001, 0.001));
                float2 offshoreGradient = float2(right - left, up - down)
                    * MotuSeaMaskLandDistanceMetres / sampleSpanMetres;
                float gradientLength = length(offshoreGradient);
                float2 onshoreDirection = gradientLength > 1.0e-5
                    ? -offshoreGradient / gradientLength
                    : float2(0.0, 0.0);

                // Fade in offshore from 128 to 96 m, then soften the last 16 m
                // approaching land.
                float coastDistance = centre * MotuSeaMaskLandDistanceMetres;
                float coastalBand = smoothstep(2.0, 16.0, coastDistance)
                    * (1.0 - smoothstep(96.0, MotuSeaMaskLandDistanceMetres, coastDistance));
                float influence = coastalBand
                    * saturate(gradientLength)
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
