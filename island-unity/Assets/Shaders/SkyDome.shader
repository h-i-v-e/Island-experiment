Shader "Motu/Sky Dome"
{
    Properties
    {
        [HideInInspector] _Color ("Color", Color) = (1, 1, 1, 1)
        _HorizonColor ("Horizon Color", Color) = (0.57, 0.68, 0.73, 1)
        _ZenithColor ("Zenith Color", Color) = (0.37, 0.60, 0.78, 1)
        _GradientPower ("Gradient Power", Range(0.1, 4.0)) = 0.7
        [HideInInspector] _SunDirection ("Sun Direction", Vector) = (0, 1, 0, 0)
        [HDR] _SunColor ("Sun Color", Color) = (1, 0.94, 0.82, 1)
        [HideInInspector] _SunDiscCosRadius ("Sun Disc Cosine Radius", Float) = 0.9999
        [HideInInspector] _SunVisibility ("Sun Visibility", Range(0, 1)) = 1
        [HDR] _SunHaloColor ("Sunset Sun Halo Color", Color) = (0.85, 0.05, 0.01, 1)
        [HideInInspector] _SunHaloStrength ("Sunset Sun Halo Strength", Range(0, 1)) = 0
        [HideInInspector] _MoonDirection ("Moon Direction", Vector) = (0, 1, 0, 0)
        [HideInInspector] _MoonLightDirection ("Moon Light Direction", Vector) = (0, -1, 0, 0)
        [HDR] _MoonColor ("Moon Color", Color) = (0.78, 0.84, 0.92, 1)
        _MoonDarkColor ("Moon Dark Color", Color) = (0.012, 0.018, 0.035, 1)
        [HideInInspector] _MoonDiscCosRadius ("Moon Disc Cosine Radius", Float) = 0.9999
        [HideInInspector] _MoonVisibility ("Moon Visibility", Range(0, 1)) = 1
        [HideInInspector] _SkyExposure ("Sky Exposure", Range(0, 1)) = 1
    }

    SubShader
    {
        Tags
        {
            "Queue" = "Background"
            "RenderType" = "Opaque"
            "IgnoreProjector" = "True"
        }
        Cull Back
        ZWrite Off
        ZTest LEqual
        Fog { Mode Off }

        Pass
        {
            CGPROGRAM
            #pragma vertex Vert
            #pragma fragment Frag
            #pragma target 3.0
            #include "UnityCG.cginc"
            float4x4 _IslandWorldToLocal;
            #include "CloudCommon.cginc"

            fixed4 _HorizonColor;
            fixed4 _ZenithColor;
            fixed4 _SunColor;
            float4 _SunDirection;
            float _SunDiscCosRadius;
            float _SunVisibility;
            fixed4 _SunHaloColor;
            float _SunHaloStrength;
            fixed4 _MoonColor;
            fixed4 _MoonDarkColor;
            float4 _MoonDirection;
            float4 _MoonLightDirection;
            float _MoonDiscCosRadius;
            float _MoonVisibility;
            float _SkyExposure;
            float _GradientPower;

            struct VertexInput
            {
                float4 vertex : POSITION;
                float2 uv : TEXCOORD0;
            };

            struct VertexOutput
            {
                float4 position : SV_POSITION;
                float elevation : TEXCOORD0;
                float3 localDirection : TEXCOORD1;
            };

            void CelestialDiscCoordinates(
                float3 skyDirection,
                float3 geometricDirection,
                float discCosRadius,
                out float2 discCoordinate,
                out float discMask,
                out float3 horizontalAxis,
                out float3 verticalAxis)
            {
                // Approximate the apparent enlargement, vertical compression,
                // and lift caused by the long atmospheric path at the horizon.
                const float HorizonBandSine = 0.207912;
                const float HorizonRefractionLift = 0.0061;
                float horizonEffect = 1.0 - smoothstep(
                    0.0,
                    HorizonBandSine,
                    max(geometricDirection.y, 0.0));
                float3 apparentDirection = normalize(
                    geometricDirection
                    + float3(0.0, HorizonRefractionLift * horizonEffect, 0.0));
                float3 basisReference = abs(apparentDirection.y) < 0.95
                    ? float3(0.0, 1.0, 0.0)
                    : float3(0.0, 0.0, 1.0);
                horizontalAxis = normalize(cross(
                    basisReference,
                    apparentDirection));
                verticalAxis = cross(
                    apparentDirection,
                    horizontalAxis);
                float baseSinRadius = sqrt(max(
                    1.0 - discCosRadius * discCosRadius,
                    0.000001));
                float magnification = lerp(1.0, 1.60, horizonEffect);
                float verticalFlattening = lerp(1.0, 0.65, horizonEffect);
                discCoordinate = float2(
                    dot(skyDirection, horizontalAxis)
                        / (baseSinRadius * magnification),
                    dot(skyDirection, verticalAxis)
                        / (baseSinRadius * magnification * verticalFlattening));
                float radialDistanceSquared = dot(discCoordinate, discCoordinate);
                float edgeWidth = max(
                    fwidth(radialDistanceSquared) * 1.5,
                    0.002);
                discMask = 1.0 - smoothstep(
                    1.0 - edgeWidth,
                    1.0 + edgeWidth,
                    radialDistanceSquared);
                // Tangent-plane coordinates are identical for a direction and
                // its antipode. Reject the rear hemisphere so a below-horizon
                // disc cannot appear above the opposite horizon.
                discMask *= step(
                    0.0,
                    dot(skyDirection, apparentDirection));
            }

            VertexOutput Vert(VertexInput input)
            {
                VertexOutput output;
                output.position = UnityObjectToClipPos(input.vertex);
                output.elevation = input.uv.y;
                output.localDirection = input.vertex.xyz;
                return output;
            }

            fixed4 Frag(VertexOutput input) : SV_Target
            {
                float elevation = pow(saturate(input.elevation), _GradientPower);
                float blend = smoothstep(0.0, 1.0, elevation);
                fixed4 sky = lerp(_HorizonColor, _ZenithColor, blend);
                sky.rgb *= _SkyExposure;

                float3 cameraLocalPosition = mul(
                    _IslandWorldToLocal,
                    float4(_WorldSpaceCameraPos.xyz, 1.0)).xyz;
                float3 skyDirection = normalize(
                    input.localDirection - cameraLocalPosition);
                half cloudDensity = MotuCloudSkyDensity(
                    cameraLocalPosition,
                    skyDirection);
                float3 sunDirection = normalize(_SunDirection.xyz);
                float sunAngularSeparation = 1.0 - saturate(dot(
                    skyDirection,
                    sunDirection));
                float sunHaloFalloff = lerp(
                    260.0,
                    65.0,
                    saturate(_SunHaloStrength));
                float sunHalo = exp2(
                    -sunAngularSeparation * sunHaloFalloff)
                    * saturate(_SunHaloStrength)
                    * _SunVisibility;
                sky.rgb += _SunHaloColor.rgb * sunHalo;

                float3 moonDirection = normalize(_MoonDirection.xyz);
                float2 moonCoordinate;
                float moonDisc;
                float3 moonHorizontalAxis;
                float3 moonVerticalAxis;
                CelestialDiscCoordinates(
                    skyDirection,
                    moonDirection,
                    _MoonDiscCosRadius,
                    moonCoordinate,
                    moonDisc,
                    moonHorizontalAxis,
                    moonVerticalAxis);
                moonDisc *= _MoonVisibility;
                float moonSurfaceDepth = sqrt(saturate(
                    1.0 - dot(moonCoordinate, moonCoordinate)));
                float3 moonSurfaceNormal = normalize(
                    moonHorizontalAxis * moonCoordinate.x
                    + moonVerticalAxis * moonCoordinate.y
                    - moonDirection * moonSurfaceDepth);
                float moonLighting = dot(
                    moonSurfaceNormal,
                    normalize(_MoonLightDirection.xyz));
                float moonTerminatorWidth = max(fwidth(moonLighting) * 1.5, 0.015);
                float moonLit = smoothstep(
                    -moonTerminatorWidth,
                    moonTerminatorWidth,
                    moonLighting);
                half earthshine = (1.0h - _SkyExposure) * 0.12h;
                fixed3 moonSurface = lerp(
                    sky.rgb,
                    _MoonDarkColor.rgb,
                    earthshine);
                moonSurface = lerp(moonSurface, _MoonColor.rgb, moonLit);
                sky.rgb = lerp(sky.rgb, moonSurface, moonDisc);

                float2 sunCoordinate;
                float sunDisc;
                float3 sunHorizontalAxis;
                float3 sunVerticalAxis;
                CelestialDiscCoordinates(
                    skyDirection,
                    sunDirection,
                    _SunDiscCosRadius,
                    sunCoordinate,
                    sunDisc,
                    sunHorizontalAxis,
                    sunVerticalAxis);
                sunDisc *= _SunVisibility;
                sky.rgb = lerp(sky.rgb, _SunColor.rgb, sunDisc);
                half cloudTransmittance = MotuCloudCelestialTransmittance(
                    cloudDensity);
                fixed3 cloudColour = MotuCloudSkyColour(
                    cloudDensity,
                    skyDirection);
                sky.rgb = sky.rgb * cloudTransmittance
                    + cloudColour * (1.0h - cloudTransmittance);
                return sky;
            }
            ENDCG
        }
    }
    Fallback Off
}
