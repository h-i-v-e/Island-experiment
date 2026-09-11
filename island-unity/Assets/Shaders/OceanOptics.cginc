#ifndef MOTU_OCEAN_OPTICS_INCLUDED
#define MOTU_OCEAN_OPTICS_INCLUDED

float _RippleStrength;
float _RippleWorldSize;
float _RippleSpeed;
float _SurfaceRoughness;
float _WindRoughness;
float4 _AbsorptionCoefficients;
float _AbsorptionStrength;
float _PlanarReflectionMaxMip;
sampler2D _OceanFoamHistory;
float4 _OceanFoamHistoryRect;
float _PersistentFoamStrength;

float MotuOceanHistoryFoam(float2 worldXZ)
{
    float2 uv = (worldXZ - _OceanFoamHistoryRect.xy) * _OceanFoamHistoryRect.zw;
    float edge = min(min(uv.x, uv.y), min(1-uv.x, 1-uv.y));
    return tex2D(_OceanFoamHistory, saturate(uv)).r
        * smoothstep(0, .04, edge) * max(_PersistentFoamStrength, 0);
}

float2 MotuOceanRippleSlope(float2 uv)
{
    // The weather texture has only four texels per noise cell. Start one mip
    // down to remove its tight cell corners before taking the height gradient.
    float2 dx = ddx(uv) * 256, dy = ddy(uv) * 256;
    float lod = max(1, .5 * log2(max(max(dot(dx, dx), dot(dy, dy)), 1)));
    const float stepUV = 2.0 / 256.0;
    return float2(
        tex2Dlod(_NoiseTex, float4(uv + float2(stepUV, 0), 0, lod)).r - tex2Dlod(_NoiseTex, float4(uv - float2(stepUV, 0), 0, lod)).r,
        tex2Dlod(_NoiseTex, float4(uv + float2(0, stepUV), 0, lod)).r - tex2Dlod(_NoiseTex, float4(uv - float2(0, stepUV), 0, lod)).r);
}

float3 MotuOceanRippleNormal(float3 worldPosition, float3 baseNormal, out float roughness)
{
    float windResponse = saturate(max(_MotuWeatherWind.z, 0) / 12);
    float strength = max(_RippleStrength, 0) * windResponse;
    float wavelength = max(_RippleWorldSize, .1);
    float footprint = max(length(ddx(worldPosition.xz)), length(ddy(worldPosition.xz)));
    float resolved = 1 - smoothstep(wavelength * .3, wavelength * 2, footprint);
    float3 normal = baseNormal;
    [branch]
    if (strength * resolved > .0001)
    {
        float2 wind = MotuWindDirection();
        float2 crossWind = float2(-wind.y, wind.x);
        float2 position = float2(dot(worldPosition.xz, wind), dot(worldPosition.xz, crossWind));
        float2 travel = float2(dot(_MotuWindOffset.xy, wind), dot(_MotuWindOffset.xy, crossWind))
            * max(_RippleSpeed, 0);
        float span = wavelength * 64;
        float2 uv = (position - travel * .12) / float2(span, span * 1.3);
        float2 coarse = MotuOceanRippleSlope(uv);
        // A weaker oblique band travels faster and slightly across the wind.
        // The two patterns continually change their combined shape instead of
        // translating as a single stamped texture. Use integrated wind travel
        // so weather speed changes do not restart the animation.
        const float c = .9063078, s = .4226183;
        float2 secondPosition = position - travel * .20 - float2(0, travel.x * .035);
        float2 fineUv = float2(c * secondPosition.x + s * secondPosition.y,
            -s * secondPosition.x + c * secondPosition.y) / (span * .62) + float2(.371, .619);
        float2 fine = MotuOceanRippleSlope(fineUv);
        fine = float2(c * fine.x - s * fine.y, s * fine.x + c * fine.y);
        float2 slope = (coarse + fine * .25) * strength * resolved * 1.5;
        float2 worldSlope = wind * slope.x + crossWind * slope.y;
        normal = normalize(baseNormal + float3(-worldSlope.x, 0, -worldSlope.y));
    }
    // Filter unresolved detail into the highlight lobe rather than shimmer.
    float variance = max(dot(ddx(normal), ddx(normal)), dot(ddy(normal), ddy(normal)));
    roughness = clamp(sqrt(pow(max(_SurfaceRoughness, .04) + windResponse * _WindRoughness, 2)
        + strength * strength * (1-resolved * resolved) * .5 + min(variance, .2)), .04, .8);
    return normal;
}

float MotuOceanFresnel(float cosine)
{
    // Air/water dielectric reflectance for IOR approximately 1.333.
    return .02037 + .97963 * pow(1 - saturate(cosine), 5);
}

float MotuOceanSunSpecular(float3 normal, float3 view, float3 light, float roughness)
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
    return distribution * visibility * MotuOceanFresnel(vh) * nl;
}

float3 MotuOceanTransmission(float distanceMetres)
{
    return exp(-max(_AbsorptionCoefficients.rgb, 0) * max(_AbsorptionStrength, 0) * clamp(distanceMetres, 0, 1000));
}

float MotuOceanRefractionValidity(float sceneEyeDepth, float surfaceEyeDepth)
{
    // Fade before a distorted lookup reaches foreground geometry.
    return smoothstep(0, .15, sceneEyeDepth - surfaceEyeDepth);
}

float3 MotuOceanRefract(float4 grabPosition, float4 screenPosition, float surfaceEyeDepth,
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
    float validity = inBounds * MotuOceanRefractionValidity(candidateDepth, surfaceEyeDepth);
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

float3 MotuOceanShade(float3 body, float3 refracted, float pathLength, float3 normal,
    float3 view, float3 worldPosition, float2 ripple, float roughness, float shadow, MotuCloudLighting cloud)
{
    float3 transmission = MotuOceanTransmission(pathLength);
    float3 volume = refracted * transmission + body * (1-transmission);
    float3 reflectionDirection = reflect(-view, normal);
    float3 sky = lerp(_ReflectionHorizonColor.rgb, _ReflectionColor.rgb, saturate(reflectionDirection.y)) * _WaterSkyExposure;
    float4 projected = mul(_PlanarReflectionMatrix, float4(worldPosition, 1));
    float2 uv = projected.xy / max(projected.w, .0001) + ripple * _PlanarReflectionDistortion;
    float inBounds = projected.w > .0001 && all(uv >= 0) && all(uv <= 1);
    float mip = roughness * max(_PlanarReflectionMaxMip, 0);
    float3 planar = tex2Dlod(_PlanarReflectionTexture, float4(saturate(uv), 0, mip)).rgb * lerp(.08, 1, _WaterSkyExposure);
    float weight = saturate(_PlanarReflectionAvailable * _PlanarReflectionWeight * inBounds);
    float3 reflection = lerp(sky, planar, weight) * lerp(.3, 1, shadow * cloud.ambientTransmittance);
    float fresnel = saturate(MotuOceanFresnel(dot(normal, view)) * _ReflectionStrength);
    float3 light = normalize(UnityWorldSpaceLightDir(worldPosition));
    float glint = MotuOceanSunSpecular(normal, view, light, roughness) * _SunGlintStrength
        * shadow * cloud.directTransmittance * _WaterSkyExposure;
    return volume * (1-fresnel) + reflection * fresnel + _LightColor0.rgb * glint;
}
#endif
