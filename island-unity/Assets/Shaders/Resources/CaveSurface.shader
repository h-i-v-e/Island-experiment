Shader "Motu/Cave Surface"
{
    Properties
    {
        _Color ("Island Tint", Color) = (1,1,1,1)
        _CliffNoise3D ("Island Rock Noise", 3D) = "gray" {}
        _CliffNoisePeriod ("Cliff Noise Period", Float) = 160
        _CliffNoiseDetailScale ("Cliff Detail Frequency", Float) = 16
        _CliffNormalStrength ("Cliff Normal Strength", Float) = 0.12
        _TerrainAlbedoArray ("Island Surface Colours", 2DArray) = "" {}
        _DirtSize ("Floor Repeat Metres", Float) = 3
        _UseTextures ("Use Island Textures", Float) = 0
        _RockColour ("Stone", Color) = (0.3,0.32,0.29,1)
        _DirtColour ("Floor", Color) = (0.18,0.13,0.08,1)
    }
    SubShader
    {
        Tags { "RenderType"="Opaque" "Queue"="Geometry" }
        LOD 200
        CGINCLUDE
            #include "UnityCG.cginc"
            #include "Lighting.cginc"
            #include "AutoLight.cginc"
            #include "../CloudCommon.cginc"

            UNITY_DECLARE_TEX2DARRAY(_TerrainAlbedoArray);
            float _DirtSize, _UseTextures;
            sampler3D _CliffNoise3D;
            float _CliffNoisePeriod, _CliffNoiseDetailScale, _CliffNormalStrength;
            fixed4 _Color;
            #include "../TerrainRockCommon.cginc"
            fixed4 _RockColour, _DirtColour;
            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 caveData : TEXCOORD2;
            };
            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float3 localPosition : TEXCOORD0;
                float3 localNormal : TEXCOORD1;
                float2 caveData : TEXCOORD2;
                float3 worldPosition : TEXCOORD3;
                float3 worldNormal : TEXCOORD4;
                UNITY_LIGHTING_COORDS(5, 6)
                UNITY_FOG_COORDS(7)
            };
            VertexOutput Vertex(VertexInput v)
            {
                VertexOutput o;
                UNITY_INITIALIZE_OUTPUT(VertexOutput, o);
                o.pos = UnityObjectToClipPos(v.vertex);
                o.localPosition = v.vertex.xyz;
                o.localNormal = v.normal;
                o.caveData = v.caveData;
                o.worldPosition = mul(unity_ObjectToWorld, v.vertex).xyz;
                o.worldNormal = UnityObjectToWorldNormal(v.normal);
                UNITY_TRANSFER_LIGHTING(o, float2(0, 0));
                UNITY_TRANSFER_FOG(o, o.pos);
                return o;
            }
            fixed3 Triplanar(float3 p, float3 weights, float layer, float size)
            {
                p /= max(size, 0.1);
                return UNITY_SAMPLE_TEX2DARRAY(_TerrainAlbedoArray, float3(p.yz, layer)).rgb * weights.x
                    + UNITY_SAMPLE_TEX2DARRAY(_TerrainAlbedoArray, float3(p.xz, layer)).rgb * weights.y
                    + UNITY_SAMPLE_TEX2DARRAY(_TerrainAlbedoArray, float3(p.xy, layer)).rgb * weights.z;
            }
            fixed4 Fragment(VertexOutput i) : SV_Target
            {
                float3 localNormal = normalize(i.localNormal);
                float3 weights = pow(abs(localNormal), 4);
                weights /= max(dot(weights, float3(1,1,1)), 0.0001);
                float floorBlend = smoothstep(0.65, 0.95, localNormal.y);
                fixed3 rock = _RockColour.rgb, dirt = _DirtColour.rgb;
                if (_UseTextures > 0.5)
                {
                    // Steep exterior rock uses the configured stone palette;
                    // its authored top-down recipe is not applied to walls.
                    dirt = Triplanar(i.localPosition, weights, 0, _DirtSize);
                }
                fixed3 albedo = lerp(rock, dirt, floorBlend) * _Color.rgb;
                // Match terrain/rock lighting and cloud attenuation at the mouth.
                // Only ambient light is reduced by cover; direct light uses shadows.
                float3 normal = normalize(i.worldNormal);
                half3 broadNoise = tex3D(_CliffNoise3D,
                    i.localPosition / max(_CliffNoisePeriod, 1.0)).rgb * 2.0h - 1.0h;
                normal = normalize(lerp(MotuProceduralRockNormal(
                    normal, i.localPosition, broadNoise), normal, floorBlend));
                float3 lightDirection = normalize(UnityWorldSpaceLightDir(i.worldPosition));
                UNITY_LIGHT_ATTENUATION(attenuation, i, i.worldPosition);
                half3 direct = _LightColor0.rgb * saturate(dot(normal, lightDirection))
                    * attenuation;
                #if defined(UNITY_PASS_FORWARDADD)
                    // Local lights (including the player torch) are unaffected
                    // by overhead cloud cover or the cave's ambient darkness.
                    fixed4 colour = fixed4(albedo * direct, 0);
                    UNITY_APPLY_FOG_COLOR(i.fogCoord, colour, fixed4(0,0,0,0));
                #else
                    MotuCloudLighting cloud = MotuCloudSurfaceLighting(i.worldPosition);
                    half3 ambient = ShadeSH9(half4(normal, 1)) * cloud.ambientTransmittance
                        * clamp(i.caveData.x, 0.08, 1);
                    #if defined(DIRECTIONAL) || defined(DIRECTIONAL_COOKIE)
                        direct *= cloud.directTransmittance;
                    #endif
                    fixed4 colour = fixed4(albedo * (ambient + direct), 1);
                    UNITY_APPLY_FOG(i.fogCoord, colour);
                #endif
                return colour;
            }
        ENDCG
        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            ENDCG
        }
        Pass
        {
            Tags { "LightMode"="ForwardAdd" }
            Blend One One
            ZWrite Off
            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile_fwdadd_fullshadows
            #pragma multi_compile_fog
            ENDCG
        }
        UsePass "Legacy Shaders/VertexLit/SHADOWCASTER"
    }
    FallBack "Diffuse"
}
