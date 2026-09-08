Shader "Hidden/Motu/Ocean Surface Query"
{
    Properties
    {
        [HideInInspector] _QueryPositions ("Probe world positions", 2D) = "black" {}
    }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment frag
            #pragma target 3.0
            #include "UnityCG.cginc"
            #include "Assets/Shaders/OceanWaves.cginc"
            sampler2D _QueryPositions;
            float4 _QueryOceanOrigin;

            float4 frag(v2f_img input) : SV_Target
            {
                float2 target = tex2Dlod(_QueryPositions, float4(input.uv, 0, 0)).rb;
                float2 position = target;
                float3 displacement;
                // Invert horizontal choppiness: physics asks for height at a
                // displaced world XZ, whereas the vertex shader starts on a grid.
                [unroll]
                for (int iteration = 0; iteration < 8; iteration++)
                {
                    MotuEvaluateOceanWaveDisplacement(position,
                        length(position - _QueryOceanOrigin.xz), displacement);
                    position += (target - position - displacement.xz) * 0.75;
                }
                MotuEvaluateOceanWaveDisplacement(position,
                    length(position - _QueryOceanOrigin.xz), displacement);
                float error = length(position + displacement.xz - target);
                return float4(_QueryOceanOrigin.y + displacement.y, error, 0, 1);
            }
            ENDCG
        }
    }
}
