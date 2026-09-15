Shader "Motu/River Water"
{
    Properties
    {
        _RippleStrength ("Fine Ripple Strength", Range(0, 0.5)) = 0.1
        _SurfaceRoughness ("Surface Roughness", Range(0.04, 0.6)) = 0.18
        _RapidRoughness ("Rapids Roughness", Range(0, 0.4)) = 0.18
        _AbsorptionCoefficients ("RGB Absorption (per metre)", Vector) = (0.32, 0.09, 0.16, 0)
        _AbsorptionStrength ("Absorption Strength", Range(0, 4)) = 1
        _FoamWorldSize ("Foam Patch Width (metres)", Float) = 0.7
        _WaterfallFlowSpeed ("Waterfall Flow Speed (metres/second)", Float) = 9
        _FoamStretch ("Foam Downstream Stretch", Range(1, 8)) = 3
        _Color ("Water Colour", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("River Noise", 2D) = "black" {}
        _CoarseNoiseWorldSize ("Broad Ripple Wavelength (metres)", Float) = 1.2
        _FineNoiseWorldSize ("Fine Ripple Wavelength (metres)", Float) = 0.65
        _CoarseFlowSpeed ("Broad Flow Speed (metres/second)", Float) = 0.8
        _FineFlowSpeed ("Fine Flow Speed (metres/second)", Float) = 1.4
        _WorldSize ("World Size", Float) = 2000
        [HideInInspector] _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        [HideInInspector] _OpacityDepth ("Full Opacity Depth", Float) = 5
        _SeaColor ("Sea Colour", Color) = (0.03, 0.28, 0.55, 1)
        _EstuaryBlendHeight ("Estuary Blend Height (metres)", Float) = 2
        _SeaLevel ("Sea Level", Float) = 0
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.45
        [HideInInspector] _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.55
        [HideInInspector] _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        [HideInInspector] _WaterSkyExposure ("Water Sky Exposure", Range(0, 1)) = 1
        _RefractionStrength ("Underwater Distortion", Range(0, 0.03)) = 0.012
        _RefractionDepth ("Full Distortion Depth (metres)", Float) = 0.6
        [HideInInspector] _PlanarReflectionWeight ("Planar Reflection Weight", Range(0, 1)) = 1
        _PlanarReflectionDistortion ("Reflection Ripple Distortion", Range(0, 0.03)) = 0.006
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
            #include "WaterOptics.cginc"
            #include "RiverSurface.cginc"

            struct VertexInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 riverUv : TEXCOORD0;
            };

            struct VertexOutput
            {
                float4 pos : SV_POSITION;
                float brightness : TEXCOORD0;
                float3 worldNormal : TEXCOORD1;
                float2 riverUv : TEXCOORD2;
                float4 screenPosition : TEXCOORD3;
                float surfaceEyeDepth : TEXCOORD4;
                float3 worldPosition : TEXCOORD5;
                UNITY_FOG_COORDS(6)
                float3 islandLocalPosition : TEXCOORD7;
                float4 grabPosition : TEXCOORD8;
                SHADOW_COORDS(9)
            };

            float _WorldSize;
            fixed4 _SeaColor;
            float _EstuaryBlendHeight;
            float _SeaLevel;
            half _WhitewaterStrength;
            half _WhitewaterSlopeStart;
            half _WhitewaterSlopeFull;

            VertexOutput Vertex(VertexInput input)
            {
                VertexOutput output;
                output.pos = UnityObjectToClipPos(input.vertex);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.worldNormal = normal;
                output.riverUv = input.riverUv;
                output.screenPosition = ComputeScreenPos(output.pos);
                output.grabPosition = ComputeGrabScreenPos(output.pos);
                output.surfaceEyeDepth = -UnityObjectToViewPos(input.vertex).z;
                output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                output.islandLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(output.worldPosition, 1.0)).xyz;
                output.brightness = 0.72
                    + 0.28 * saturate(dot(
                        normal,
                        normalize(float3(0.3, 1.0, 0.2))));
                TRANSFER_SHADOW_WPOS(output, output.worldPosition);
                UNITY_TRANSFER_FOG(output, output.pos);
                return output;
            }

            float4 Fragment(VertexOutput input) : SV_Target
            {
                float2 riverMetres = input.riverUv * _WorldSize;
                float3 viewDirection = normalize(
                    _WorldSpaceCameraPos.xyz - input.worldPosition);
                float3 worldNormal = MotuFacingWaterNormal(
                    input.worldNormal,
                    viewDirection);
                UNITY_LIGHT_ATTENUATION(
                    shadowAttenuation,
                    input,
                    input.worldPosition);
                half verticalAlignment = abs(normalize(input.worldNormal).y);
                half normalDeviation = 1.0h - verticalAlignment;
                half fineSlopeWhitewater = smoothstep(
                    _WhitewaterSlopeStart,
                    max(_WhitewaterSlopeFull, _WhitewaterSlopeStart + 0.001h),
                    normalDeviation);
                float roughness;
                float3 detailNormal = MotuRiverDetailNormal(input.worldPosition, input.worldNormal,
                    riverMetres, _Time.y, fineSlopeWhitewater, roughness);
                worldNormal = MotuFacingWaterNormal(detailNormal, viewDirection);
                float waterDepth = MotuWaterDepth(
                    input.screenPosition,
                    input.surfaceEyeDepth);
                float heightAboveSea = max(
                    input.islandLocalPosition.y - _SeaLevel,
                    0.0);
                half estuaryWeight = 1.0h - smoothstep(
                    0.0,
                    max(_EstuaryBlendHeight, 0.001),
                    heightAboveSea);

                half whitewater = saturate(MotuRiverFoam(riverMetres, _Time.y,
                    fineSlopeWhitewater) * _WhitewaterStrength);

                half horizontalSurface = smoothstep(.65h, .96h, verticalAlignment);

                MotuCloudLighting cloud = MotuCloudSurfaceLighting(input.worldPosition);
                fixed3 waterIllumination = MotuWaterIllumination(
                    worldNormal,
                    input.worldPosition,
                    shadowAttenuation,
                    0.12h,
                    cloud);
                fixed3 waterBody = lerp(
                    _Color.rgb,
                    _SeaColor.rgb,
                    estuaryWeight)
                    * input.brightness
                    * waterIllumination;
                float2 ripple = mul((float3x3)UNITY_MATRIX_V,
                    detailNormal - normalize(input.worldNormal)).xy;
                float2 refractionRipple = ripple + MotuRiverDepthDistortion(riverMetres,
                    _Time.y, fineSlopeWhitewater);
                float pathLength;
                float3 refracted = MotuWaterRefractDepthSafe(input.grabPosition, input.screenPosition,
                    input.surfaceEyeDepth, waterDepth, worldNormal, viewDirection, refractionRipple, pathLength);
                // The shared reflection camera mirrors sea level, not uphill rivers
                // or vertical waterfalls. Restrict it to the flat estuary surface.
                float planarWeight = horizontalSurface * (1 - smoothstep(0, .25, heightAboveSea));
                float3 water = MotuWaterShadeOptics(waterBody, refracted, pathLength,
                    worldNormal, viewDirection, input.worldPosition, ripple, roughness,
                    shadowAttenuation, cloud, planarWeight);
                fixed3 foamColour = MotuWaterIllumination(
                    worldNormal,
                    input.worldPosition,
                    shadowAttenuation,
                    0.35h,
                    cloud);
                float4 result = float4(lerp(water, foamColour, whitewater), 1);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }
    }
}
