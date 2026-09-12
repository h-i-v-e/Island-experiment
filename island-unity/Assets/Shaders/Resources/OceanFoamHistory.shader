Shader "Hidden/Motu/Ocean Foam History"
{
    Properties { _FoamPrevious ("Previous foam and displacement", 2D) = "black" {} }
    SubShader
    {
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            CGPROGRAM
            #pragma vertex vert_img
            #pragma fragment Frag
            #pragma target 3.5
            #include "UnityCG.cginc"
            #include "Assets/Shaders/OceanWaves.cginc"
            #include "Assets/Shaders/OceanShipWaves.cginc"
            #include "Assets/Shaders/OceanDeckWaveClamp.cginc"
            sampler2D _FoamPrevious;
            float4 _FoamPreviousRect, _FoamCurrentRect;
            float _FoamDeltaTime, _FoamLifetime, _FoamDepositRate, _FoamHistoryValid;
            float2 _FoamWindDrift;

            float4 History(float2 worldXZ)
            {
                float2 uv = (worldXZ - _FoamPreviousRect.xy) * _FoamPreviousRect.zw;
                if (any(uv < 0) || any(uv > 1) || _FoamHistoryValid < .5) return 0;
                return tex2D(_FoamPrevious, uv);
            }

            float4 Frag(v2f_img input) : SV_Target
            {
                float2 world = _FoamCurrentRect.xy + input.uv / _FoamCurrentRect.zw;
                // Recover the base position so the stored texture follows the
                // displaced world XZ used by the ocean fragment shader.
                float2 basePosition = world;
                float3 displacement;
                [unroll]
                for (int i = 0; i < 3; i++)
                {
                    MotuEvaluateOceanWaveDisplacement(basePosition, 0, displacement);
                    basePosition += (world - basePosition - displacement.xz) * .75;
                }
                MotuEvaluateOceanWaveDisplacement(basePosition, 0, displacement);
                float3 normal;
                float source;
                float crestResponse;
                float foamAllowance;
                MotuEvaluateOceanWaveNormal(basePosition, normal, source, crestResponse, foamAllowance);
                float3 shipField = MotuShipWaveField(world);
                float clampWeight = MotuDeckWaveClampWeight(world);
                float finalHeight = MotuShipWaveHeight(displacement.y, shipField, clampWeight,
                    MotuOceanClampHeightEnvelope(basePosition));
                float boat = MotuShipDisplacementFoam(abs(finalHeight-displacement.y));
                source = max(source, max(shipField.z, boat * MotuOceanFoamPatches(basePosition) * max(_WhitecapStrength, 0)));
                float4 previous = History(world);
                // Advect using the displacement delta at a fixed world location.
                // Bound discontinuities from weather edits and sparse updates.
                float2 velocity = previous.a > .5 ? (displacement.xz - previous.gb) / max(_FoamDeltaTime, .001) : 0;
                velocity *= min(1, 5 / max(length(velocity), .001));
                float oldFoam = History(world - (velocity + _FoamWindDrift) * _FoamDeltaTime).r;
                float decay = exp(-_FoamDeltaTime / max(_FoamLifetime, .1));
                float foam = oldFoam * decay + saturate(source) * (1-exp(-max(_FoamDepositRate, 0) * _FoamDeltaTime));
                // Clear stored patches in calm/river-suppressed areas as well
                // as dry terrain. Active boat disturbance can still make foam.
                float wet = smoothstep(0, .02, MotuOceanCoastalData(basePosition).b);
                float active = step(.0001, max(foamAllowance, max(shipField.z, boat)));
                return float4(saturate(foam) * wet * active, displacement.xz, 1);
            }
            ENDCG
        }
    }
}
