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
            #pragma multi_compile_local __ MOTU_QUERY_VELOCITY
            #if defined(MOTU_QUERY_VELOCITY)
            static float queryTimeOffset = 0;
            float _QueryWaveTimeScale;
            float4 QueryPattern(float4 pattern, float speed)
            {
                pattern.w += queryTimeOffset * speed * _QueryWaveTimeScale
                    * 6.28318530718 / max(pattern.z, .25);
                return pattern;
            }
            #define MOTU_OCEAN_SAMPLE_PATTERN(pattern, speed) QueryPattern(pattern, speed)
            #define MOTU_OCEAN_SAMPLE_ONSHORE_PHASE (_OnshoreWavePhase + queryTimeOffset * _OnshoreWaveParameters.z * _QueryWaveTimeScale * 6.28318530718 / max(_OnshoreWaveParameters.x, 1.0))
            #define MOTU_OCEAN_SAMPLE_WIND_POSITION(position) (MotuWindAdvectedPosition(position) - queryTimeOffset * MotuWindDirection() * max(_MotuWeatherWind.z, 0.0))
            #endif
            #include "Assets/Shaders/OceanWaves.cginc"
            sampler2D _QueryPositions;
            float4 _QueryOceanOrigin;

            float4 frag(v2f_img input) : SV_Target
            {
                float2 target = tex2Dlod(_QueryPositions, float4(input.uv.x, .5, 0, 0)).rb;
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
                #if defined(MOTU_QUERY_VELOCITY)
                if (input.uv.y > .5)
                {
                    // Follow one undeformed surface point, rather than subtracting
                    // heights from moving ship probes (which confuses slope with
                    // water velocity). Freeze weather/transition weights for this
                    // derivative; advance both phase banks and advected noise.
                    const float stepSeconds = .02;
                    float3 before, after;
                    queryTimeOffset = -stepSeconds;
                    MotuEvaluateOceanWaveDisplacement(position,
                        length(position - _QueryOceanOrigin.xz), before);
                    queryTimeOffset = stepSeconds;
                    MotuEvaluateOceanWaveDisplacement(position,
                        length(position - _QueryOceanOrigin.xz), after);
                    return float4((after - before) / (2 * stepSeconds), error);
                }
                #endif
                return float4(_QueryOceanOrigin.y + displacement.y, error, 0, 1);
            }
            ENDCG
        }
    }
}
