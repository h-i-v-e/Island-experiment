Shader "Motu/Sea Water"
{
    Properties
    {
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("Ocean Ripple Noise", 2D) = "black" {}
        _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        _OpacityDepth ("Full Opacity Depth", Float) = 5
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.65
        _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.8
        _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        [HideInInspector] _WaterSkyExposure ("Water Sky Exposure", Range(0, 1)) = 1
        _RefractionStrength ("Underwater Distortion", Range(0, 0.03)) = 0.02
        _RefractionDepth ("Full Distortion Depth (metres)", Float) = 0.6
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.008
        [HideInInspector] _GeometricWaves ("Geometric Waves", Float) = 1
        [NoScaleOffset] _WaveAttenuationTex ("Wave Attenuation", 2D) = "white" {}
        [HideInInspector] _WaveAttenuationWorldRect ("Wave Attenuation World Rect", Vector) = (-1, -1, 0.5, 0.5)
        [HideInInspector] _WaveFadeStart ("Wave Fade Start", Float) = 320
        [HideInInspector] _WaveFadeEnd ("Wave Fade End", Float) = 480
        [HideInInspector] _OceanWave0 ("Ocean Wave 0", Vector) = (1, 0, 30, 0.34)
        [HideInInspector] _OceanWave1 ("Ocean Wave 1", Vector) = (0, 1, 15, 0.18)
        [HideInInspector] _OceanWave2 ("Ocean Wave 2", Vector) = (-0.8, 0.5, 7.5, 0.09)
        [HideInInspector] _OceanWave3 ("Ocean Wave 3", Vector) = (0.6, -0.8, 4, 0.04)
        [HideInInspector] _OceanWaveSpeeds ("Ocean Wave Speeds", Vector) = (3.6, 2.8, 2.1, 1.5)
        [HideInInspector] _OceanWaveChoppiness ("Ocean Wave Choppiness", Vector) = (0, 0, 0, 0)
        [HideInInspector] _WaveNoiseWorldSize ("Wave Noise World Size", Float) = 2048
        [HideInInspector] _WaveDomainWarp ("Wave Domain Warp", Float) = 9
        [HideInInspector] _WaveAmplitudeVariation ("Wave Amplitude Variation", Range(0, 0.75)) = 0.6
    }

    SubShader
    {
        Tags { "Queue"="Transparent" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        // The composed deep-ocean colour is opaque. Writing depth prevents
        // distant wave triangles and their back faces from being drawn over
        // nearer crests as a saw-tooth pattern at grazing view angles.
        ZWrite On
        Cull Back

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
            #include "OceanWaves.cginc"

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
            };

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                float3 baseWorldPosition = mul(
                    unity_ObjectToWorld,
                    input.vertex).xyz;
                float3 waveDisplacement;
                MotuEvaluateOceanWaveDisplacement(
                    baseWorldPosition.xz,
                    length(input.vertex.xz),
                    waveDisplacement);
                float3 displacedWorldPosition = baseWorldPosition + waveDisplacement;
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

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 analyticWaveNormal;
                MotuEvaluateOceanWaveNormal(
                    input.waveSamplePosition,
                    analyticWaveNormal);
                float3 worldNormal = MotuFacingWaterNormal(
                    analyticWaveNormal,
                    viewDirection);
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
                fixed3 waterBody = _Color.rgb
                    * brightness
                    * waterIllumination;
                float2 reflectionNoiseUv = MotuCloudWorldToLocal(
                    input.worldPosition).xz / 8.0;
                half2 reflectionRipple = half2(
                    tex2D(
                        _NoiseTex,
                        reflectionNoiseUv + float2(_Time.y * 0.025, 0.0)).r,
                    tex2D(
                        _NoiseTex,
                        reflectionNoiseUv.yx + float2(0.0, _Time.y * 0.02)).g)
                    - 0.5h;
                fixed3 water = MotuShadeWater(
                    waterBody,
                    worldNormal,
                    viewDirection,
                    input.worldPosition,
                    reflectionRipple,
                    1.0h,
                    shadowAttenuation,
                    cloud);
                half waterOpacity = MotuWaterOpacity(
                    waterDepth,
                    _OpacityDepth);
                fixed4 foggedSurface = fixed4(water, 1.0h);
                UNITY_APPLY_FOG(input.fogCoord, foggedSurface);
                fixed3 refractedScene = MotuRefractScene(
                    input.grabPosition,
                    waterDepth,
                    worldNormal,
                    viewDirection,
                    reflectionRipple);
                return fixed4(
                    lerp(refractedScene, foggedSurface.rgb, waterOpacity),
                    1.0h);
            }
            ENDCG
        }
    }
}
