Shader "Hidden/Motu/Ocean Shore Query"
{
    Properties { [HideInInspector] _QueryPositions ("Positions", 2D) = "black" {} }
    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Pass
        {
            ZTest Always ZWrite Off Cull Off
            HLSLPROGRAM
            #pragma vertex vert_img
            #pragma fragment frag
            #pragma target 3.5
            #include "../MotuUrp.hlsl"
            #include "Packages/com.motu.runtime/Runtime/Shaders/OceanWaves.cginc"
            sampler2D _QueryPositions;
            float4 _ShoreQueryCentre;
            float _ShoreQueryRadius, _ShoreQueryCount, _ShoreQueryPlacement;

            float Depth(float2 position)
            {
                return saturate(MotuOceanCoastalData(position).b) * MotuSeaMaskDepthMetres;
            }

            float RiverAllowance(float2 position)
            {
                // Match the local river exclusion used by MotuOceanFoamAllowance.
                // The filtered direction field alone can bleed across a river bank.
                return smoothstep(.02, .10, MotuOceanCoastalData(position).a);
            }

            float4 PlaceProbe(float index)
            {
                if (_OnshoreWaveEnabled < .5 || _OnshoreWaveParameters.y <= 0) return 0;
                // Cross the listener's circle with evenly spaced horizontal and
                // vertical scans. Radial spokes collapse onto one point when
                // the listener stands on the breaking-depth contour itself.
                float firstCount = ceil(_ShoreQueryCount * .5);
                bool vertical = index >= firstCount;
                float row = vertical ? index - firstCount : index;
                float rowCount = vertical ? max(1.0, _ShoreQueryCount - firstCount) : firstCount;
                float offset = ((row + .5) / rowCount * 2.0 - 1.0) * _ShoreQueryRadius;
                float halfLength = sqrt(max(0.0, _ShoreQueryRadius * _ShoreQueryRadius - offset * offset));
                float2 direction = vertical ? float2(0, 1) : float2(1, 0);
                float2 tangent = vertical ? float2(1, 0) : float2(0, 1);
                if (fmod(index, 2.0) > .5) direction = -direction;
                float2 origin = _ShoreQueryCentre.xz + tangent * offset - direction * halfLength;
                float startDepth = clamp(_OnshoreWaveBreaking.z, .01, 10.0);
                float fullDepth = clamp(_OnshoreWaveBreaking.w, 0.0, startDepth - .01);
                float targetDepth = lerp(fullDepth, startDepth, .35);
                float previousDepth = Depth(origin);
                float4 fallback = 0;
                float fallbackError = 1.0e20;
                // Trace the depth map rather than terrain physics, so unloaded
                // colliders, the ship and cave geometry cannot misplace the surf.
                [loop] for (int step = 1; step <= 48; step++)
                {
                    float distance = step * (2.0 * halfLength) / 48.0;
                    float2 position = origin + direction * distance;
                    float depth = Depth(position);
                    float2 fallbackDirection; float fallbackInfluence, fallbackCoordinate;
                    MotuOceanOnshoreField(position, fallbackDirection, fallbackInfluence, fallbackCoordinate);
                    fallbackInfluence *= RiverAllowance(position);
                    // On very gentle beaches the preferred depth may lie beyond
                    // the audible coastal band. Use shallower breaking water there.
                    float depthError = abs(depth - targetDepth);
                    if (depth > .15 && depth < startDepth - .01 && fallbackInfluence > .1
                        && fallbackCoordinate * MotuSeaMaskLandDistanceMetres > .01
                        && fallbackCoordinate * MotuSeaMaskLandDistanceMetres < _OnshoreWaveBreaking.y * .8
                        && depthError < fallbackError)
                    {
                        fallback = float4(position, depth, fallbackInfluence * .5);
                        fallbackError = depthError;
                    }
                    if ((depth - targetDepth) * (previousDepth - targetDepth) <= 0.0)
                    {
                        float low = distance - (2.0 * halfLength) / 48.0, high = distance;
                        float lowDepth = previousDepth;
                        [unroll] for (int iteration = 0; iteration < 7; iteration++)
                        {
                            float middle = (low + high) * .5;
                            float middleDepth = Depth(origin + direction * middle);
                            if ((middleDepth - targetDepth) * (lowDepth - targetDepth) > 0.0)
                            { low = middle; lowDepth = middleDepth; }
                            else high = middle;
                        }
                        position = origin + direction * ((low + high) * .5);
                        float2 onshore; float influence, coordinate;
                        MotuOceanOnshoreField(position, onshore, influence, coordinate);
                        influence *= RiverAllowance(position);
                        float coastDistance = coordinate * MotuSeaMaskLandDistanceMetres;
                        // Only the same coastal/depth band that can produce a breaker.
                        if (influence > .1 && coastDistance > .01
                            && coastDistance < _OnshoreWaveBreaking.y && Depth(position) > .05)
                            return float4(position, Depth(position), influence);
                    }
                    previousDepth = depth;
                }
                return fallback;
            }

            float4 frag(v2f_img input) : SV_Target
            {
                if (_ShoreQueryPlacement > .5)
                    return input.uv.y < .5 ? PlaceProbe(floor(input.uv.x * _ShoreQueryCount)) : 0;
                float2 target = tex2Dlod(_QueryPositions, float4(input.uv.x, .5, 0, 0)).rb;
                float riverAllowance = RiverAllowance(target);
                if (riverAllowance <= .0001) return 0;
                float2 position = target;
                float3 displacement; float2 derivative; float influence, foam;
                [unroll] for (int iteration = 0; iteration < 8; iteration++)
                {
                    MotuEvaluateOnshoreWaveField(position, displacement, derivative, influence, foam);
                    displacement *= MotuOceanDepthWaveScale(MotuOceanCoastalData(position));
                    position += (target - position - displacement.xz) * .75;
                }
                MotuEvaluateOnshoreWaveField(position, displacement, derivative, influence, foam);
                float depthScale = MotuOceanDepthWaveScale(MotuOceanCoastalData(position));
                displacement *= depthScale;
                if (length(position + displacement.xz - target) > .1) return 0;
                // Check both the world contact and the undisplaced wave position:
                // horizontal choppiness must not pull a river probe onto sea waves.
                riverAllowance = min(riverAllowance, RiverAllowance(position));
                if (riverAllowance <= .0001) return 0;
                float geometryWeight = saturate(_GeometricWaves);
                if (input.uv.y >= .5)
                {
                    // Local shore amplitude in metres, independent of the phase
                    // at which the leading breaker face crosses the audio trigger.
                    float amplitude = max(_OnshoreWaveParameters.y, 0.0) * influence
                        * max(_MotuWeatherWind.w, 0.0) * depthScale * geometryWeight * riverAllowance;
                    float waveScale = MotuOceanShelterWaveScale(position);
                    return float4(amplitude * waveScale, waveScale, 0, 0);
                }
                return float4(_ShoreQueryCentre.y + displacement.y * geometryWeight,
                    foam * riverAllowance * geometryWeight, Depth(position), influence * riverAllowance);
            }
            ENDHLSL
        }
    }
}
