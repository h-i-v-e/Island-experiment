// Scalable ambient obscurance adapted from Unity's Post-processing Stack v2.
// See THIRD_PARTY_NOTICES.md for source and license details.
Shader "Hidden/Motu/Real-Time Ambient Occlusion"
{
    Properties
    {
        _MainTex ("Source", 2D) = "white" {}
    }

    SubShader
    {
        Tags { "RenderPipeline"="UniversalPipeline" }
        Cull Off
        ZWrite Off
        ZTest Always

        HLSLINCLUDE
        #include "MotuUrp.hlsl"

        #include "Packages/com.unity.render-pipelines.core/Runtime/Utilities/Blit.hlsl"

        sampler2D _CameraDepthNormalsTexture;
        UNITY_DECLARE_DEPTH_TEXTURE(_CameraDepthTexture);
        sampler2D _OcclusionTexture;
        float4 _OcclusionTexture_TexelSize;
        float4 _AOParams;

        #define INTENSITY _AOParams.x
        #define RADIUS _AOParams.y
        #define DOWNSAMPLE _AOParams.z
        #define SAMPLE_COUNT _AOParams.w

        static const float GeometryCoefficient = 0.8;
        static const float SelfOcclusionBias = 0.002;
        static const float Contrast = 0.6;

        half4 PackOcclusionNormal(half occlusion, half3 normal)
        {
            return half4(occlusion, normal * 0.5h + 0.5h);
        }

        half PackedOcclusion(half4 packed)
        {
            return packed.r;
        }

        half3 PackedNormal(half4 packed)
        {
            return packed.gba * 2.0h - 1.0h;
        }

        float CheckBounds(float2 uv, float rawDepth)
        {
            float outside = any(uv < 0.0) + any(uv > 1.0);
            #if defined(UNITY_REVERSED_Z)
            outside += rawDepth <= 0.00001;
            #else
            outside += rawDepth >= 0.99999;
            #endif
            return outside * 1.0e8;
        }

        float SampleEyeDepth(float2 uv)
        {
            float rawDepth = SAMPLE_DEPTH_TEXTURE(_CameraDepthTexture, uv);
            return Linear01Depth(rawDepth)
                * _ProjectionParams.z
                + CheckBounds(uv, rawDepth);
        }

        float3 SampleViewNormal(float2 uv)
        {
            float2 stepUv = rcp(_ScaledScreenParams.xy);
            float depth = SampleEyeDepth(uv);
            float left = SampleEyeDepth(uv - float2(stepUv.x, 0));
            float right = SampleEyeDepth(uv + float2(stepUv.x, 0));
            float down = SampleEyeDepth(uv - float2(0, stepUv.y));
            float up = SampleEyeDepth(uv + float2(0, stepUv.y));
            float2 diagonal = float2(unity_CameraProjection._m00, unity_CameraProjection._m11);
            float3 center = float3((uv * 2 - 1) / diagonal * depth, depth);
            float2 horizontal = float2(abs(right-depth) < abs(left-depth) ? stepUv.x : -stepUv.x, 0);
            float2 vertical = float2(0, abs(up-depth) < abs(down-depth) ? stepUv.y : -stepUv.y);
            float dx = horizontal.x > 0 ? right : left;
            float dy = vertical.y > 0 ? up : down;
            float3 a = float3(((uv+horizontal)*2-1) / diagonal * dx, dx) - center;
            float3 b = float3(((uv+vertical)*2-1) / diagonal * dy, dy) - center;
            return -normalize(cross(a,b)) * sign(horizontal.x * vertical.y);
        }

        half CompareNormal(half3 first, half3 second)
        {
            return smoothstep(GeometryCoefficient, 1.0h, dot(first, second));
        }

        float Random(float first, float second)
        {
            float seed = dot(
                float2(12.9898, 78.233),
                float2(first, second));
            return frac(43758.5453 * sin(seed));
        }

        float GradientNoise(float2 uv)
        {
            return frac(52.9829189 * frac(
                dot(uv, float2(0.06711056, 0.00583715))));
        }

        float PerspectiveScale(float value)
        {
            return lerp(value, 1.0, unity_OrthoParams.w);
        }

        float3 ReconstructViewPosition(
            float2 uv,
            float depth,
            float2 diagonal,
            float2 offset)
        {
            return float3(
                (uv * 2.0 - 1.0 - offset)
                    / diagonal
                    * PerspectiveScale(depth),
                depth);
        }

        float3 PickSamplePoint(float2 uv, float index)
        {
            float noise = GradientNoise(uv * DOWNSAMPLE);
            float vertical = frac(
                Random(0.0, index + uv.x * 1.0e-10) + noise)
                * 2.0
                - 1.0;
            float angle = (
                Random(1.0, index + uv.x * 1.0e-10) + noise)
                * 6.28318530718;
            float radial = sqrt(1.0 - vertical * vertical);
            float3 direction = float3(
                cos(angle) * radial,
                sin(angle) * radial,
                vertical);
            float distanceFromOrigin = sqrt((index + 1.0) / SAMPLE_COUNT) * RADIUS;
            return direction * distanceFromOrigin;
        }

        half4 EstimateOcclusion(Varyings input) : SV_Target
        {
            float2 diagonal = float2(
                unity_CameraProjection._m00,
                unity_CameraProjection._m11);
            float2 offset = float2(
                unity_CameraProjection._m02,
                unity_CameraProjection._m12);
            float centerDepth = SampleEyeDepth(input.texcoord);
            float3 centerNormal = SampleViewNormal(input.texcoord);
            float3 centerPosition = ReconstructViewPosition(
                input.texcoord,
                centerDepth,
                diagonal,
                offset);
            float occlusion = 0.0;

            [loop]
            for (int sampleIndex = 0; sampleIndex < 12; ++sampleIndex)
            {
                if (sampleIndex >= (int)SAMPLE_COUNT)
                {
                    break;
                }

                float3 sampleVector = PickSamplePoint(input.texcoord, sampleIndex);
                sampleVector = faceforward(
                    sampleVector,
                    -centerNormal,
                    sampleVector);
                float3 projectedSamplePosition = centerPosition + sampleVector;
                float3 projectedSample = mul(
                    (float3x3)unity_CameraProjection,
                    projectedSamplePosition);
                float2 sampleUv = (
                    projectedSample.xy
                        / PerspectiveScale(projectedSamplePosition.z)
                        + 1.0)
                    * 0.5;
                float sampleDepth = SampleEyeDepth(sampleUv);
                float3 visibleSamplePosition = ReconstructViewPosition(
                    sampleUv,
                    sampleDepth,
                    diagonal,
                    offset);
                float3 difference = visibleSamplePosition - centerPosition;
                float numerator = max(
                    dot(difference, centerNormal)
                        - SelfOcclusionBias * centerDepth,
                    0.0);
                occlusion += numerator
                    / (dot(difference, difference) + 1.0e-4);
            }

            occlusion *= RADIUS;
            occlusion = pow(
                max(occlusion * INTENSITY / SAMPLE_COUNT, 1.0e-7),
                Contrast);
            return PackOcclusionNormal(occlusion, centerNormal);
        }

        half4 BlurHorizontal(Varyings input) : SV_Target
        {
            float2 delta = float2(_BlitTexture_TexelSize.x * 2.0, 0.0);
            half4 center = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord);
            half4 nearA = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord - delta * 1.3846153846);
            half4 nearB = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord + delta * 1.3846153846);
            half4 farA = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord - delta * 3.2307692308);
            half4 farB = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord + delta * 3.2307692308);
            half3 normal = SampleViewNormal(input.texcoord);
            half centerWeight = 0.2270270270h;
            half nearAWeight = CompareNormal(normal, PackedNormal(nearA)) * 0.3162162162h;
            half nearBWeight = CompareNormal(normal, PackedNormal(nearB)) * 0.3162162162h;
            half farAWeight = CompareNormal(normal, PackedNormal(farA)) * 0.0702702703h;
            half farBWeight = CompareNormal(normal, PackedNormal(farB)) * 0.0702702703h;
            half total = PackedOcclusion(center) * centerWeight
                + PackedOcclusion(nearA) * nearAWeight
                + PackedOcclusion(nearB) * nearBWeight
                + PackedOcclusion(farA) * farAWeight
                + PackedOcclusion(farB) * farBWeight;
            half weight = centerWeight
                + nearAWeight
                + nearBWeight
                + farAWeight
                + farBWeight;
            return PackOcclusionNormal(total / weight, normal);
        }

        half4 BlurVertical(Varyings input) : SV_Target
        {
            float2 delta = float2(
                0.0,
                _BlitTexture_TexelSize.y / DOWNSAMPLE * 2.0);
            half4 center = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord);
            half4 nearA = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord - delta * 1.3846153846);
            half4 nearB = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord + delta * 1.3846153846);
            half4 farA = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord - delta * 3.2307692308);
            half4 farB = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord + delta * 3.2307692308);
            half3 normal = PackedNormal(center);
            half centerWeight = 0.2270270270h;
            half nearAWeight = CompareNormal(normal, PackedNormal(nearA)) * 0.3162162162h;
            half nearBWeight = CompareNormal(normal, PackedNormal(nearB)) * 0.3162162162h;
            half farAWeight = CompareNormal(normal, PackedNormal(farA)) * 0.0702702703h;
            half farBWeight = CompareNormal(normal, PackedNormal(farB)) * 0.0702702703h;
            half total = PackedOcclusion(center) * centerWeight
                + PackedOcclusion(nearA) * nearAWeight
                + PackedOcclusion(nearB) * nearBWeight
                + PackedOcclusion(farA) * farAWeight
                + PackedOcclusion(farB) * farBWeight;
            half weight = centerWeight
                + nearAWeight
                + nearBWeight
                + farAWeight
                + farBWeight;
            return PackOcclusionNormal(total / weight, normal);
        }

        half4 Composite(Varyings input) : SV_Target
        {
            fixed4 source = SAMPLE_TEXTURE2D_X(_BlitTexture, sampler_LinearClamp, input.texcoord);
            // The bilateral upsample can reach neighbouring geometry at the skyline.
            // Empty depth must remain untouched even when those neighbours are occluded.
            float rawDepth = SAMPLE_DEPTH_TEXTURE(_CameraDepthTexture, input.texcoord);
            #if UNITY_REVERSED_Z
            if (rawDepth <= 0.00001) return source;
            #else
            if (rawDepth >= 0.99999) return source;
            #endif
            float2 delta = _OcclusionTexture_TexelSize.xy / DOWNSAMPLE;
            half4 center = tex2D(_OcclusionTexture, input.texcoord);
            half3 normal = PackedNormal(center);
            half total = PackedOcclusion(center);
            half weight = 1.0h;

            half4 corner = tex2D(_OcclusionTexture, input.texcoord + delta);
            half cornerWeight = CompareNormal(normal, PackedNormal(corner));
            total += PackedOcclusion(corner) * cornerWeight;
            weight += cornerWeight;
            corner = tex2D(_OcclusionTexture, input.texcoord - delta);
            cornerWeight = CompareNormal(normal, PackedNormal(corner));
            total += PackedOcclusion(corner) * cornerWeight;
            weight += cornerWeight;
            corner = tex2D(_OcclusionTexture, input.texcoord + float2(delta.x, -delta.y));
            cornerWeight = CompareNormal(normal, PackedNormal(corner));
            total += PackedOcclusion(corner) * cornerWeight;
            weight += cornerWeight;
            corner = tex2D(_OcclusionTexture, input.texcoord + float2(-delta.x, delta.y));
            cornerWeight = CompareNormal(normal, PackedNormal(corner));
            total += PackedOcclusion(corner) * cornerWeight;
            weight += cornerWeight;

            half occlusion = saturate(total / weight);
            source.rgb *= 1.0h - occlusion;
            return source;
        }
        ENDHLSL

        Pass
        {
            Name "Occlusion Estimation"
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment EstimateOcclusion
            #pragma target 3.0
            ENDHLSL
        }

        Pass
        {
            Name "Horizontal Bilateral Blur"
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment BlurHorizontal
            #pragma target 3.0
            ENDHLSL
        }

        Pass
        {
            Name "Vertical Bilateral Blur"
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment BlurVertical
            #pragma target 3.0
            ENDHLSL
        }

        Pass
        {
            Name "Composite"
            HLSLPROGRAM
            #pragma vertex Vert
            #pragma fragment Composite
            #pragma target 3.0
            ENDHLSL
        }
    }

    FallBack Off
}
