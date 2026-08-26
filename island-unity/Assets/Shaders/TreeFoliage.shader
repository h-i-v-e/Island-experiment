Shader "Motu/Tree Foliage"
{
    Properties
    {
        _BaseColor ("Shadow Green", Color) = (0.08, 0.28, 0.055, 1)
        _LightColor ("Sunlit Green", Color) = (0.22, 0.55, 0.12, 1)
        [NoScaleOffset] _CliffNoise3D ("Tree Surface Noise", 3D) = "gray" {}
        _TreeNoisePeriod ("Canopy Noise Period (metres)", Float) = 24
        _TreeNoiseDetailScale ("Canopy Detail Frequency", Range(1, 16)) = 5
        _TreeNoiseFineScale ("Leaf Fine Frequency", Range(2, 48)) = 20
        _TreeNormalStrength ("Leaf Normal Strength", Range(0, 0.5)) = 0.24
        _TreeHueVariationDegrees ("Foliage Hue Variation", Range(0, 30)) = 16
        _CanopyCoverage ("Canopy Coverage", Range(0, 1)) = 0.66
        _CanopyEdgeSoftness ("Canopy Hole Edge Softness", Range(0.001, 0.25)) = 0.08
        _AlphaCutoff ("Canopy Alpha Cutoff", Range(0, 1)) = 0.5
        _FoliageFurHeight ("Foliage Fur Depth (metres)", Range(0.05, 1.5)) = 0.45
        _FoliageLeafWorldSize ("Broad Leaf Texture Size (metres)", Range(0.1, 4)) = 1.25
        _FoliageLeafCoverage ("Leaf Shell Coverage", Range(0, 1)) = 0.72
        _FoliageLeafEdgeSoftness ("Leaf Edge Softness", Range(0.001, 0.25)) = 0.08
        [Enum(UnityEngine.Rendering.CullMode)] _CullMode ("Base Surface Culling", Float) = 2
        [HideInInspector] _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
        _GrassRadius ("Foliage Fur Outer Radius (metres)", Float) = 20
        _GrassFadeWidth ("Foliage Fur Edge Fade (metres)", Range(0.1, 20)) = 10
    }

    SubShader
    {
        Tags { "RenderType"="TransparentCutout" "Queue"="AlphaTest" "IgnoreProjector"="True" }
        LOD 350

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull [_CullMode]
            AlphaToMask On

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing

            #include "UnityCG.cginc"
            #include "Lighting.cginc"
            #include "AutoLight.cginc"
            #include "TreeSurfaceNoise.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float3 worldPosition : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                SHADOW_COORDS(2)
                UNITY_FOG_COORDS(3)
                float3 islandLocalPosition : TEXCOORD4;
                UNITY_VERTEX_INPUT_INSTANCE_ID
                UNITY_VERTEX_OUTPUT_STEREO
            };

            fixed4 _BaseColor;
            fixed4 _LightColor;
            half _CanopyCoverage;
            half _CanopyEdgeSoftness;
            half _AlphaCutoff;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                UNITY_SETUP_INSTANCE_ID(input);
                UNITY_INITIALIZE_OUTPUT(VertexOutput, output);
                UNITY_TRANSFER_INSTANCE_ID(input, output);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                output.pos = UnityObjectToClipPos(input.vertex);
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                output.worldNormal = UnityObjectToWorldNormal(input.normal);
                TRANSFER_SHADOW(output);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                UNITY_SETUP_INSTANCE_ID(input);
                half3 normal = normalize(input.worldNormal);
                MotuTreeNoiseSample noise = MotuSampleTreeNoise(
                    input.islandLocalPosition);
                normal = MotuPerturbTreeNormal(normal, noise);
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half diffuse = saturate(dot(normal, lightDirection));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                half lightBlend = saturate(
                    0.18 + diffuse * 0.72 + noise.broad.g * 0.10);
                half materialAlpha = lerp(
                    _BaseColor.a,
                    _LightColor.a,
                    lightBlend);
                half alpha = MotuTreeCanopyAlpha(
                    input.islandLocalPosition,
                    _CanopyCoverage,
                    _CanopyEdgeSoftness,
                    materialAlpha);
                clip(alpha - _AlphaCutoff);
                fixed3 albedo = MotuRotateTreeHue(
                    lerp(_BaseColor.rgb, _LightColor.rgb, lightBlend),
                    noise.hue);
                fixed4 result = fixed4(
                    albedo
                        * (ShadeSH9(half4(normal, 1.0))
                            + _LightColor0.rgb * diffuse * attenuation),
                    alpha);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.125
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.25
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.375
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.5
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.625
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.75
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 0.875
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            Cull Off
            ZWrite Off
            Blend SrcAlpha OneMinusSrcAlpha

            CGPROGRAM
            #define FOLIAGE_SHELL_LAYER 1.0
            #pragma vertex FoliageFurVertex
            #pragma fragment FoliageFurFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            #include "TreeFoliageFurCommon.cginc"
            ENDCG
        }

        Pass
        {
            Name "ShadowCaster"
            Tags { "LightMode"="ShadowCaster" }
            Cull Off
            ZWrite On
            ZTest LEqual

            CGPROGRAM
            #pragma vertex ShadowVertex
            #pragma fragment ShadowFragment
            #pragma target 3.0
            #pragma multi_compile_shadowcaster
            #pragma multi_compile_instancing

            #include "UnityCG.cginc"
            #include "TreeSurfaceNoise.cginc"

            fixed4 _BaseColor;
            fixed4 _LightColor;
            half _CanopyCoverage;
            half _CanopyEdgeSoftness;
            half _AlphaCutoff;

            struct ShadowInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct ShadowOutput
            {
                V2F_SHADOW_CASTER;
                float3 islandLocalPosition : TEXCOORD1;
                UNITY_VERTEX_OUTPUT_STEREO
            };

            ShadowOutput ShadowVertex(ShadowInput v)
            {
                ShadowOutput output;
                UNITY_SETUP_INSTANCE_ID(v);
                UNITY_INITIALIZE_OUTPUT(ShadowOutput, output);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                TRANSFER_SHADOW_CASTER_NORMALOFFSET(output)
                float3 worldPosition = mul(unity_ObjectToWorld, v.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(worldPosition, 1.0)).xyz;
                return output;
            }

            float4 ShadowFragment(ShadowOutput input) : SV_Target
            {
                half materialAlpha = min(_BaseColor.a, _LightColor.a);
                half alpha = MotuTreeCanopyAlpha(
                    input.islandLocalPosition,
                    _CanopyCoverage,
                    _CanopyEdgeSoftness,
                    materialAlpha);
                clip(alpha - _AlphaCutoff);
                SHADOW_CASTER_FRAGMENT(input)
            }
            ENDCG
        }
    }

    FallBack "Transparent/Cutout/VertexLit"
}
