Shader "Hidden/Motu/Terrain Surface Query"
{
    Properties
    {
        [NoScaleOffset] [HideInInspector] _MotuWheatGroundMask ("Wheat ground coverage", 2D) = "black" {}
        [HideInInspector] _MotuWheatGroundUV ("Wheat ground coordinates", Vector) = (0, 0, 0, 0)
    }
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
            float _SurfaceWorldSize;
            struct Input
            {
                float3 vertex : POSITION;
                float4 material : COLOR;
                float4 environment : TEXCOORD1;
            };
            struct Varying
            {
                float4 pos : SV_POSITION;
                float3 local : TEXCOORD0;
                float4 material : TEXCOORD1;
                float4 environment : TEXCOORD2;
            };
            Varying Vert(Input input)
            {
                Varying output;
                output.pos = float4(input.vertex.xz / _SurfaceWorldSize * 2.0, 0, 1);
                #if UNITY_UV_STARTS_AT_TOP
                    output.pos.y = -output.pos.y;
                #endif
                output.local = input.vertex;
                output.material = input.material;
                output.environment = input.environment;
                return output;
            }
            float4 Frag(Varying input) : SV_Target
            {
                MotuTerrainCoverage c = MotuBuildTerrainCoverage(input.local, input.material, input.environment);
                float surface = c.rock > .5 ? 3 : 1;
                if (c.forestFloor > .5) surface = 1;
                if (c.beach > .5) surface = 4;
                if (c.river > .5) surface = 6;
                if (c.stones > .5) surface = 5;
                if (c.grass > .5) surface = 2;
                if (max(c.gravel, c.road) > .5) surface = 5;
                if (c.snow > .5) surface = 7;
                return float4(surface, input.local.y, 0, 1);
            }
            ENDHLSL
        }
    }
}
