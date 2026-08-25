Shader "Hidden/ProceduralMaterialStudio/Preview"
{
    Properties
    {
        _MainTex ("Albedo", 2D) = "white" {}
        _BumpMap ("Normal", 2D) = "bump" {}
        _HeightTex ("R16 Height", 2D) = "black" {}
        _OcclusionTex ("Occlusion", 2D) = "white" {}
        _HeightRange ("Height Range", Vector) = (0, 1, 0.5, 0)
        _HeightDisplacementScale ("Height Displacement Scale", Range(0, 8)) = 2
        _NormalStrength ("Normal Strength", Range(0, 2)) = 1
        _NormalGreenSign ("Normal Green Sign", Float) = -1
        _UseHeight ("Use Height", Float) = 0
        _UseNormal ("Use Normal", Float) = 0
        _UseOcclusion ("Use Occlusion", Float) = 0
        _OcclusionStrength ("Occlusion Strength", Range(0, 1)) = 1
        _LightDirection ("Light Direction", Vector) = (0.35, 0.8, 0.45, 0)
        _LightStrength ("Light Strength", Range(0, 3)) = 1.2
    }
    SubShader
    {
        Tags { "RenderType" = "Opaque" "Queue" = "Geometry" }
        Pass
        {
            CGPROGRAM
            #pragma vertex vert
            #pragma fragment frag
            #pragma target 3.0
            #include "UnityCG.cginc"

            sampler2D _MainTex;
            sampler2D _BumpMap;
            sampler2D _HeightTex;
            sampler2D _OcclusionTex;
            float4 _MainTex_ST;
            float4 _HeightRange;
            float _HeightDisplacementScale;
            float _NormalStrength;
            float _NormalGreenSign;
            float _UseHeight;
            float _UseNormal;
            float _UseOcclusion;
            float _OcclusionStrength;
            float3 _LightDirection;
            float _LightStrength;

            struct appdata
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float4 tangent : TANGENT;
                float2 uv : TEXCOORD0;
            };

            struct v2f
            {
                float4 position : SV_POSITION;
                float2 uv : TEXCOORD0;
                float3 normal : TEXCOORD1;
                float3 tangent : TEXCOORD2;
                float3 bitangent : TEXCOORD3;
            };

            v2f vert(appdata input)
            {
                v2f output;
                float normalizedHeight = tex2Dlod(_HeightTex, float4(TRANSFORM_TEX(input.uv, _MainTex), 0, 0)).r;
                float physicalHeight = lerp(_HeightRange.x, _HeightRange.y, normalizedHeight) - _HeightRange.z;
                input.vertex.xyz += input.normal * physicalHeight * _HeightDisplacementScale * _UseHeight;
                output.position = UnityObjectToClipPos(input.vertex);
                output.uv = TRANSFORM_TEX(input.uv, _MainTex);
                output.normal = UnityObjectToWorldNormal(input.normal);
                output.tangent = UnityObjectToWorldDir(input.tangent.xyz);
                output.bitangent = cross(output.normal, output.tangent) * input.tangent.w;
                return output;
            }

            fixed4 frag(v2f input) : SV_Target
            {
                fixed3 colour = tex2D(_MainTex, input.uv).rgb;
                fixed3 tangentNormal = UnpackNormal(tex2D(_BumpMap, input.uv));
                tangentNormal = lerp(fixed3(0, 0, 1), tangentNormal, _UseNormal);
                tangentNormal.xy *= _NormalStrength;
                tangentNormal.y *= _NormalGreenSign;
                fixed3 worldNormal = normalize(input.tangent * tangentNormal.x + input.bitangent * tangentNormal.y + input.normal * tangentNormal.z);
                fixed lighting = (saturate(dot(worldNormal, normalize(_LightDirection))) * 0.75 + 0.25) * _LightStrength;
                fixed occlusion = tex2D(_OcclusionTex, input.uv).r;
                lighting *= lerp(1.0, occlusion, _UseOcclusion * _OcclusionStrength);
                return fixed4(colour * lighting, 1.0);
            }
            ENDCG
        }
    }
    Fallback Off
}
