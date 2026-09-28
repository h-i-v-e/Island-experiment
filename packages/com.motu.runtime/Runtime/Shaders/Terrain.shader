Shader "Motu/Terrain"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
    }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" "RenderType"="Opaque" "MotuReflection"="Terrain" }
        LOD 200

        Pass
        {
            Tags { "LightMode"="UniversalForwardOnly" }
            HLSLPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma multi_compile _ _MAIN_LIGHT_SHADOWS _MAIN_LIGHT_SHADOWS_CASCADE
            #pragma multi_compile_fragment _ _SHADOWS_SOFT
            #pragma multi_compile _ _ADDITIONAL_LIGHTS
            #pragma multi_compile_fragment _ _ADDITIONAL_LIGHT_SHADOWS
            #pragma multi_compile_fog
            #include "MotuUrp.hlsl"
            half4 _Color;

            struct Attributes { float4 vertex : POSITION; float3 normal : NORMAL; float2 uv : TEXCOORD0; };
            struct Varyings { float4 pos : SV_POSITION; float3 worldPosition : TEXCOORD0; half3 worldNormal : TEXCOORD1; float2 uv : TEXCOORD2; float fog : TEXCOORD3; };
            Varyings Vertex(Attributes input)
            {
                Varyings output;
                output.pos = TransformObjectToHClip(input.vertex.xyz);
                output.worldPosition = TransformObjectToWorld(input.vertex.xyz);
                output.worldNormal = TransformObjectToWorldNormal(input.normal);
                output.uv = input.uv;
                output.fog = ComputeFogFactor(output.pos.z);
                return output;
            }
            half4 Fragment(Varyings input) : SV_Target
            {

            float elevation = input.worldPosition.y;
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
            half3 albedo = baseColor * _Color.rgb;

                half3 normal = normalize(input.worldNormal);
                InputData lighting = (InputData)0;
                lighting.positionWS = input.worldPosition;
                lighting.normalWS = normal;
                lighting.viewDirectionWS = GetWorldSpaceNormalizeViewDir(input.worldPosition);
                lighting.shadowCoord = TransformWorldToShadowCoord(input.worldPosition);
                lighting.bakedGI = SampleSH(normal);
                lighting.normalizedScreenSpaceUV = GetNormalizedScreenSpaceUV(input.pos);
                lighting.shadowMask = half4(1,1,1,1);
                SurfaceData surface = (SurfaceData)0;
                surface.albedo = albedo;
                surface.smoothness = 0.08;
                surface.occlusion = 1;
                surface.alpha = 1;
                half4 colour = UniversalFragmentPBR(lighting, surface);
                colour.rgb = MixFog(colour.rgb, input.fog);
                return colour;
            }
            ENDHLSL
        }
        UsePass "Hidden/Motu/Depth/ShadowCaster"
        UsePass "Motu/Planar Reflection Simplified/ReflectionTerrain"

    }
    FallBack Off
}
