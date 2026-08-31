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
        [HideInInspector] _StarSettings ("Star Settings", Vector) = (0.18, 1.35, 0.052, 0)
        [HideInInspector] _StarVisibility ("Star Visibility", Range(0, 1)) = 0
        [HideInInspector] _StarRotation ("Star Rotation", Vector) = (0, 0, 1, 0)
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
            float4 _StarSettings;
            float _StarVisibility;
            float4 _StarRotation;

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

            float MotuStarHash(float3 value)
            {
                value = frac(value * 0.1031);
                value += dot(value, value.yzx + 33.33);
                return frac((value.x + value.y) * value.z);
            }

            void MotuStarFaceCoordinates(
                float3 direction,
                out float2 faceUv,
                out float faceIndex)
            {
                float3 absoluteDirection = abs(direction);
                if (absoluteDirection.x >= absoluteDirection.y
                    && absoluteDirection.x >= absoluteDirection.z)
                {
                    faceUv = direction.x >= 0.0
                        ? float2(-direction.z, direction.y) / absoluteDirection.x
                        : float2(direction.z, direction.y) / absoluteDirection.x;
                    faceIndex = direction.x >= 0.0 ? 0.0 : 1.0;
                }
                else if (absoluteDirection.y >= absoluteDirection.z)
                {
                    faceUv = direction.y >= 0.0
                        ? float2(direction.x, -direction.z) / absoluteDirection.y
                        : float2(direction.x, direction.z) / absoluteDirection.y;
                    faceIndex = direction.y >= 0.0 ? 2.0 : 3.0;
                }
                else
                {
                    faceUv = direction.z >= 0.0
                        ? float2(direction.x, direction.y) / absoluteDirection.z
                        : float2(-direction.x, direction.y) / absoluteDirection.z;
                    faceIndex = direction.z >= 0.0 ? 4.0 : 5.0;
                }
                faceUv = faceUv * 0.5 + 0.5;
            }

            fixed3 MotuNightStars(float3 skyDirection)
            {
                float3 rotationAxis = normalize(_StarRotation.xyz);
                float rotationCosine = cos(_StarRotation.w);
                float rotationSine = sin(_StarRotation.w);
                float3 starDirection = skyDirection * rotationCosine
                    + cross(rotationAxis, skyDirection) * rotationSine
                    + rotationAxis
                        * dot(rotationAxis, skyDirection)
                        * (1.0 - rotationCosine);
                float2 faceUv;
                float faceIndex;
                MotuStarFaceCoordinates(starDirection, faceUv, faceIndex);
                const float StarCellsPerFace = 40.0;
                float2 starCoordinate = faceUv * StarCellsPerFace;
                float2 cell = floor(starCoordinate);
                float2 cellCoordinate = frac(starCoordinate);
                float seed = _StarSettings.w;
                float3 key = float3(cell, faceIndex * 37.0 + seed);
                float presence = MotuStarHash(key);
                float2 centre = 0.25 + 0.5 * float2(
                    MotuStarHash(key + float3(11.7, 3.1, 5.3)),
                    MotuStarHash(key + float3(2.9, 17.3, 7.1)));
                float variation = MotuStarHash(
                    key + float3(19.1, 23.7, 13.9));
                float radius = _StarSettings.z * lerp(0.55, 1.25, variation);
                float distanceToStar = length(cellCoordinate - centre);
                float antialias = max(fwidth(distanceToStar) * 1.5, 0.002);
                float core = 1.0 - smoothstep(
                    radius - antialias,
                    radius + antialias,
                    distanceToStar);
                float glowRadius = min(radius * 3.25 + antialias, 0.22);
                float glow = 1.0 - smoothstep(
                    radius,
                    glowRadius,
                    distanceToStar);
                float occupied = step(
                    1.0 - saturate(_StarSettings.x),
                    presence) * step(0.0001, _StarSettings.x);
                float twinkle = 0.88 + 0.12 * sin(
                    _Time.y * lerp(0.45, 1.35, variation)
                    + presence * 41.0);
                float brightness = lerp(0.24, 1.0, variation * variation)
                    * (core + glow * 0.16)
                    * occupied
                    * twinkle
                    * max(_StarSettings.y, 0.0);
                float horizonFade = smoothstep(0.015, 0.16, skyDirection.y);
                float nightFade = smoothstep(0.12, 0.82, _StarVisibility);
                fixed3 warmStar = fixed3(1.0, 0.78, 0.58);
                fixed3 coolStar = fixed3(0.68, 0.82, 1.0);
                fixed3 starColour = lerp(
                    warmStar,
                    coolStar,
                    MotuStarHash(key + float3(29.3, 31.7, 37.9)));
                starColour = lerp(fixed3(1.0, 1.0, 1.0), starColour, 0.38);
                return starColour * brightness * horizonFade * nightFade;
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
                fixed3 stars = MotuNightStars(skyDirection);
                MotuCloudSkyVolume cloudVolume = MotuCloudSkyVolumeAt(
                    cameraLocalPosition,
                    skyDirection,
                    input.position.xy);
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
                half cloudTransmittance = pow(
                    max(cloudVolume.transmittance, 0.0001h),
                    max(_MotuCloudCelestialStrength, 0.0));
                sky.rgb = sky.rgb * cloudTransmittance
                    + cloudVolume.averageColour * (1.0h - cloudTransmittance);
                // Stars are tiny HDR sources, so ordinary translucent-cloud
                // blending can leave them visibly punching through. Treat the
                // integrated volume as a substantially denser astronomical
                // occluder while retaining softer cloud/sky compositing.
                half starTransmittance = pow(
                    saturate(cloudVolume.transmittance),
                    4.0h);
                sky.rgb += stars * starTransmittance;
                return sky;
            }
            ENDCG
        }
    }
    Fallback Off
}
