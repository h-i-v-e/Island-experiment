Shader "Motu/Terrain"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        _WorldSize ("World Size", Float) = 2000
    }
    SubShader
    {
        Tags { "RenderType"="Opaque" }
        LOD 200

        CGPROGRAM
        #pragma surface Surface Standard fullforwardshadows addshadow
        #pragma target 3.0

        fixed4 _Color;
        float _WorldSize;

        struct Input
        {
            float3 worldPos;
            float3 worldNormal;
        };

        void Surface(Input input, inout SurfaceOutputStandard output)
        {
            float elevation = input.worldPos.y * (100.0 / max(_WorldSize, 1.0));
            float slope = 1.0 - saturate(input.worldNormal.y);
            fixed3 deep = fixed3(0.08, 0.16, 0.12);
            fixed3 sand = fixed3(0.62, 0.57, 0.34);
            fixed3 grass = fixed3(0.20, 0.48, 0.16);
            fixed3 rock = fixed3(0.34, 0.32, 0.29);
            fixed3 snow = fixed3(0.82, 0.84, 0.81);

            fixed3 baseColor = elevation < 0.0
                ? lerp(deep, sand, saturate((elevation + 8.0) / 8.0))
                : lerp(grass, rock, saturate(slope * 2.2));
            baseColor = lerp(baseColor, snow, saturate((elevation - 13.0) / 5.0));
            output.Albedo = baseColor * _Color.rgb;
            output.Smoothness = 0.08;
            output.Metallic = 0.0;
        }
        ENDCG
    }
    FallBack "Diffuse"
}
