Shader "Motu/Tree Wood"
{
    Properties
    {
        _BaseColor ("Dark Bark", Color) = (0.24, 0.105, 0.045, 1)
        _LightColor ("Light Bark", Color) = (0.43, 0.22, 0.09, 1)
        _BarkContrast ("Bark Colour Variation", Range(0, 1)) = 0.35
        [NoScaleOffset] _CliffNoise3D ("Tree Surface Noise", 3D) = "gray" {}
        _TreeNoisePeriod ("Bark Noise Period (metres)", Float) = 7
        _TreeNoiseDetailScale ("Bark Detail Frequency", Range(1, 16)) = 4
        _TreeNoiseFineScale ("Bark Fine Frequency", Range(2, 48)) = 18
        _TreeNormalStrength ("Bark Normal Strength", Range(0, 0.5)) = 0.18
        _TreeHueVariationDegrees ("Bark Hue Variation", Range(0, 30)) = 8
    }

    SubShader
    {
        Tags { "RenderType"="Opaque" "Queue"="Geometry" }
        LOD 200

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

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
            half _BarkContrast;

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
                half irregular = noise.broad.r * 0.25
                    + noise.detail.g * 0.45
                    + noise.fine.g * 0.30;
                half bark = saturate(
                    0.5 + irregular * _BarkContrast);
                fixed3 albedo = MotuRotateTreeHue(
                    lerp(_BaseColor.rgb, _LightColor.rgb, bark),
                    noise.hue);
                half3 ambient = ShadeSH9(half4(normal, 1.0));
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half diffuse = saturate(dot(normal, lightDirection));
                UNITY_LIGHT_ATTENUATION(attenuation, input, input.worldPosition);
                fixed4 result = fixed4(
                    albedo * (ambient + _LightColor0.rgb * diffuse * attenuation),
                    1.0);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ShadowCaster" }
            ZWrite On
            ZTest LEqual

            CGPROGRAM
            #pragma vertex ShadowVertex
            #pragma fragment ShadowFragment
            #pragma target 3.0
            #pragma multi_compile_shadowcaster
            #pragma multi_compile_instancing

            #include "UnityCG.cginc"

            struct ShadowInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                UNITY_VERTEX_INPUT_INSTANCE_ID
            };

            struct ShadowOutput
            {
                V2F_SHADOW_CASTER;
                UNITY_VERTEX_OUTPUT_STEREO
            };

            ShadowOutput ShadowVertex(ShadowInput v)
            {
                ShadowOutput output;
                UNITY_SETUP_INSTANCE_ID(v);
                UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
                TRANSFER_SHADOW_CASTER_NORMALOFFSET(output)
                return output;
            }

            float4 ShadowFragment(ShadowOutput input) : SV_Target
            {
                SHADOW_CASTER_FRAGMENT(input)
            }
            ENDCG
        }
    }

    FallBack "Diffuse"
}
