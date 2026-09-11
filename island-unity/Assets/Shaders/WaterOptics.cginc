#ifndef MOTU_WATER_OPTICS_INCLUDED
#define MOTU_WATER_OPTICS_INCLUDED

// Shared dielectric lighting and depth optics for sea and river surfaces.
float _SurfaceRoughness;
float4 _AbsorptionCoefficients;
float _AbsorptionStrength;
float _PlanarReflectionMaxMip;

float MotuWaterFresnel(float cosine)
{
    // Air/water dielectric reflectance for IOR approximately 1.333.
    return .02037 + .97963 * pow(1 - saturate(cosine), 5);
}

float MotuWaterSunSpecular(float3 normal, float3 view, float3 light, float roughness)
{
    float nv = max(dot(normal, view), .001);
    float nl = saturate(dot(normal, light));
    float3 halfVector = normalize(view + light + float3(0, .000001, 0));
    float nh = saturate(dot(normal, halfVector));
    float vh = saturate(dot(view, halfVector));
    float alpha = max(roughness * roughness, .006);
    float alpha2 = alpha * alpha;
    float denominator = nh * nh * (alpha2 - 1) + 1;
    float distribution = alpha2 / max(3.14159265 * denominator * denominator, 1.0e-10);
    float visibility = .5 / max(nl * sqrt(nv * nv * (1-alpha2) + alpha2)
        + nv * sqrt(nl * nl * (1-alpha2) + alpha2), .0001);
    return distribution * visibility * MotuWaterFresnel(vh) * nl;
}

float3 MotuWaterTransmission(float distanceMetres)
{
    return exp(-max(_AbsorptionCoefficients.rgb, 0) * max(_AbsorptionStrength, 0) * clamp(distanceMetres, 0, 1000));
}

float MotuWaterRefractionValidity(float sceneEyeDepth, float surfaceEyeDepth)
{
    // Fade before a distorted lookup reaches foreground geometry.
    return smoothstep(0, .15, sceneEyeDepth - surfaceEyeDepth);
}

float3 MotuWaterRefractDepthSafe(float4 grabPosition, float4 screenPosition, float surfaceEyeDepth,
    float originalDepth, float3 normal, float3 view, float2 ripple, out float pathLength)
{
    float2 grabUv = grabPosition.xy / max(grabPosition.w, .0001);
    float2 depthUv = screenPosition.xy / max(screenPosition.w, .0001);
    float2 viewNormal = mul((float3x3)UNITY_MATRIX_V, normal).xy;
    float depthWeight = smoothstep(.02, max(_RefractionDepth, .021), originalDepth);
    float2 offset = (ripple + viewNormal * .22) * _RefractionStrength * depthWeight
        * lerp(.25, 1, saturate(dot(normal, view)));
    // GrabPass and camera-depth textures can have opposite Y conventions.
    float2 depthOffset = offset;
    #if UNITY_UV_STARTS_AT_TOP
    depthOffset.y *= -_ProjectionParams.x;
    #endif
    float2 inset = abs(_MotuWaterBackground_TexelSize.xy) * 1.5;
    float2 candidateUv = depthUv + depthOffset;
    float inBounds = all(candidateUv >= inset) && all(candidateUv <= 1-inset);
    float candidateDepth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE(_CameraDepthTexture, saturate(candidateUv)));
    float validity = inBounds * MotuWaterRefractionValidity(candidateDepth, surfaceEyeDepth);
    // Validate the final interpolated location as well, not just the full offset.
    float2 finalDepthUv = depthUv + depthOffset * validity;
    float finalDepth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE(_CameraDepthTexture, saturate(finalDepthUv)));
    if (finalDepth <= surfaceEyeDepth + .001) { validity = 0; finalDepth = surfaceEyeDepth + originalDepth; }
    float2 finalGrabUv = clamp(grabUv + offset * validity, inset, 1-inset);
    float eyeDistance = max(finalDepth - surfaceEyeDepth, 0);
    float3 viewInCamera = mul((float3x3)UNITY_MATRIX_V, view);
    pathLength = min(1000, eyeDistance / max(abs(viewInCamera.z), .05));
    return tex2D(_MotuWaterBackground, finalGrabUv).rgb;
}

float3 MotuWaterShadeOptics(float3 body, float3 refracted, float pathLength, float3 normal,
    float3 view, float3 worldPosition, float2 ripple, float roughness, float shadow, MotuCloudLighting cloud, float planarWeight)
{
    float3 transmission = MotuWaterTransmission(pathLength);
    float3 volume = refracted * transmission + body * (1-transmission);
    float3 reflectionDirection = reflect(-view, normal);
    float3 sky = lerp(_ReflectionHorizonColor.rgb, _ReflectionColor.rgb, saturate(reflectionDirection.y)) * _WaterSkyExposure;
    float4 projected = mul(_PlanarReflectionMatrix, float4(worldPosition, 1));
    float2 uv = projected.xy / max(projected.w, .0001) + ripple * _PlanarReflectionDistortion;
    float inBounds = projected.w > .0001 && all(uv >= 0) && all(uv <= 1);
    float mip = roughness * max(_PlanarReflectionMaxMip, 0);
    float3 planar = tex2Dlod(_PlanarReflectionTexture, float4(saturate(uv), 0, mip)).rgb * lerp(.08, 1, _WaterSkyExposure);
    float weight = saturate(_PlanarReflectionAvailable * _PlanarReflectionWeight * planarWeight * inBounds);
    float3 reflection = lerp(sky, planar, weight) * lerp(.3, 1, shadow * cloud.ambientTransmittance);
    float fresnel = saturate(MotuWaterFresnel(dot(normal, view)) * _ReflectionStrength);
    float3 light = normalize(UnityWorldSpaceLightDir(worldPosition));
    float glint = MotuWaterSunSpecular(normal, view, light, roughness) * _SunGlintStrength
        * shadow * cloud.directTransmittance * _WaterSkyExposure;
    return volume * (1-fresnel) + reflection * fresnel + _LightColor0.rgb * glint;
}
#endif
