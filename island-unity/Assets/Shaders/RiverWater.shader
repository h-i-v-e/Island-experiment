Shader "Motu/River Water"
{
    Properties
    {
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("River Noise", 2D) = "black" {}
        _CoarseNoiseWorldSize ("Coarse Noise World Size", Float) = 6
        _FineNoiseWorldSize ("Fine Noise World Size", Float) = 3
        _CoarseFlowSpeed ("Coarse Flow Speed", Float) = 2.25
        _FineFlowSpeed ("Fine Flow Speed", Float) = 9
        _WorldSize ("World Size", Float) = 2000
        _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        _OpacityDepth ("Full Opacity Depth", Float) = 5
        _EstuaryStrength ("Estuary Silt Strength", Range(0, 1)) = 1
        _EstuaryColor ("Estuary Silt Colour", Color) = (0.325, 0.425, 0.445, 1)
        _EstuaryBlendHeight ("Estuary Blend Height (metres)", Float) = 2
        _SeaLevel ("Sea Level", Float) = 0
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.45
        _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.55
        _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.006
        _ShoreWaveStrength ("Bank Wave Strength", Range(0, 1)) = 0.35
        _ShoreWaveSpacing ("Bank Wave Spacing (metres)", Float) = 0.11
        _ShoreWaveSpeed ("Bank Wave Speed (metres/second)", Float) = -0.07
        _ShoreWaveDepth ("Bank Wave Range (metres)", Float) = 0.5
        _ShoreWaveNoiseWorldSize ("Bank Wave Noise World Size", Float) = 1
        _WhitewaterStrength ("Whitewater Strength", Range(0, 1)) = 0.9
        _WhitewaterSlopeStart ("Whitewater Slope Start", Range(0, 1)) = 0.05
        _WhitewaterSlopeFull ("Whitewater Slope Full", Range(0, 1)) = 0.55
    }

    SubShader
    {
        Tags { "Queue"="Transparent+10" "RenderType"="Transparent" }
        Blend SrcAlpha OneMinusSrcAlpha
        ZWrite Off
        Cull Off

        Pass
        {
            Tags { "LightMode"="ForwardBase" }

            CGPROGRAM
            #pragma vertex Vertex
            #pragma fragment Fragment
            #pragma multi_compile_fog
            #pragma multi_compile_fwdbase

            #include "WaterCommon.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 riverUv : TEXCOORD0;
            };

            struct VertexOutput
            {
                float4 position : SV_POSITION;
                float brightness : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float2 riverUv : TEXCOORD2;
                float4 screenPosition : TEXCOORD3;
                float surfaceEyeDepth : TEXCOORD4;
                float3 worldPosition : TEXCOORD5;
                UNITY_FOG_COORDS(6)
                float3 islandLocalPosition : TEXCOORD7;
            };

            sampler2D _NoiseTex;
            float _CoarseNoiseWorldSize;
            float _FineNoiseWorldSize;
            float _CoarseFlowSpeed;
            float _FineFlowSpeed;
            float _WorldSize;
            half _EstuaryStrength;
            fixed4 _EstuaryColor;
            float _EstuaryBlendHeight;
            float _SeaLevel;
            half _ShoreWaveStrength;
            float _ShoreWaveSpacing;
            float _ShoreWaveSpeed;
            float _ShoreWaveDepth;
            float _ShoreWaveNoiseWorldSize;
            half _WhitewaterStrength;
            half _WhitewaterSlopeStart;
            half _WhitewaterSlopeFull;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.position = UnityObjectToClipPos(input.vertex);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.worldNormal = normal;
                output.riverUv = input.riverUv;
                output.screenPosition = ComputeScreenPos(output.position);
                output.surfaceEyeDepth = -UnityObjectToViewPos(input.vertex).z;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                output.brightness = 0.72
                    + 0.28 * saturate(dot(
                        normal,
                        normalize(float3(0.3, 1.0, 0.2))));
                UNITY_TRANSFER_FOG(output, output.position);
                return output;
            }

            fixed4 Fragment(VertexOutput input) : SV_Target
            {
                float coarseSize = max(_CoarseNoiseWorldSize, 0.01);
                float fineSize = max(_FineNoiseWorldSize, 0.01);
                float2 riverMetres = input.riverUv * _WorldSize;
                float2 coarseUv = (riverMetres
                    - float2(0.0, _Time.y * _CoarseFlowSpeed)) / coarseSize;
                float2 fineUv = (riverMetres
                    - float2(0.0, _Time.y * _FineFlowSpeed)) / fineSize;
                half coarseNoise = tex2D(_NoiseTex, coarseUv).r;
                half fineNoise = tex2D(_NoiseTex, fineUv).g;
                half coarseFoam = smoothstep(0.42h, 0.68h, coarseNoise);
                half fineFoam = smoothstep(0.24h, 0.80h, fineNoise);

                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 worldNormal = MotuFacingWaterNormal(
                    input.worldNormal,
                    viewDirection);
                half verticalAlignment = abs(worldNormal.y);
                half normalDeviation = 1.0h - verticalAlignment;
                half fineSlopeWhitewater = smoothstep(
                    _WhitewaterSlopeStart,
                    max(_WhitewaterSlopeFull, _WhitewaterSlopeStart + 0.001h),
                    normalDeviation);
                float waterDepth = MotuWaterDepth(
                    input.screenPosition,
                    input.surfaceEyeDepth);
                float heightAboveSea = max(
                    input.islandLocalPosition.y - _SeaLevel,
                    0.0);
                half estuaryWeight = _EstuaryStrength * (
                    1.0h - smoothstep(
                        0.0,
                        max(_EstuaryBlendHeight, 0.001),
                        heightAboveSea));

                half coarseWhitewater = coarseFoam
                    * 0.03h
                    * (1.0h - estuaryWeight);
                half fineWhitewater = fineFoam
                    * 0.50h
                    * fineSlopeWhitewater;
                half layeredWhitewater = coarseWhitewater
                    + fineWhitewater * (1.0h - coarseWhitewater);
                half whitewater = saturate(
                    layeredWhitewater * _WhitewaterStrength);

                // River UV.x is the generated horizontal distance from a bank.
                float shoreDepth = max(input.riverUv.x, 0.0) * _WorldSize;
                float shoreSpacing = max(_ShoreWaveSpacing, 0.001);
                float shoreRange = max(_ShoreWaveDepth, shoreSpacing);
                float2 shoreNoiseUv = input.islandLocalPosition.xz
                    / max(_ShoreWaveNoiseWorldSize, 0.001);
                half shoreNoise = tex2D(_NoiseTex, shoreNoiseUv).r - 0.5h;
                float shorePhase = (shoreDepth
                    + _Time.y * _ShoreWaveSpeed
                    + shoreNoise * shoreSpacing * 0.45) / shoreSpacing;
                half shoreCrest = smoothstep(
                    0.72h,
                    0.98h,
                    0.5h + 0.5h * cos(shorePhase * 6.2831853));
                float contactStart = shoreRange * 0.01;
                float contactEnd = max(
                    shoreRange * 0.05,
                    contactStart + 0.0001);
                half contactFade = smoothstep(
                    contactStart,
                    contactEnd,
                    shoreDepth);
                half deepFade = 1.0h - smoothstep(
                    max(shoreRange - shoreSpacing, shoreSpacing),
                    shoreRange,
                    shoreDepth);
                half horizontalSurface = smoothstep(
                    0.65h,
                    0.96h,
                    verticalAlignment);
                half bankWave = saturate(
                    shoreCrest
                    * contactFade
                    * deepFade
                    * horizontalSurface
                    * _ShoreWaveStrength
                    * (1.0h - estuaryWeight));
                whitewater = whitewater + bankWave * (1.0h - whitewater);

                float normalOpacityDepth = max(_OpacityDepth, 0.001);
                float heightOpacityDepth = min(
                    heightAboveSea,
                    normalOpacityDepth);
                float opacityDepth = lerp(
                    normalOpacityDepth,
                    heightOpacityDepth,
                    _EstuaryStrength);
                half waterOpacity = MotuWaterOpacity(
                    waterDepth,
                    opacityDepth);
                fixed3 waterBody = lerp(
                    _Color.rgb * input.brightness,
                    _EstuaryColor.rgb,
                    estuaryWeight);
                fixed3 water = MotuShadeWater(
                    waterBody,
                    worldNormal,
                    viewDirection,
                    input.worldPosition,
                    half2(coarseNoise, fineNoise) - 0.5h,
                    estuaryWeight);
                fixed4 color = fixed4(
                    lerp(water, fixed3(1.0, 1.0, 1.0), whitewater),
                    saturate(waterOpacity + whitewater * 0.08h));
                UNITY_APPLY_FOG(input.fogCoord, color);
                return color;
            }
            ENDCG
        }
    }
}
