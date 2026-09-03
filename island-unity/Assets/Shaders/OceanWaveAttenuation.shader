Shader "Hidden/Motu/Ocean Wave Attenuation"
{
    Properties
    {
        [NoScaleOffset] _SeaMask ("Sea Mask", 2D) = "white" {}
        [HideInInspector] _IslandWorldSize ("Island World Size", Float) = 2000
        [HideInInspector] _CompositionWorldRect ("Composition World Rect", Vector) = (0, 0, 1, 1)
        [HideInInspector] _DepthAllowancePower ("Depth Allowance Power", Float) = 1
        [HideInInspector] _DistanceAllowancePower ("Distance Allowance Power", Float) = 1
    }

    SubShader
    {
        Tags { "RenderType" = "Opaque" }
        Cull Off
        ZWrite Off
        ZTest Always
        BlendOp Min
        Blend One One
        ColorMask R

        Pass
        {
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Fragment
            #pragma target 3.5

            #include "UnityCG.cginc"

            sampler2D _SeaMask;
            float4x4 _IslandWorldToLocal;
            float _IslandWorldSize;
            float4 _CompositionWorldRect;
            float _DepthAllowancePower;
            float _DistanceAllowancePower;

            fixed4 Fragment(v2f_img input) : SV_Target
            {
                float2 worldPosition = _CompositionWorldRect.xy
                    + input.uv * _CompositionWorldRect.zw;
                float3 islandLocal = mul(
                    _IslandWorldToLocal,
                    float4(worldPosition.x, 0.0, worldPosition.y, 1.0)).xyz;
                float2 islandUv = islandLocal.xz
                    / max(_IslandWorldSize, 0.001) + 0.5;
                if (any(islandUv < 0.0) || any(islandUv > 1.0))
                {
                    return 1.0;
                }
                half2 seaMask = tex2D(_SeaMask, islandUv).rg;
                half depthAllowance = pow(
                    saturate(1.0h - seaMask.r),
                    max(_DepthAllowancePower, 0.001));
                half distanceAllowance = pow(
                    saturate(seaMask.g),
                    max(_DistanceAllowancePower, 0.001));
                half allowance = min(depthAllowance, distanceAllowance);
                return fixed4(allowance, allowance, allowance, 1.0h);
            }
            ENDCG
        }
    }
}
