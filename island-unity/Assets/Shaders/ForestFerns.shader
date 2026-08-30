Shader "Motu/Forest Ferns"
{
    Properties
    {
        _BaseColor ("Base Colour", Color) = (0.055, 0.18, 0.045, 1)
        _TipColor ("Tip Colour", Color) = (0.24, 0.48, 0.12, 1)
        _Cutoff ("Silhouette Cutoff", Range(0, 1)) = 0.44
        _FernWindMultiplier ("Wind Multiplier", Range(0, 8)) = 1.8
        _FernFadeStart ("LOD 0 Fade Start", Float) = 34
        _FernFadeEnd ("LOD 0 Fade End", Float) = 47
        [HideInInspector] _WorldSize ("Island World Size", Float) = 2000
        [NoScaleOffset] [HideInInspector] _GrassPatchNoise ("Shared Wind Noise", 2D) = "white" {}
        [HideInInspector] _GrassPlayerPosition ("Player Position", Vector) = (0, 0, 0, 0)
        [HideInInspector] _GrassWindDirection ("Wind Direction", Vector) = (1, 0, 0.35, 0)
        [HideInInspector] _GrassWindStrength ("Wind Strength", Float) = 0.07
        [HideInInspector] _GrassWindSpeed ("Wind Speed", Float) = 1.8
        [HideInInspector] _GrassWindWorldSize ("Wind Gust Size", Float) = 12
    }

    CGINCLUDE
    #include "UnityCG.cginc"
    #include "Lighting.cginc"

    fixed4 _BaseColor;
    fixed4 _TipColor;
    sampler2D _GrassPatchNoise;
    float4 _GrassWindDirection;
    float _GrassWindStrength;
    float _GrassWindSpeed;
    float _GrassWindWorldSize;
    float _WorldSize;
    half _Cutoff;
    float _FernWindMultiplier;
    float _FernFadeStart;
    float _FernFadeEnd;
    float4 _GrassPlayerPosition;

    #include "GrassWindCommon.cginc"

    struct FernVertexInput
    {
        float4 vertex : POSITION;
        float3 normal : NORMAL;
        float2 uv : TEXCOORD0;
        float2 root : TEXCOORD1;
        float4 data : COLOR;
        UNITY_VERTEX_INPUT_INSTANCE_ID
    };

    struct FernVertexOutput
    {
        float4 pos : SV_POSITION;
        float3 worldPosition : TEXCOORD0;
        half3 worldNormal : TEXCOORD1;
        float2 uv : TEXCOORD2;
        half4 data : TEXCOORD3;
        float rootDistance : TEXCOORD4;
        UNITY_FOG_COORDS(5)
        UNITY_VERTEX_OUTPUT_STEREO
    };

    struct FernShadowOutput
    {
        float4 pos : SV_POSITION;
        float2 uv : TEXCOORD0;
        half4 data : TEXCOORD1;
        float rootDistance : TEXCOORD2;
    };

    float FernSilhouette(float2 uv, float4 data)
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
            float localWidth = (0.34 * taper + 0.04)
                * lerp(0.92, 1.08, frac(data.x * 7.3 + leaflet * 0.37));
            float localHeight = lerp(0.055, 0.032, progress);
            float verticalOffset = (leaflet & 1) == 0 ? -0.010 : 0.010;
            float2 left = float2(
                (uv.x - (0.5 - localWidth * 0.52)) / max(localWidth, 0.01),
                (uv.y - progress - verticalOffset) / localHeight);
            float2 right = float2(
                (uv.x - (0.5 + localWidth * 0.52)) / max(localWidth, 0.01),
                (uv.y - progress + verticalOffset) / localHeight);
            float leftLeaf = 1.0 - smoothstep(0.82, 1.0, dot(left, left));
            float rightLeaf = 1.0 - smoothstep(0.82, 1.0, dot(right, right));
            mask = max(mask, max(leftLeaf, rightLeaf));
        }
        float tip = 1.0 - smoothstep(
            0.72,
            1.0,
            dot(float2((uv.x - 0.5) / 0.11, (uv.y - 0.94) / 0.09),
                float2((uv.x - 0.5) / 0.11, (uv.y - 0.94) / 0.09)));
        mask = max(mask, tip);
        mask *= smoothstep(0.0, 0.025, uv.y);
        return mask;
    }

    float3 FernWorldPosition(FernVertexInput input)
    {
        float3 worldPosition = mul(unity_ObjectToWorld, input.vertex).xyz;
        float2 rootLocal = (input.root - 0.5) * _WorldSize;
        float2 rootWorld = mul(
            unity_ObjectToWorld,
            float4(rootLocal.x, 0.0, rootLocal.y, 1.0)).xz;
        float3 wind = MotuGrassWindSample(rootWorld);
        float bend = input.uv.y * input.uv.y;
        float flexibility = lerp(0.45, 1.0, saturate(input.data.z));
        float strength = _GrassWindStrength * _FernWindMultiplier * wind.y * flexibility;
        worldPosition.xz += wind.xz * (strength * bend);
        worldPosition.y += sin(
            _Time.y * (_GrassWindSpeed * 1.7) + input.data.w * 6.283 + input.uv.y * 2.1)
            * strength * 0.12 * bend;
        return worldPosition;
    }

    float FernFade(float4 clipPosition, float distanceToPlayer)
    {
        float fade = 1.0 - smoothstep(_FernFadeStart, _FernFadeEnd, distanceToPlayer);
        float dither = frac(sin(dot(clipPosition.xy, float2(12.9898, 78.233))) * 43758.5453);
        return fade - dither;
    }

    FernVertexOutput FernVertex(FernVertexInput input)
    {
        FernVertexOutput output;
        UNITY_SETUP_INSTANCE_ID(input);
        UNITY_INITIALIZE_OUTPUT(FernVertexOutput, output);
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        output.worldPosition = FernWorldPosition(input);
        output.pos = UnityWorldToClipPos(output.worldPosition);
        output.worldNormal = UnityObjectToWorldNormal(input.normal);
        output.uv = input.uv;
        output.data = input.data;
        output.rootDistance = distance(output.worldPosition.xz, _GrassPlayerPosition.xz);
        UNITY_TRANSFER_FOG(output, output.pos);
        return output;
    }

    void ClipFern(FernVertexOutput input)
    {
        clip(FernSilhouette(input.uv, input.data) - _Cutoff);
        clip(FernFade(input.pos, input.rootDistance));
    }
    ENDCG

    SubShader
    {
        Tags { "RenderType"="MotuFernCutout" "Queue"="AlphaTest" "IgnoreProjector"="True" "MotuReflection"="Ferns" }
        Cull Off
        AlphaToMask On

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            ZWrite On

            CGPROGRAM
            #pragma vertex FernVertex
            #pragma fragment FernFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing

            fixed4 FernFragment(FernVertexOutput input, fixed facing : VFACE) : SV_Target
            {
                ClipFern(input);
                half3 normal = normalize(input.worldNormal) * (facing >= 0 ? 1.0h : -1.0h);
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half diffuse = saturate(dot(normal, lightDirection));
                fixed3 albedo = lerp(_BaseColor.rgb, _TipColor.rgb, input.uv.y * 0.72);
                albedo *= lerp(0.84, 1.16, input.data.y);
                half3 ambient = max(ShadeSH9(half4(normal, 1.0h)), 0.10h);
                fixed4 result = fixed4(
                    albedo * (ambient + _LightColor0.rgb * (0.22h + 0.78h * diffuse)),
                    1.0h);
                UNITY_APPLY_FOG(input.fogCoord, result);
                return result;
            }
            ENDCG
        }

        Pass
        {
            Tags { "LightMode"="ShadowCaster" }
            ZWrite On
            ZTest LEqual

            CGPROGRAM
            #pragma vertex ShadowVertex
            #pragma fragment ShadowFragment
            #pragma target 3.0
            #pragma multi_compile_shadowcaster

            FernShadowOutput ShadowVertex(FernVertexInput input)
            {
                FernShadowOutput output;
                float3 worldPosition = FernWorldPosition(input);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.pos = UnityApplyLinearShadowBias(
                    UnityWorldToClipPos(worldPosition + normal * unity_LightShadowBias.z));
                output.uv = input.uv;
                output.data = input.data;
                output.rootDistance = distance(worldPosition.xz, _GrassPlayerPosition.xz);
                return output;
            }

            float4 ShadowFragment(FernShadowOutput input) : SV_Target
            {
                clip(FernSilhouette(input.uv, input.data) - _Cutoff);
                clip(FernFade(input.pos, input.rootDistance));
                return 0;
            }
            ENDCG
        }
    }
    FallBack Off
}
