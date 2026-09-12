Shader "Motu/Sea Water"
{
    Properties
    {
        _RippleStrength ("Fine Ripple Strength", Range(0, 1)) = 0.12
        _RippleWorldSize ("Fine Ripple Wavelength (m)", Range(0.1, 3)) = 1.2
        _RippleSpeed ("Fine Ripple Animation Speed", Range(0, 3)) = 1
        _SurfaceRoughness ("Surface Roughness", Range(0.04, 0.6)) = 0.16
        _WindRoughness ("Wind Roughness", Range(0, 0.4)) = 0.12
        _AbsorptionCoefficients ("RGB Absorption (per metre)", Vector) = (0.45, 0.12, 0.055, 0)
        _AbsorptionStrength ("Absorption Strength", Range(0, 4)) = 1
        _PersistentFoamStrength ("Persistent Foam Strength", Range(0, 2)) = 0.8
        _FoamLifetime ("Foam Lifetime (seconds)", Range(0.1, 30)) = 6
        _FoamDepositRate ("Foam Deposit Rate", Range(0, 10)) = 2
        [HideInInspector] _OceanFoamHistory ("Foam History", 2D) = "black" {}
        [HideInInspector] _OceanFoamHistoryRect ("Foam History World Rect", Vector) = (0, 0, 0, 0)
        _CoastalOpacity ("Shallow Coastal Tint", Range(0, 1)) = 0.16
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("Ocean Ripple Noise", 2D) = "black" {}
        [HideInInspector] _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        [HideInInspector] _OpacityDepth ("Full Opacity Depth", Float) = 5
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.65
        [HideInInspector] _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.8
        [HideInInspector] _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        _BoatDisplacementFoamStrength ("Boat Displacement Foam (per metre)", Range(0, 4)) = 1
        _WaveTranslucencyColour ("Wave Translucency Colour", Color) = (0.08, 0.55, 0.42, 1)
        _WaveTranslucencyStrength ("Wave Translucency Strength", Range(0, 8)) = 6
        _WaveTranslucencyAmbient ("Translucency Ambient Contribution", Range(0, 0.25)) = 0.03
        _WaveTranslucencyFalloff ("Translucency Sun Directionality", Range(1, 16)) = 4
        [HideInInspector] _WaterSkyExposure ("Water Sky Exposure", Range(0, 1)) = 1
        _RefractionStrength ("Underwater Distortion", Range(0, 0.03)) = 0.02
        _RefractionDepth ("Full Distortion Depth (metres)", Float) = 0.6
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.008
        [HideInInspector] _GeometricWaves ("Geometric Waves", Float) = 1
        [NoScaleOffset] _WaveAttenuationTex ("Wave Attenuation", 2D) = "white" {}
        [NoScaleOffset] _WaveOnshoreTex ("Onshore Wave Direction", 2D) = "black" {}
        [HideInInspector] _WaveAttenuationWorldRect ("Wave Attenuation World Rect", Vector) = (-1, -1, 0.5, 0.5)
        [HideInInspector] _WaveFadeStart ("Wave Fade Start", Float) = 320
        [HideInInspector] _WaveFadeEnd ("Wave Fade End", Float) = 480
        [HideInInspector] _OceanWave0 ("Ocean Wave 0", Vector) = (1, 0, 30, 0.34)
        [HideInInspector] _OceanWave1 ("Ocean Wave 1", Vector) = (0, 1, 15, 0.18)
        [HideInInspector] _OceanWave2 ("Ocean Wave 2", Vector) = (-0.8, 0.5, 7.5, 0.09)
        [HideInInspector] _OceanWave3 ("Ocean Wave 3", Vector) = (0.6, -0.8, 4, 0.04)
        [HideInInspector] _OceanWaveFrom0 ("Wave From 0", Vector) = (1, 0, 30, 0)
        [HideInInspector] _OceanWaveFrom1 ("Wave From 1", Vector) = (0, 1, 15, 0)
        [HideInInspector] _OceanWaveFrom2 ("Wave From 2", Vector) = (-0.848, 0.530, 7.5, 0)
        [HideInInspector] _OceanWaveFrom3 ("Wave From 3", Vector) = (0.6, -0.8, 4, 0)
        [HideInInspector] _OceanWaveTo0 ("Wave To 0", Vector) = (1, 0, 30, 0)
        [HideInInspector] _OceanWaveTo1 ("Wave To 1", Vector) = (0, 1, 15, 0)
        [HideInInspector] _OceanWaveTo2 ("Wave To 2", Vector) = (-0.848, 0.530, 7.5, 0)
        [HideInInspector] _OceanWaveTo3 ("Wave To 3", Vector) = (0.6, -0.8, 4, 0)
        [HideInInspector] _OceanWaveTransition ("Wave Direction Transition", Float) = 0
        [HideInInspector] _OnshoreWavePhase ("Onshore Wave Phase", Float) = 0
        [HideInInspector] _OceanFoamTravel ("Foam Travel XY and Distortion Phase Z", Vector) = (0, 0, 0, 0)
        [HideInInspector] _OceanWaveSpeeds ("Ocean Wave Speeds", Vector) = (3.6, 2.8, 2.1, 1.5)
        [HideInInspector] _OceanWaveChoppiness ("Ocean Wave Choppiness", Vector) = (0, 0, 0, 0)
        [HideInInspector] _WaveNoiseWorldSize ("Wave Noise World Size", Float) = 2048
        [HideInInspector] _WaveDomainWarp ("Wave Domain Warp", Float) = 9
        [HideInInspector] _WaveAmplitudeVariation ("Wave Amplitude Variation", Range(0, 0.75)) = 0.6
        [HideInInspector] _WhitecapColour ("Whitecap Colour", Color) = (0.9, 0.96, 1, 1)
        [HideInInspector] _WhitecapStrength ("Whitecap Strength", Range(0, 2)) = 0.85
        [HideInInspector] _WhitecapHeightThreshold ("Whitecap Height Threshold", Range(0.5, 0.98)) = 0.68
        [HideInInspector] _WhitecapSlopeThreshold ("Whitecap Slope Threshold", Range(0, 1)) = 0.12
        [HideInInspector] _WhitecapCoverage ("Whitecap Coverage", Range(0, 1)) = 0.58
        [HideInInspector] _WhitecapNoiseWorldSize ("Whitecap Noise World Size", Float) = 7
        _WhitecapDistortionStrength ("Foam Distortion Strength", Range(0, 1.5)) = 0.65
        [HideInInspector] _WhitecapDistortionScale ("Whitecap Distortion Scale", Range(0.1, 1)) = 0.32
        [HideInInspector] _WhitecapDistortionSpeed ("Whitecap Distortion Speed", Range(0, 2)) = 0.65
        [HideInInspector] _OnshoreWaveEnabled ("Onshore Wave Enabled", Float) = 1
        [HideInInspector] _OnshoreWaveParameters ("Onshore Wave Parameters", Vector) = (12, 0.16, 2.2, 0.18)
        [HideInInspector] _OnshoreWaveBreaking ("Onshore Wave Breaking", Vector) = (0.95, 96, 5, 3.5)
    }

    SubShader
    {
        Tags { "Queue"="Transparent" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        // The composed deep-ocean colour is opaque. Writing depth prevents
        // distant wave triangles and their back faces from being drawn over
        // nearer crests as a saw-tooth pattern at grazing view angles.
        ZWrite On
        Cull Off

        GrabPass { "_MotuWaterBackground" }

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma target 3.5
            #pragma multi_compile_fog
            #pragma multi_compile_fwdbase

            #include "WaterCommon.cginc"
            #include "OceanSurfaceGeometry.cginc"
            #include "OceanOptics.cginc"

            half4 _WaveTranslucencyColour;
            half _WaveTranslucencyStrength;
            half _WaveTranslucencyFalloff;
            half _WaveTranslucencyAmbient;

            half3 WaveTranslucency(float crestResponse, float3 worldNormal,
                float3 viewDirection, float3 worldPosition, half shadowAttenuation,
                MotuCloudLighting cloud)
            {
                // Height above the mean sea plane, relative to the largest wave peak.
                half crest = saturate(crestResponse);
                float3 lightVector = UnityWorldSpaceLightDir(worldPosition);
                float3 lightDirection = lightVector
                    * rsqrt(max(dot(lightVector, lightVector), 0.000001));
                half throughWave = pow(saturate(dot(viewDirection, -lightDirection)),
                    max(_WaveTranslucencyFalloff, 1.0h));
                half faceWeight = 1.0h - saturate(dot(worldNormal, viewDirection));
                faceWeight *= faceWeight;
                half daylight = smoothstep(0.0h, 0.1h, lightDirection.y);
                // Direct scattering favours the sun; a small sky-light contribution
                // keeps curved crests visible at other sun orientations.
                half3 illumination = _LightColor0.rgb * shadowAttenuation
                    * cloud.directTransmittance * daylight * throughWave * faceWeight;
                illumination += UNITY_LIGHTMODEL_AMBIENT.rgb
                    * cloud.ambientTransmittance * max(_WaveTranslucencyAmbient, 0.0h)
                    * faceWeight;
                return _WaveTranslucencyColour.rgb * illumination
                    * crest * max(_WaveTranslucencyStrength, 0.0h);
            }

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float4 screenPosition : TEXCOORD0;
                float surfaceEyeDepth : TEXCOORD1;
                float3 worldPosition : TEXCOORD2;
                UNITY_FOG_COORDS(3)
                float4 grabPosition : TEXCOORD4;
                SHADOW_COORDS(5)
                float2 waveSamplePosition : TEXCOORD6;
                float4 deckWaveData : TEXCOORD7; // original height, clamp, final height, boat displacement
            };

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                float3 baseWorldPosition = mul(
                    unity_ObjectToWorld,
                    input.vertex).xyz;
                float3 displacedWorldPosition = MotuOceanSurfacePosition(baseWorldPosition,
                    length(input.vertex.xz), output.deckWaveData);
                output.pos = UnityWorldToClipPos(displacedWorldPosition);
                output.screenPosition = ComputeScreenPos(output.pos);
                output.grabPosition = ComputeGrabScreenPos(output.pos);
                output.surfaceEyeDepth = -mul(
                    UNITY_MATRIX_V,
                    float4(displacedWorldPosition, 1.0)).z;
                output.worldPosition = displacedWorldPosition;
                output.waveSamplePosition = baseWorldPosition.xz;
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            float4 Fragment(VertexOutput input, float facing : VFACE) : SV_Target
            {
                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 analyticWaveNormal;
                float whitecap;
                float analyticCrestResponse;
                float foamAllowance;
                MotuEvaluateOceanWaveNormal(
                    input.waveSamplePosition,
                    analyticWaveNormal,
                    whitecap,
                    analyticCrestResponse,
                    foamAllowance);
                float2 oceanOrigin = float2(unity_ObjectToWorld._m03, unity_ObjectToWorld._m23);
                float crestResponse = MotuOceanSurfaceTranslucency(input.deckWaveData.z,
                    analyticCrestResponse, length(input.waveSamplePosition - oceanOrigin));
                // Use the final rasterized height too: analytic waves can still
                // be large after the visible mesh or hull clamp flattens them.
                float visibleFoamAllowance = MotuOceanFoamHeightAllowance(input.deckWaveData.z);
                whitecap = max(whitecap, MotuOceanHistoryFoam(input.worldPosition.xz) * foamAllowance)
                    * visibleFoamAllowance;
                MotuShipWaveShading(input.worldPosition.xz, input.deckWaveData.x,
                    input.deckWaveData.y, MotuOceanClampHeightEnvelope(input.waveSamplePosition), analyticWaveNormal, whitecap);
                // Add after hull suppression: displaced water at the hull should
                // foam even where the boat has flattened an incoming crest.
                float boatFoam = MotuShipDisplacementFoam(input.deckWaveData.w);
                if (boatFoam > 0.0001)
                    whitecap = max(whitecap, boatFoam * MotuOceanFoamPatches(input.waveSamplePosition)
                        * max(_WhitecapStrength, 0.0));
                float roughness;
                float3 detailNormal = MotuOceanRippleNormal(input.worldPosition, analyticWaveNormal, roughness);
                float3 worldNormal = MotuFacingWaterNormal(detailNormal, viewDirection);
                half brightness = 0.72h
                    + 0.28h * saturate(dot(
                        analyticWaveNormal,
                        normalize(float3(0.3, 1.0, 0.2))));
                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                float waterDepth = MotuWaterDepth(
                    input.screenPosition,
                    input.surfaceEyeDepth);

                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                fixed3 waterIllumination = MotuWaterIllumination(
                    worldNormal,
                    input.worldPosition,
                    shadowAttenuation,
                    0.12h,
                    cloud);
                if (facing < 0)
                {
                    // Water-to-air refraction ends at the interface. Distance
                    // absorption is applied by the underwater camera afterwards.
                    float cosine = saturate(dot(worldNormal, viewDirection));
                    float sinTransmittedSquared = 1.333 * 1.333 * (1 - cosine * cosine);
                    float reflection = lerp(MotuWaterFresnel(cosine), 1,
                        smoothstep(.97, 1, sinTransmittedSquared));
                    float2 uv = input.grabPosition.xy / input.grabPosition.w;
                    float2 offset = mul((float3x3)UNITY_MATRIX_V, worldNormal).xy * _RefractionStrength * .15;
                    float3 throughSurface = tex2D(_MotuWaterBackground, saturate(uv + offset)).rgb;
                    float3 underside = _Color.rgb * waterIllumination;
                    return float4(lerp(throughSurface, underside, reflection), 1);
                }
                fixed3 waterBody = _Color.rgb
                    * brightness
                    * waterIllumination;
                waterBody += WaveTranslucency(crestResponse, worldNormal,
                    viewDirection, input.worldPosition, shadowAttenuation, cloud);
                // Reflections and refraction follow the animated surface detail,
                // rather than a separate high-frequency distortion texture.
                float2 reflectionRipple = mul((float3x3)UNITY_MATRIX_V,
                    detailNormal - analyticWaveNormal).xy;
                float pathLength;
                float3 refractedPosition;
                float3 refractedScene = MotuWaterRefractDepthSafe(input.grabPosition, input.screenPosition,
                    input.surfaceEyeDepth, waterDepth, worldNormal, viewDirection, reflectionRipple, pathLength,
                    refractedPosition);
                float seaLevel = input.worldPosition.y - input.deckWaveData.z;
                refractedScene = lerp(waterBody, refractedScene,
                    MotuOceanSeabedVisibility(refractedPosition.y, seaLevel));
                float3 water = MotuWaterShadeOptics(waterBody, refractedScene, pathLength, worldNormal,
                    viewDirection, input.worldPosition, reflectionRipple, roughness, shadowAttenuation, cloud, 1);
                water = lerp(water, _Color.rgb * waterIllumination,
                    MotuOceanCoastalTint(input.worldPosition.xz));
                float3 litWhitecap = _WhitecapColour.rgb * waterIllumination;
                water = lerp(water, litWhitecap, saturate(whitecap));
                float4 result = float4(water, 1);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }
    }
}
