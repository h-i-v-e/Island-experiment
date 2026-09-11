Shader "Motu/Coastal Water Overlay"
{
    Properties
    {
        _Color ("Shallow Water Tint", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _SeaMask ("Sea Depth And Land Distance", 2D) = "black" {}
        _WorldSize ("Island World Size", Float) = 2000
        _CoastalOpacity ("Shallow Tint Opacity", Range(0, 1)) = 0.16
        _EdgeFadeMetres ("Patch Edge Fade", Float) = 24
    }

    SubShader
    {
        Tags
        {
            "Queue" = "Transparent+5"
            "RenderType" = "Transparent"
            "IgnoreProjector" = "True"
        }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        ZTest LEqual
        Cull Off
        Offset -1, -1

        Pass
        {
            Tags { "LightMode" = "ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile_fog
            #pragma multi_compile_fwdbase

            #include "WaterCommon.cginc"
            #include "SeaMaskCommon.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float3 worldPosition : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float3 islandLocalPosition : TEXCOORD2;
                UNITY_FOG_COORDS(3)
                SHADOW_COORDS(4)
            };

            sampler2D _SeaMask;
            float _WorldSize;
            half _CoastalOpacity;
            float _EdgeFadeMetres;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.pos = UnityObjectToClipPos(input.vertex);
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.worldNormal = UnityObjectToWorldNormal(input.normal);
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float2 seaMaskUv =
                    input.islandLocalPosition.xz / max(_WorldSize, 0.001) + 0.5;
                float nearestPatchEdge = min(
                    min(seaMaskUv.x, 1.0 - seaMaskUv.x),
                    min(seaMaskUv.y, 1.0 - seaMaskUv.y));
                half patchFade = smoothstep(
                    0.0,
                    max(_EdgeFadeMetres / max(_WorldSize, 0.001), 0.0001),
                    nearestPatchEdge);
                if (patchFade <= 0.0001h)
                    discard;

                half2 seaMask = tex2D(_SeaMask, saturate(seaMaskUv)).rg;
                float landDistance = seaMask.g * MotuSeaMaskLandDistanceMetres;
                half nearShoreDistance = saturate(landDistance / MotuOceanAttenuationDistanceMetres);
                half shallowTint = seaMask.r
                    * lerp(0.35h, 1.0h, 1.0h - nearShoreDistance);
                half alpha = patchFade * shallowTint * _CoastalOpacity;
                if (alpha <= 0.001h)
                    discard;

                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 worldNormal = MotuFacingWaterNormal(
                    input.worldNormal,
                    viewDirection);
                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                fixed3 illumination = MotuWaterIllumination(
                    worldNormal,
                    input.worldPosition,
                    shadowAttenuation,
                    0.20h,
                    cloud);
                fixed3 shallowColour = _Color.rgb * illumination;
                fixed4 result = fixed4(
                    shallowColour,
                    alpha);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }
    }
}
