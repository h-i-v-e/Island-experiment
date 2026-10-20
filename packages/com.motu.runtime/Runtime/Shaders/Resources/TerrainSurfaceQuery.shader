Shader "Hidden/Motu/Terrain Surface Query"
{
    SubShader
    {
        Pass
        {
            Cull Off ZWrite Off ZTest Always
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma target 3.5
            #include "../MotuUrp.hlsl"
            #include "../TerrainCoverageCommon.cginc"
            sampler2D _WorldNormal;
            float _SurfaceWorldSize, _SurfaceUseNormalMap;
            struct Input { float3 vertex : POSITION; float3 normal : NORMAL; float4 material : COLOR; float2 environment : TEXCOORD1; };
            struct Varying { float4 pos : SV_POSITION; float3 local : TEXCOORD0; float3 normal : TEXCOORD1; float4 material : TEXCOORD2; float2 environment : TEXCOORD3; };
            Varying Vert(Input input)
            {
                Varying output;
                output.pos = float4(input.vertex.xz / _SurfaceWorldSize * 2.0, 0, 1);
                #if UNITY_UV_STARTS_AT_TOP
                    output.pos.y = -output.pos.y;
                #endif
                output.local = input.vertex; output.normal = input.normal;
                output.material = input.material; output.environment = input.environment;
                return output;
            }
            float4 Frag(Varying input) : SV_Target
            {
                half3 normal = normalize(input.normal);
                if (_SurfaceUseNormalMap > .5)
                {
                    half3 packed = tex2D(_WorldNormal, input.local.xz / _SurfaceWorldSize + .5).rgb;
                    normal = normalize(packed.rbg * 2.0 - 1.0);
                }
                MotuTerrainCoverage c = MotuBuildTerrainCoverage(input.local, normal, input.material, input.environment);
                float surface = 1;
                if (c.grass > .5) surface = 2;
                if (c.forestFloor > .5) surface = 1;
                if (c.beach > .5) surface = 4;
                if (c.river > .5) surface = 6;
                if (c.stones > .5) surface = 5;
                if (c.rock > .5) surface = 3;
                if (c.snow > .5) surface = 7;
                return float4(surface, input.local.y, 0, 1);
            }
            ENDHLSL
        }
    }
}
