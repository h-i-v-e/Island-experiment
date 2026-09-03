Shader "Motu/Planar Reflection Simplified"
{
    Properties
    {
        _Color ("Tint", Color) = (1, 1, 1, 1)
        _GroundDirtColor ("Dirt", Color) = (0.09, 0.055, 0.026, 1)
        _RockColor ("Rock", Color) = (0.30, 0.32, 0.29, 1)
        _SandColor ("Sand", Color) = (0.62, 0.57, 0.34, 1)
        _GrassColorA ("Grass A", Color) = (0.18, 0.46, 0.14, 1)
        _GrassColorB ("Grass B", Color) = (0.34, 0.50, 0.14, 1)
        _BaseColor ("Dark Surface", Color) = (0.10, 0.16, 0.06, 1)
        _LightColor ("Light Surface", Color) = (0.30, 0.46, 0.16, 1)
        [PerRendererData] _RockTint ("Rock Tint", Color) = (1, 1, 1, 1)
        _SnowLine ("Snow Line", Float) = 100
        _TipColor ("Reed Tip", Color) = (0.38, 0.48, 0.09, 1)
        _Cutoff ("Reed Cutoff", Range(0, 1)) = 0.46
        _ReedWindMultiplier ("Reed Wind", Float) = 3
        _ReedFadeStart ("Reed Fade Start", Float) = 34
        _ReedFadeEnd ("Reed Fade End", Float) = 47
        _FernWindMultiplier ("Fern Wind", Float) = 1.8
        _FernFadeStart ("Fern Fade Start", Float) = 34
        _FernFadeEnd ("Fern Fade End", Float) = 47
        _WorldSize ("Island World Size", Float) = 2000
        [NoScaleOffset] _GrassPatchNoise ("Wind Noise", 2D) = "white" {}
        _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
        _GrassWindDirection ("Wind Direction", Vector) = (1, 0, 0.35, 0)
        _GrassWindStrength ("Wind Strength", Float) = 0.07
        _GrassWindSpeed ("Wind Speed", Float) = 1.8
        _GrassWindWorldSize ("Wind Size", Float) = 12
    }

    CGINCLUDE
    #include "UnityCG.cginc"
    #include "Lighting.cginc"
    sampler2D _GrassPatchNoise;
    float4 _GrassWindDirection;
    float _GrassWindStrength;
    float _GrassWindSpeed;
    float _GrassWindWorldSize;
    #include "GrassWindCommon.cginc"

    struct ReflectionVertexInput
    {
        float4 vertex : POSITION;
        float3 normal : NORMAL;
        float4 material : COLOR;
        float2 environment : TEXCOORD1;
        UNITY_VERTEX_INPUT_INSTANCE_ID
    };

    struct ReflectionVertexOutput
    {
        float4 position : SV_POSITION;
        half3 worldNormal : TEXCOORD0;
        float3 worldPosition : TEXCOORD1;
        float3 localPosition : TEXCOORD2;
        half4 material : TEXCOORD3;
        half2 environment : TEXCOORD4;
        UNITY_VERTEX_OUTPUT_STEREO
    };

    fixed4 _Color;
    fixed4 _GroundDirtColor;
    fixed4 _RockColor;
    fixed4 _SandColor;
    fixed4 _GrassColorA;
    fixed4 _GrassColorB;
    fixed4 _BaseColor;
    fixed4 _LightColor;
    fixed4 _TipColor;
    fixed4 _RockTint;
    float _SnowLine;
    half _Cutoff;
    float _ReedWindMultiplier;
    float _ReedFadeStart;
    float _ReedFadeEnd;
    float _FernWindMultiplier;
    float _FernFadeStart;
    float _FernFadeEnd;
    float _WorldSize;
    float4 _GrassPlayerPosition;
    float4 _PlanarReflectionViewerPosition;
    float4x4 _IslandWorldToLocal;

    #include "CloudCommon.cginc"

    ReflectionVertexOutput ReflectionVertex(ReflectionVertexInput input)
    {
        ReflectionVertexOutput output;
        UNITY_SETUP_INSTANCE_ID(input);
        UNITY_INITIALIZE_OUTPUT(ReflectionVertexOutput, output);
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        output.position = UnityObjectToClipPos(input.vertex);
        output.worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
        output.localPosition = mul(
            _IslandWorldToLocal,
            float4(output.worldPosition, 1.0)).xyz;
        output.worldNormal = UnityObjectToWorldNormal(input.normal);
        output.material = input.material;
        output.environment = input.environment;
        return output;
    }

    fixed4 ApplyReflectionDistanceHaze(
        fixed4 colour,
        float3 worldPosition)
    {
        #if defined(FOG_LINEAR) || defined(FOG_EXP) || defined(FOG_EXP2)
            float hazeDistance = distance(
                worldPosition,
                _PlanarReflectionViewerPosition.xyz);
            UNITY_CALC_FOG_FACTOR_RAW(hazeDistance);
            colour.rgb = lerp(
                unity_FogColor.rgb,
                colour.rgb,
                saturate(unityFogFactor));
        #endif
        return colour;
    }

    fixed3 ReflectionLighting(
        fixed3 albedo,
        half3 worldNormal,
        float3 worldPosition)
    {
        half3 normal = normalize(worldNormal);
        MotuCloudLighting cloud = MotuCloudSurfaceLighting(worldPosition);
        half3 ambient = max(
            ShadeSH9(half4(normal, 1.0h))
                * cloud.ambientTransmittance,
            0.08h);
        half3 lightDirection = normalize(UnityWorldSpaceLightDir(worldPosition));
        half diffuse = saturate(dot(normal, lightDirection));
        return albedo * (ambient + _LightColor0.rgb
            * (0.18h + diffuse * 0.72h)
            * cloud.directTransmittance);
    }

    fixed4 FinishReflection(
        fixed3 albedo,
        ReflectionVertexOutput input)
    {
        fixed4 result = fixed4(
            ReflectionLighting(albedo, input.worldNormal, input.worldPosition),
            1.0h);
        return ApplyReflectionDistanceHaze(result, input.worldPosition);
    }

    fixed4 TerrainFragment(ReflectionVertexOutput input) : SV_Target
    {
        half up = saturate(normalize(input.worldNormal).y);
        half slope = 1.0h - up;
        half looseCover = saturate(input.material.g);
        half materialRock = saturate(input.material.r) * (1.0h - looseCover);
        half rockWeight = max(
            materialRock,
            smoothstep(0.18h, 0.58h, slope));
        rockWeight = max(rockWeight, saturate(input.environment.y) * 0.65h);
        fixed3 grass = lerp(_GrassColorA.rgb, _GrassColorB.rgb, 0.45h);
        fixed3 albedo = lerp(
            _GroundDirtColor.rgb,
            grass,
            smoothstep(0.12h, 0.72h, looseCover));
        half sandWeight = (1.0h - smoothstep(0.5h, 4.0h, input.localPosition.y))
            * (1.0h - rockWeight);
        albedo = lerp(albedo, _SandColor.rgb, sandWeight);
        albedo = lerp(albedo, _RockColor.rgb, rockWeight);
        half snowWeight = smoothstep(
            _SnowLine,
            _SnowLine + 8.0,
            input.localPosition.y) * smoothstep(0.12h, 0.55h, up);
        albedo = lerp(albedo, fixed3(0.82h, 0.84h, 0.81h), snowWeight);
        return FinishReflection(albedo * _Color.rgb, input);
    }

    fixed4 GrassFragment(ReflectionVertexOutput input) : SV_Target
    {
        fixed3 albedo = lerp(_GrassColorA.rgb, _GrassColorB.rgb, 0.45h);
        return FinishReflection(albedo, input);
    }

    fixed4 WoodFragment(ReflectionVertexOutput input) : SV_Target
    {
        fixed3 albedo = lerp(_BaseColor.rgb, _LightColor.rgb, 0.38h);
        return FinishReflection(albedo, input);
    }

    fixed4 FoliageFragment(ReflectionVertexOutput input) : SV_Target
    {
        fixed3 albedo = lerp(_BaseColor.rgb, _LightColor.rgb, 0.42h);
        return FinishReflection(albedo, input);
    }

    fixed4 RockFragment(ReflectionVertexOutput input) : SV_Target
    {
        return FinishReflection(_RockColor.rgb * _RockTint.rgb, input);
    }
    ENDCG

    SubShader
    {
        Tags { "MotuReflection"="Terrain" "RenderType"="Opaque" }
        LOD 50
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment TerrainFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Grass" "RenderType"="Opaque" }
        LOD 50
        Cull Off
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment GrassFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Wood" "RenderType"="Opaque" }
        LOD 50
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment WoodFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Foliage" "RenderType"="Opaque" }
        LOD 50
        Cull Off
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment FoliageFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Reeds" "RenderType"="TransparentCutout" }
        LOD 50
        Cull Off
        AlphaToMask On
        Pass
        {
            CGPROGRAM
            #pragma vertex ReedReflectionVertex
            #pragma fragment ReedReflectionFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            struct ReedInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 uv : TEXCOORD0;
                float2 root : TEXCOORD1;
                float4 data : COLOR;
            };

            struct ReedOutput
            {
                float4 position : SV_POSITION;
                float3 worldPosition : TEXCOORD0;
                half3 worldNormal : TEXCOORD1;
                float2 uv : TEXCOORD2;
                half4 data : TEXCOORD3;
                float distanceToPlayer : TEXCOORD4;
            };

            float ReedHash(float value)
            {
                return frac(sin(value * 91.3458 + 17.13) * 47453.5453);
            }

            float ReedReflectionMask(float2 uv, float4 data)
            {
                float mask = 0.0;
                [unroll]
                for (int stem = 0; stem < 7; stem++)
                {
                    float seed = stem + data.w * 19.0 + data.y * 37.0 + data.x * 11.0;
                    float centre = (stem + 0.5) / 7.0 + (ReedHash(seed) - 0.5) * 0.075;
                    float height = lerp(0.60, 1.0, ReedHash(seed + 2.7))
                        * lerp(1.0, 0.78, data.x);
                    float lean = (ReedHash(seed + 7.1) - 0.5) * 0.12 * uv.y;
                    float width = lerp(0.010, 0.025, ReedHash(seed + 4.2))
                        * lerp(0.55, 1.0, data.x)
                        * lerp(1.0, 0.45, saturate(uv.y / max(height, 0.01)));
                    float blade = 1.0 - smoothstep(
                        width,
                        width + fwidth(uv.x) * 1.5,
                        abs(uv.x - centre - lean));
                    blade *= 1.0 - smoothstep(height, height + fwidth(uv.y) * 2.0, uv.y);
                    blade *= step(0.015, uv.y);
                    float2 headDelta = float2(
                        (uv.x - centre - lean) / 0.035,
                        (uv.y - height + 0.015) / 0.075);
                    float head = (1.0 - smoothstep(0.70, 1.0, dot(headDelta, headDelta)))
                        * (1.0 - data.x);
                    mask = max(mask, max(blade, head));
                }
                return mask;
            }

            ReedOutput ReedReflectionVertex(ReedInput input)
            {
                ReedOutput output;
                float3 worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                float2 rootLocal = (input.root - 0.5) * _WorldSize;
                float2 rootWorld = mul(
                    unity_ObjectToWorld,
                    float4(rootLocal.x, 0.0, rootLocal.y, 1.0)).xz;
                float3 wind = MotuGrassWindSample(rootWorld);
                float bend = input.uv.y * input.uv.y;
                float flexibility = lerp(1.0, 0.35, saturate(input.data.z));
                worldPosition.xz += wind.xz
                    * (_GrassWindStrength * _ReedWindMultiplier * wind.y * flexibility * bend);
                output.worldPosition = worldPosition;
                output.position = UnityWorldToClipPos(worldPosition);
                output.worldNormal = UnityObjectToWorldNormal(input.normal);
                output.uv = input.uv;
                output.data = input.data;
                output.distanceToPlayer = distance(worldPosition.xz, _GrassPlayerPosition.xz);
                return output;
            }

            fixed4 ReedReflectionFragment(ReedOutput input, fixed facing : VFACE) : SV_Target
            {
                float mask = ReedReflectionMask(input.uv, input.data);
                float fade = 1.0 - smoothstep(
                    _ReedFadeStart,
                    _ReedFadeEnd,
                    input.distanceToPlayer);
                float dither = frac(sin(dot(input.position.xy, float2(12.9898, 78.233))) * 43758.5453);
                clip(mask - _Cutoff);
                clip(fade - dither);
                half3 normal = normalize(input.worldNormal) * (facing >= 0 ? 1.0h : -1.0h);
                fixed3 albedo = lerp(_BaseColor.rgb, _TipColor.rgb, input.uv.y)
                    * lerp(0.88, 1.12, input.data.y);
                fixed4 result = fixed4(
                    ReflectionLighting(albedo, normal, input.worldPosition),
                    1.0h);
                return ApplyReflectionDistanceHaze(result, input.worldPosition);
            }
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Rock" "RenderType"="Opaque" }
        LOD 50
        Pass
        {
            CGPROGRAM
            #pragma vertex ReflectionVertex
            #pragma fragment RockFragment
            #pragma target 3.0
            #pragma multi_compile_fog
            #pragma multi_compile_instancing
            ENDCG
        }
    }

    SubShader
    {
        Tags { "MotuReflection"="Ferns" "RenderType"="TransparentCutout" }
        LOD 50
        Cull Off
        AlphaToMask On
        Pass
        {
            CGPROGRAM
            #pragma vertex FernReflectionVertex
            #pragma fragment FernReflectionFragment
            #pragma target 3.0
            #pragma multi_compile_fog

            struct FernInput
            {
                float4 vertex : POSITION;
                float3 normal : NORMAL;
                float2 uv : TEXCOORD0;
                float2 root : TEXCOORD1;
                float4 data : COLOR;
            };

            struct FernOutput
            {
                float4 position : SV_POSITION;
                float3 worldPosition : TEXCOORD0;
                half3 worldNormal : TEXCOORD1;
                float2 uv : TEXCOORD2;
                half4 data : TEXCOORD3;
                float distanceToPlayer : TEXCOORD4;
            };

            float FernReflectionMask(float2 uv, float4 data)
            {
                float edge = max(fwidth(uv.x), fwidth(uv.y)) * 1.5;
                float taper = pow(saturate(1.0 - uv.y), 0.56);
                float rachisWidth = lerp(0.036, 0.015, uv.y);
                float mask = 1.0 - smoothstep(
                    rachisWidth,
                    rachisWidth + edge,
                    abs(uv.x - 0.5));
                [unroll]
                for (int leaflet = 0; leaflet < 8; leaflet++)
                {
                    float progress = (leaflet + 1.0) / 10.0;
                    float width = (0.34 * taper + 0.04)
                        * lerp(0.92, 1.08, frac(data.x * 7.3 + leaflet * 0.37));
                    float height = lerp(0.055, 0.032, progress);
                    float offset = (leaflet & 1) == 0 ? -0.010 : 0.010;
                    float2 left = float2(
                        (uv.x - (0.5 - width * 0.52)) / max(width, 0.01),
                        (uv.y - progress - offset) / height);
                    float2 right = float2(
                        (uv.x - (0.5 + width * 0.52)) / max(width, 0.01),
                        (uv.y - progress + offset) / height);
                    mask = max(mask, max(
                        1.0 - smoothstep(0.82, 1.0, dot(left, left)),
                        1.0 - smoothstep(0.82, 1.0, dot(right, right))));
                }
                return mask * smoothstep(0.0, 0.025, uv.y);
            }

            FernOutput FernReflectionVertex(FernInput input)
            {
                FernOutput output;
                float3 worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
                float2 rootLocal = (input.root - 0.5) * _WorldSize;
                float2 rootWorld = mul(
                    unity_ObjectToWorld,
                    float4(rootLocal.x, 0.0, rootLocal.y, 1.0)).xz;
                float3 wind = MotuGrassWindSample(rootWorld);
                float bend = input.uv.y * input.uv.y;
                float flexibility = lerp(0.45, 1.0, saturate(input.data.z));
                float strength = _GrassWindStrength * _FernWindMultiplier
                    * wind.y * flexibility;
                worldPosition.xz += wind.xz * (strength * bend);
                output.worldPosition = worldPosition;
                output.position = UnityWorldToClipPos(worldPosition);
                output.worldNormal = UnityObjectToWorldNormal(input.normal);
                output.uv = input.uv;
                output.data = input.data;
                output.distanceToPlayer = distance(worldPosition.xz, _GrassPlayerPosition.xz);
                return output;
            }

            fixed4 FernReflectionFragment(FernOutput input, fixed facing : VFACE) : SV_Target
            {
                float fade = 1.0 - smoothstep(
                    _FernFadeStart,
                    _FernFadeEnd,
                    input.distanceToPlayer);
                float dither = frac(
                    sin(dot(input.position.xy, float2(12.9898, 78.233))) * 43758.5453);
                clip(FernReflectionMask(input.uv, input.data) - _Cutoff);
                clip(fade - dither);
                half3 normal = normalize(input.worldNormal) * (facing >= 0 ? 1.0h : -1.0h);
                fixed3 albedo = lerp(_BaseColor.rgb, _TipColor.rgb, input.uv.y * 0.72)
                    * lerp(0.84, 1.16, input.data.y);
                fixed4 result = fixed4(
                    ReflectionLighting(albedo, normal, input.worldPosition),
                    1.0h);
                return ApplyReflectionDistanceHaze(result, input.worldPosition);
            }
            ENDCG
        }
    }

    FallBack Off
}
