Shader "Motu/Water"
{
    Properties
    {
        _Color ("Color", Color) = (0.03, 0.28, 0.55, 1)
        [NoScaleOffset] _NoiseTex ("River Noise", 2D) = "black" {}
        _CoarseNoiseWorldSize ("Coarse Noise World Size", Float) = 6
        _FineNoiseWorldSize ("Fine Noise World Size", Float) = 3
        _CoarseFlowSpeed ("Coarse Flow Speed", Float) = 2.25
        _FineFlowSpeed ("Fine Flow Speed", Float) = 9
        _WorldSize ("World Size", Float) = 2000
        _ShallowOpacity ("Shallow Opacity", Range(0, 1)) = 0.25
        _OpacityDepth ("Full Opacity Depth", Float) = 5
        _ReflectionColor ("Sky Reflection", Color) = (0.49, 0.68, 0.82, 1)
        _ReflectionHorizonColor ("Horizon Reflection", Color) = (0.68, 0.79, 0.88, 1)
        _ReflectionStrength ("Reflection Strength", Range(0, 1)) = 0.5
        _ReflectionFresnelPower ("Reflection Fresnel Power", Range(1, 8)) = 4
        _SunGlintStrength ("Sun Glint Strength", Range(0, 2)) = 0.7
        _SunGlintSharpness ("Sun Glint Sharpness", Range(8, 256)) = 128
        _WhitewaterStrength ("Whitewater Strength", Range(0, 1)) = 0
        _WhitewaterSlopeStart ("Whitewater Slope Start", Range(0, 1)) = 0.05
        _WhitewaterSlopeFull ("Whitewater Slope Full", Range(0, 1)) = 0.55
    }
    SubShader
    {
        Tags { "Queue"="Transparent" "RenderType"="Transparent" }
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
            #include "UnityCG.cginc"
            #include "Lighting.cginc"

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
            };

            fixed4 _Color;
            sampler2D _NoiseTex;
            UNITY_DECLARE_DEPTH_TEXTURE(_CameraDepthTexture);
            float _CoarseNoiseWorldSize;
            float _FineNoiseWorldSize;
            float _CoarseFlowSpeed;
            float _FineFlowSpeed;
            float _WorldSize;
            half _ShallowOpacity;
            float _OpacityDepth;
            fixed4 _ReflectionColor;
            fixed4 _ReflectionHorizonColor;
            half _ReflectionStrength;
            half _ReflectionFresnelPower;
            half _SunGlintStrength;
            half _SunGlintSharpness;
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
                output.brightness = 0.72 + 0.28 * saturate(dot(normal, normalize(float3(0.3, 1.0, 0.2))));
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

                // Rust's vertical Z axis is Unity's Y axis after mesh import.
                float3 worldNormal = normalize(input.worldNormal);
                float3 viewDirection = normalize(_WorldSpaceCameraPos.xyz - input.worldPosition);
                worldNormal *= dot(worldNormal, viewDirection) >= 0.0 ? 1.0 : -1.0;
                half verticalAlignment = abs(worldNormal.y);
                half normalDeviation = 1.0h - verticalAlignment;
                half fineSlopeWhitewater = smoothstep(
                    _WhitewaterSlopeStart,
                    max(_WhitewaterSlopeFull, _WhitewaterSlopeStart + 0.001h),
                    normalDeviation);
                half coarseWhitewater = coarseFoam * 0.03h;
                half fineWhitewater = fineFoam * 0.50h * fineSlopeWhitewater;
                half layeredWhitewater = coarseWhitewater
                    + fineWhitewater * (1.0h - coarseWhitewater);
                half whitewater = saturate(layeredWhitewater * _WhitewaterStrength);
                float sceneDepth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE_PROJ(
                    _CameraDepthTexture,
                    UNITY_PROJ_COORD(input.screenPosition)));
                float waterDepth = max(sceneDepth - input.surfaceEyeDepth, 0.0);
                half depthOpacity = saturate(waterDepth / max(_OpacityDepth, 0.001));
                half waterOpacity = lerp(_ShallowOpacity, _Color.a, depthOpacity);
                float3 reflectionDirection = reflect(-viewDirection, worldNormal);
                half skyHeight = saturate(reflectionDirection.y);
                fixed3 skyReflection = lerp(
                    _ReflectionHorizonColor.rgb,
                    _ReflectionColor.rgb,
                    skyHeight);
                half fresnel = pow(
                    1.0h - saturate(dot(worldNormal, viewDirection)),
                    _ReflectionFresnelPower);
                half reflectionWeight = saturate(
                    _ReflectionStrength * lerp(0.08h, 1.0h, fresnel));
                fixed3 water = lerp(
                    _Color.rgb * input.brightness,
                    skyReflection,
                    reflectionWeight);
                half sunAlignment = saturate(dot(
                    reflectionDirection,
                    normalize(_WorldSpaceLightPos0.xyz)));
                half sunGlint = pow(sunAlignment, _SunGlintSharpness)
                    * _SunGlintStrength;
                water += _LightColor0.rgb * sunGlint;
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
