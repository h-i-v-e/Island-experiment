Shader "Hidden/Motu/Ocean Island Shelter"
{
    Properties
    {
        _SeaMask ("Island Sea Mask", 2D) = "black" {}
        _MainTex ("New Shelter", 2D) = "black" {}
        _ShelterHistoryTex ("Previous Shelter", 2D) = "black" {}
    }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Cull Off ZWrite Off ZTest Always

        // Draw every island into a cleared black target. Max composition lets
        // one island shelter another island's exposed coast without double
        // counting land or darkening already protected water.
        Pass
        {
            BlendOp Max
            Blend One One
            HLSLPROGRAM
            #pragma vertex vert_img
            #pragma fragment Shelter
            #pragma target 3.5
            #include "../MotuUrp.hlsl"
            sampler2D _SeaMask;
            float4x4 _IslandWorldToLocal;
            float _IslandWorldSize;
            float4 _CompositionWorldRect;
            float2 _ShelterWindDirection;
            // Reach metres, recovery-start fraction, side softness metres, strength.
            float4 _ShelterGenerationParameters;

            float LandAt(float2 worldPosition)
            {
                float3 local = mul(_IslandWorldToLocal,
                    float4(worldPosition.x, 0, worldPosition.y, 1)).xyz;
                float2 uv = local.xz / max(_IslandWorldSize, 1.0) + 0.5;
                if (any(uv < 0) || any(uv > 1)) return 0;
                float2 mask = tex2Dlod(_SeaMask, float4(uv, 0, 0)).rg;
                // R=one above sea level, G=zero on land. Shallow offshore
                // water alone must not cast a wind shadow.
                return smoothstep(0.98, 0.998, mask.r)
                    * (1.0 - smoothstep(0.005, 0.02, mask.g));
            }

            float4 Shelter(v2f_img input) : SV_Target
            {
                float2 world = _CompositionWorldRect.xy
                    + input.uv * _CompositionWorldRect.zw;
                float2 direction = normalize(_ShelterWindDirection + float2(0.000001, 0));
                float2 across = float2(-direction.y, direction.x);
                float reach = max(_ShelterGenerationParameters.x, 1.0);
                float recovery = clamp(_ShelterGenerationParameters.y, 0.0, 0.99);
                float softness = max(_ShelterGenerationParameters.z, 0.0);
                // Reject pixels whose entire upwind segment misses the island
                // (expanded by the penumbra) before doing any texture search.
                float2 originUv = mul(_IslandWorldToLocal,
                    float4(world.x, 0, world.y, 1)).xz / max(_IslandWorldSize, 1.0) + 0.5;
                float2 rayUv = mul((float3x3)_IslandWorldToLocal,
                    float3(-direction.x, 0, -direction.y) * reach).xz
                    / max(_IslandWorldSize, 1.0);
                float2 sideUv = abs(mul((float3x3)_IslandWorldToLocal,
                    float3(across.x, 0, across.y) * softness).xz)
                    / max(_IslandWorldSize, 1.0);
                float2 safeRay = float2(rayUv.x >= 0 ? 1 : -1, rayUv.y >= 0 ? 1 : -1)
                    * max(abs(rayUv), 0.000001);
                float2 a = (-sideUv - originUv) / safeRay;
                float2 b = (1.0 + sideUv - originUv) / safeRay;
                float entry = max(max(min(a.x, b.x), min(a.y, b.y)), 0.0);
                float exit = min(min(max(a.x, b.x), max(a.y, b.y)), 1.0);
                if (exit < entry) return 0;
                float3 shelter = 0;
                [loop]
                for (int sampleIndex = 0; sampleIndex < 48; sampleIndex++)
                {
                    // Quadratic spacing catches small coves near the shore
                    // while still allowing a long, inexpensive fetch search.
                    float t = (sampleIndex + 0.5) / 48.0;
                    float fraction = t * t;
                    float2 upwind = world - direction * (fraction * reach);
                    float spread = softness * smoothstep(0, 0.2, fraction);
                    float weight = 1.0 - smoothstep(recovery, 1.0, fraction);
                    float3 land = float3(LandAt(upwind),
                        LandAt(upwind + across * spread),
                        LandAt(upwind - across * spread));
                    shelter = max(shelter, land * weight);
                    if (all(shelter > 0.999)) break;
                }
                float value = dot(shelter, float3(0.65, 0.175, 0.175));
                return saturate(value * _ShelterGenerationParameters.w);
            }
            ENDHLSL
        }

        // Reproject the old world-space field before blending so grid
        // recentering never drags a calm patch along with the player.
        Pass
        {
            HLSLPROGRAM
            #pragma vertex vert_img
            #pragma fragment BlendShelter
            #pragma target 3.0
            #include "../MotuUrp.hlsl"
            sampler2D _MainTex;
            sampler2D _ShelterHistoryTex;
            float4 _CompositionWorldRect;
            float4 _ShelterHistoryWorldRect;
            float _ShelterBlend;
            float _ShelterHistoryValid;
            float4 BlendShelter(v2f_img input) : SV_Target
            {
                float current = tex2D(_MainTex, input.uv).r;
                float2 world = _CompositionWorldRect.xy
                    + input.uv * _CompositionWorldRect.zw;
                float2 oldUv = (world - _ShelterHistoryWorldRect.xy)
                    * _ShelterHistoryWorldRect.zw;
                if (_ShelterHistoryValid < 0.5 || any(oldUv < 0) || any(oldUv > 1))
                    return current;
                float previous = tex2D(_ShelterHistoryTex, oldUv).r;
                return lerp(previous, current, saturate(_ShelterBlend));
            }
            ENDHLSL
        }
    }
}
