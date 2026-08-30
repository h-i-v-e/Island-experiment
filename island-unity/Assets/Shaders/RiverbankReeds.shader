Shader "Motu/Riverbank Reeds"
{
    Properties
    {
        _BaseColor ("Base Colour", Color) = (0.12, 0.24, 0.055, 1)
        _TipColor ("Tip Colour", Color) = (0.38, 0.48, 0.09, 1)
        _Cutoff ("Silhouette Cutoff", Range(0, 1)) = 0.46
        _ReedWindMultiplier ("Wind Multiplier", Range(0, 8)) = 3
        _ReedFadeStart ("LOD 0 Fade Start", Float) = 34
        _ReedFadeEnd ("LOD 0 Fade End", Float) = 47
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
    float _ReedWindMultiplier;
    float _ReedFadeStart;
    float _ReedFadeEnd;
    float4 _GrassPlayerPosition;

    #include "GrassWindCommon.cginc"

    struct ReedVertexInput
    {
        float4 vertex : POSITION;
        float3 normal : NORMAL;
        float2 uv : TEXCOORD0;
        float2 root : TEXCOORD1;
        float4 data : COLOR;
        UNITY_VERTEX_INPUT_INSTANCE_ID
    };

    struct ReedVertexOutput
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

    struct ReedShadowOutput
    {
        float4 pos : SV_POSITION;
        float2 uv : TEXCOORD0;
        half4 data : TEXCOORD1;
        float rootDistance : TEXCOORD2;
    };

    float Hash11(float value)
    {
        return frac(sin(value * 91.3458 + 17.13) * 47453.5453);
    }

    float ReedSilhouette(float2 uv, float4 data)
    {
        float silhouette = 0.0;
        [unroll]
        for (int stem = 0; stem < 7; stem++)
        {
            float seed = stem + data.w * 19.0 + data.y * 37.0 + data.x * 11.0;
            float centre = (stem + 0.5) / 7.0 + (Hash11(seed) - 0.5) * 0.075;
            // Keep enough unused card height for a complete tapered tip and
            // seed head. Reaching uv.y == 1 exposes the card's flat top edge.
            float stemHeight = lerp(0.58, 0.88, Hash11(seed + 2.7));
            stemHeight *= lerp(1.0, 0.78, data.x);
            float lean = (Hash11(seed + 7.1) - 0.5) * 0.12 * uv.y;
            float stemWidth = lerp(0.010, 0.025, Hash11(seed + 4.2));
            stemWidth *= lerp(0.55, 1.0, data.x);
            float heightProgress = saturate(uv.y / max(stemHeight, 0.01));
            float taper = lerp(
                1.0,
                0.03,
                smoothstep(0.55, 1.0, heightProgress));
            float stemMask = 1.0 - smoothstep(
                stemWidth * taper,
                stemWidth * taper + fwidth(uv.x) * 1.5,
                abs(uv.x - centre - lean));
            stemMask *= 1.0 - smoothstep(0.92, 1.0, heightProgress);
            stemMask *= 1.0 - smoothstep(stemHeight, stemHeight + fwidth(uv.y) * 2.0, uv.y);
            stemMask *= step(0.015, uv.y);
            silhouette = max(silhouette, stemMask);

            // Tall inner-bank reeds receive small seed heads; shorter outer
            // rushes remain blade-only.
            float2 headDelta = float2(
                (uv.x - centre - lean) / 0.035,
                (uv.y - stemHeight + 0.015) / 0.075);
            float head = 1.0 - smoothstep(0.70, 1.0, dot(headDelta, headDelta));
            silhouette = max(silhouette, head * (1.0 - data.x));
        }
        return silhouette;
    }

    float3 ReedWorldPosition(ReedVertexInput input)
    {
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
        return worldPosition;
    }

    ReedVertexOutput ReedVertex(ReedVertexInput input)
    {
        ReedVertexOutput output;
        UNITY_SETUP_INSTANCE_ID(input);
        UNITY_INITIALIZE_OUTPUT(ReedVertexOutput, output);
        UNITY_INITIALIZE_VERTEX_OUTPUT_STEREO(output);
        output.worldPosition = ReedWorldPosition(input);
        output.pos = UnityWorldToClipPos(output.worldPosition);
        output.worldNormal = UnityObjectToWorldNormal(input.normal);
        output.uv = input.uv;
        output.data = input.data;
        output.rootDistance = distance(output.worldPosition.xz, _GrassPlayerPosition.xz);
        UNITY_TRANSFER_FOG(output, output.pos);
        return output;
    }

    void ClipReed(ReedVertexOutput input)
    {
        float silhouette = ReedSilhouette(input.uv, input.data);
        float fade = 1.0 - smoothstep(_ReedFadeStart, _ReedFadeEnd, input.rootDistance);
        float dither = frac(sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
        clip(silhouette - _Cutoff);
        clip(fade - dither);
    }
    ENDCG

    SubShader
    {
        Tags { "RenderType"="MotuReedCutout" "Queue"="AlphaTest" "IgnoreProjector"="True" "MotuReflection"="Reeds" }
        Cull Off
        AlphaToMask On

        Pass
        {
            Tags { "LightMode"="ForwardBase" }
            ZWrite On

            CGPROGRAM
            #pragma vertex ReedVertex
            #pragma fragment ReedFragment
            #pragma target 3.0
            #pragma multi_compile_fwdbase
            #pragma multi_compile_fog
            #pragma multi_compile_instancing

            fixed4 ReedFragment(ReedVertexOutput input, fixed facing : VFACE) : SV_Target
            {
                ClipReed(input);
                half3 normal = normalize(input.worldNormal) * (facing >= 0 ? 1.0h : -1.0h);
                half3 lightDirection = normalize(UnityWorldSpaceLightDir(input.worldPosition));
                half attenuation = 1.0h;
                half diffuse = saturate(dot(normal, lightDirection));
                fixed3 albedo = lerp(_BaseColor.rgb, _TipColor.rgb, input.uv.y);
                albedo *= lerp(0.88, 1.12, input.data.y);
                half3 ambient = max(ShadeSH9(half4(normal, 1.0h)), 0.10h);
                fixed4 result = fixed4(
                    albedo * (ambient + _LightColor0.rgb * attenuation * (0.18h + 0.82h * diffuse)),
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

            ReedShadowOutput ShadowVertex(ReedVertexInput input)
            {
                ReedShadowOutput output;
                float3 worldPosition = ReedWorldPosition(input);
                float3 normal = UnityObjectToWorldNormal(input.normal);
                output.pos = UnityApplyLinearShadowBias(
                    UnityWorldToClipPos(worldPosition + normal * unity_LightShadowBias.z));
                output.uv = input.uv;
                output.data = input.data;
                output.rootDistance = distance(worldPosition.xz, _GrassPlayerPosition.xz);
                return output;
            }

            float4 ShadowFragment(ReedShadowOutput input) : SV_Target
            {
                float silhouette = ReedSilhouette(input.uv, input.data);
                float fade = 1.0 - smoothstep(
                    _ReedFadeStart,
                    _ReedFadeEnd,
                    input.rootDistance);
                float dither = frac(
                    sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
                clip(silhouette - _Cutoff);
                clip(fade - dither);
                return 0;
            }
            ENDCG
        }
    }
    FallBack Off
}
