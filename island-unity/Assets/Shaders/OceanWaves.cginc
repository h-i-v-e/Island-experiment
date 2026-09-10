#ifndef MOTU_OCEAN_WAVES_INCLUDED
#define MOTU_OCEAN_WAVES_INCLUDED

#include "WeatherWindCommon.cginc"
#include "SeaMaskCommon.cginc"

// Query shaders may sample neighbouring times. Rendering keeps the exact
// integrated phases and wind offset; these hooks compile away in surface passes.
#ifndef MOTU_OCEAN_SAMPLE_PATTERN
#define MOTU_OCEAN_SAMPLE_PATTERN(pattern, speed) pattern
#endif
#ifndef MOTU_OCEAN_SAMPLE_ONSHORE_PHASE
#define MOTU_OCEAN_SAMPLE_ONSHORE_PHASE _OnshoreWavePhase
#endif
#ifndef MOTU_OCEAN_SAMPLE_WIND_POSITION
#define MOTU_OCEAN_SAMPLE_WIND_POSITION(position) MotuWindAdvectedPosition(position)
#endif

sampler2D _WaveAttenuationTex;
sampler2D _WaveOnshoreTex;
sampler2D _NoiseTex;
float4 _WaveAttenuationWorldRect;
float4 _OceanWave0;
float4 _OceanWave1;
float4 _OceanWave2;
float4 _OceanWave3;
float4 _OceanWaveSpeeds;
// XY direction and Z wavelength are fixed during a transition; W is the
// CPU-integrated phase in radians. Both normals and geometry use these banks.
float4 _OceanWaveFrom0;
float4 _OceanWaveFrom1;
float4 _OceanWaveFrom2;
float4 _OceanWaveFrom3;
float4 _OceanWaveTo0;
float4 _OceanWaveTo1;
float4 _OceanWaveTo2;
float4 _OceanWaveTo3;
float _OceanWaveTransition;
float _OnshoreWavePhase;
float4 _OceanFoamTravel;
float4 _OceanWaveChoppiness;
float _GeometricWaves;
float _WaveFadeStart;
float _WaveFadeEnd;
float _WaveNoiseWorldSize;
float _WaveDomainWarp;
float _WaveAmplitudeVariation;
float4 _WhitecapColour;
float _WhitecapStrength;
float _WhitecapHeightThreshold;
float _WhitecapSlopeThreshold;
float _WhitecapCoverage;
float _WhitecapNoiseWorldSize;
float _WhitecapDistortionScale;
float _WhitecapDistortionStrength;
float _WhitecapDistortionSpeed;
float _OnshoreWaveEnabled;
float4 _OnshoreWaveParameters;
float4 _OnshoreWaveBreaking;

float4 MotuOceanCoastalData(float2 worldPosition)
{
    float2 uv = (worldPosition - _WaveAttenuationWorldRect.xy)
        * _WaveAttenuationWorldRect.zw;
    float inside = step(0.0, uv.x)
        * step(0.0, uv.y)
        * step(uv.x, 1.0)
        * step(uv.y, 1.0);
    float4 coastalData = tex2Dlod(
        _WaveAttenuationTex,
        float4(saturate(uv), 0.0, 0.0));
    return lerp(float4(1.0, 1.0, 1.0, 1.0), coastalData, inside);
}

float MotuOceanWaveAttenuation(float2 worldPosition)
{
    return MotuOceanCoastalData(worldPosition).r;
}

float MotuOceanMaximumWaveHeight()
{
    float directionalAmplitude = (
        max(_OceanWave0.w, 0.0)
        + max(_OceanWave1.w, 0.0)
        + max(_OceanWave2.w, 0.0)
        + max(_OceanWave3.w, 0.0))
        * (1.0 + saturate(_WaveAmplitudeVariation));
    float onshoreAmplitude = max(_OnshoreWaveParameters.y, 0.0)
        * saturate(_OnshoreWaveEnabled);
    float weatherScale = max(_MotuWeatherWind.w, 0.0);
    float maximumChoppiness = max(
        max(_OceanWaveChoppiness.x, _OceanWaveChoppiness.y),
        max(_OceanWaveChoppiness.z, _OceanWaveChoppiness.w));
    maximumChoppiness = max(
        maximumChoppiness,
        saturate(_OnshoreWaveParameters.w));
    return max(
        (directionalAmplitude + onshoreAmplitude) * weatherScale
            * (1.0 + 0.22 * saturate(maximumChoppiness)),
        0.001);
}

float MotuOceanDepthWaveScale(float4 coastalData)
{
    // The depth channel covers 0-5 m. Retain this conservative limit in
    // deeper water so large troughs cannot expose the seabed near shore.
    float waterDepth = saturate(coastalData.b) * MotuSeaMaskDepthMetres;
    return saturate(waterDepth / MotuOceanMaximumWaveHeight());
}

float MotuOceanClampHeightEnvelope(float2 worldPosition)
{
    // Use the envelope of the rendered, depth-limited waves. Using the raw
    // weather amplitude here compresses the mask transition into a steep rim.
    float waterDepth = saturate(MotuOceanCoastalData(worldPosition).b) * MotuSeaMaskDepthMetres;
    return min(MotuOceanMaximumWaveHeight(), waterDepth);
}

void MotuOceanOnshoreField(
    float2 worldPosition,
    out float2 onshoreDirection,
    out float influence,
    out float coastalCoordinate)
{
    float2 uv = (worldPosition - _WaveAttenuationWorldRect.xy)
        * _WaveAttenuationWorldRect.zw;
    float inside = step(0.0, uv.x)
        * step(0.0, uv.y)
        * step(uv.x, 1.0)
        * step(uv.y, 1.0);
    float4 packedField = tex2Dlod(
        _WaveOnshoreTex,
        float4(saturate(uv), 0.0, 0.0));
    onshoreDirection = normalize(
        packedField.rg * 2.0 - 1.0 + float2(1.0e-6, 0.0));
    influence = packedField.b
        * inside
        * saturate(_OnshoreWaveEnabled);
    coastalCoordinate = packedField.a;
}

void MotuEvaluateOnshoreBreakerShape(
    float phase,
    float coastDistance,
    float wavelength,
    float waterDepth,
    out float height,
    out float distanceDerivative,
    out float breakerFoam,
    out float crestCurvature)
{
    static const float Pi = 3.14159265359;
    static const float TwoPi = 6.28318530718;
    static const float MinimumLeadingFraction = 0.04;
    float maximumSharpness = saturate(_OnshoreWaveBreaking.x);
    float sharpeningDistance = max(_OnshoreWaveBreaking.y, 0.25);
    // Distance only bounds where coastal breakers can form. Actual depth
    // controls the leading face as the same depth map reduces wave height.
    float bandStart = sharpeningDistance * 0.8;
    float bandWidth = max(sharpeningDistance - bandStart, 0.001);
    float distanceProgress = saturate((coastDistance - bandStart) / bandWidth);
    float coastalBand = 1.0 - smoothstep(bandStart, sharpeningDistance, coastDistance);
    float startDepth = clamp(_OnshoreWaveBreaking.z, 0.01, MotuSeaMaskDepthMetres);
    float fullDepth = clamp(_OnshoreWaveBreaking.w, 0.0, startDepth - 0.01);
    float shallowBreaking = 1.0 - smoothstep(fullDepth, startDepth, waterDepth);
    float shoreProximity = shallowBreaking * coastalBand;
    float proximityDerivative = -shallowBreaking * 6.0
        * distanceProgress * (1.0 - distanceProgress) / bandWidth;

    // At 0.5 the two faces reproduce an ordinary sine wave. Moving only the
    // leading fraction toward zero compresses the shore-facing rise while the
    // rear face expands to retain the complete wavelength.
    float leadingFraction = lerp(
        0.5,
        MinimumLeadingFraction,
        maximumSharpness * shoreProximity);
    float proximitySecondDerivative = -shallowBreaking * 6.0
        * (1.0 - 2.0 * distanceProgress) / (bandWidth * bandWidth)
        * step(bandStart, coastDistance) * step(coastDistance, sharpeningDistance);
    float leadingFractionSecondDerivative = (MinimumLeadingFraction - 0.5)
        * maximumSharpness * proximitySecondDerivative;
    float leadingFractionDerivative = (MinimumLeadingFraction - 0.5)
        * maximumSharpness
        * proximityDerivative;
    float cycle = frac(phase / TwoPi + 0.25);
    float cycleDerivative = 1.0 / max(wavelength, 1.0);
    float breakingStrength = maximumSharpness * shoreProximity;

    [flatten]
    if (cycle < leadingFraction)
    {
        float leadingProgress = cycle / leadingFraction;
        float angle = Pi * cycle / leadingFraction;
        float angleDerivative = Pi
            * (cycleDerivative * leadingFraction
                - cycle * leadingFractionDerivative)
            / (leadingFraction * leadingFraction);
        float angleSecondDerivative = Pi * (
            -cycle * leadingFractionSecondDerivative / (leadingFraction * leadingFraction)
            - 2.0 * angleDerivative / Pi * leadingFractionDerivative / leadingFraction);
        crestCurvature = -cos(angle) * angleDerivative * angleDerivative
            - sin(angle) * angleSecondDerivative;
        height = -cos(angle);
        distanceDerivative = sin(angle) * angleDerivative;
        breakerFoam = breakingStrength
            * smoothstep(0.55, 0.88, leadingProgress);
        return;
    }

    float rearFraction = 1.0 - leadingFraction;
    float rearProgress = (cycle - leadingFraction) / rearFraction;
    float rearProgressDerivative = (
        cycleDerivative * rearFraction
        + leadingFractionDerivative * (cycle - 1.0))
        / (rearFraction * rearFraction);
    float rearAngle = Pi * rearProgress;
    float rearProgressSecondDerivative = leadingFractionSecondDerivative
        * (cycle - 1.0) / (rearFraction * rearFraction)
        + 2.0 * rearProgressDerivative * leadingFractionDerivative / rearFraction;
    float rearAngleDerivative = Pi * rearProgressDerivative;
    crestCurvature = cos(rearAngle) * rearAngleDerivative * rearAngleDerivative
        + sin(rearAngle) * Pi * rearProgressSecondDerivative;
    height = cos(rearAngle);
    distanceDerivative = -sin(rearAngle)
        * Pi
        * rearProgressDerivative;
    breakerFoam = breakingStrength
        * (1.0 - smoothstep(0.0, 0.12, rearProgress));
}

// Existing geometry/foam callers discard curvature; shader optimization removes
// the extra derivative work when it is not consumed.
void MotuEvaluateOnshoreBreakerShape(float phase, float coastDistance, float wavelength,
    float waterDepth, out float height, out float distanceDerivative, out float breakerFoam)
{
    float unusedCurvature;
    MotuEvaluateOnshoreBreakerShape(phase, coastDistance, wavelength, waterDepth,
        height, distanceDerivative, breakerFoam, unusedCurvature);
}

void MotuAccumulateOnshoreWave(
    float2 onshoreDirection,
    float influence,
    float coastalCoordinate,
    float waterDepth,
    inout float3 displacement,
    inout float2 heightDerivative,
    out float breakerFoam,
    out float crestCurvature)
{
    float wavelength = max(_OnshoreWaveParameters.x, 1.0);
    float amplitude = max(_OnshoreWaveParameters.y, 0.0)
        * max(influence, 0.0)
        * max(_MotuWeatherWind.w, 0.0);
    float choppiness = saturate(_OnshoreWaveParameters.w);
    float waveNumber = 6.28318530718 / wavelength;

    // Decode linear distance so wavelength stays in metres as the coastal band
    // grows. Positive time travels towards coordinate 0 around bays/headlands.
    float coastDistance = saturate(coastalCoordinate) * MotuSeaMaskLandDistanceMetres;
    float phase = coastDistance * waveNumber + MOTU_OCEAN_SAMPLE_ONSHORE_PHASE;
    float waveSin;
    float waveCos;
    sincos(phase, waveSin, waveCos);
    float breakerHeight;
    float breakerDistanceDerivative;
    MotuEvaluateOnshoreBreakerShape(
        phase,
        coastDistance,
        wavelength,
        waterDepth,
        breakerHeight,
        breakerDistanceDerivative,
        breakerFoam,
        crestCurvature);
    // Foam follows breaking and wetness, not the depth-limited wave height.
    // Keep it on an existing incoming wave and fade the last film of water.
    breakerFoam *= smoothstep(0.0, 0.1, amplitude)
        * smoothstep(0.02, 0.15, waterDepth);
    float waveSinDouble = 2.0 * waveSin * waveCos;
    float waveCosDouble = waveCos * waveCos - waveSin * waveSin;
    float crestBias = choppiness * 0.22;
    crestCurvature = amplitude * (crestCurvature
        - 4.0 * crestBias * waveNumber * waveNumber * waveCosDouble);
    displacement.y += amplitude
        * (breakerHeight - crestBias * waveCosDouble);
    displacement.xz += onshoreDirection
        * (amplitude * choppiness * waveCos);
    heightDerivative += onshoreDirection
        * (amplitude
            * (breakerDistanceDerivative
                + 2.0 * crestBias * waveNumber * waveSinDouble));
}

void MotuAccumulateOceanWave(
    float4 pattern,
    float baseAmplitude,
    float choppiness,
    float amplitudeScale,
    float contributionWeight,
    float2 worldPosition,
    inout float3 displacement,
    inout float2 heightDerivative,
    inout float crestCurvature)
{
    float2 direction = pattern.xy;
    float wavelength = max(pattern.z, 0.25);
    float weatherScale = max(_MotuWeatherWind.w, 0.0);
    float amplitude = max(baseAmplitude, 0.0)
        * max(amplitudeScale, 0.0)
        * weatherScale;
    float waveNumber = 6.28318530718 / wavelength;
    float phase = dot(direction, worldPosition) * waveNumber + pattern.w;
    float waveSin;
    float waveCos;
    sincos(phase, waveSin, waveCos);
    float waveSinDouble = 2.0 * waveSin * waveCos;
    float waveCosDouble = waveCos * waveCos - waveSin * waveSin;
    float crestBias = saturate(choppiness) * 0.22;
    // Negative second height derivative: convex crests positive, troughs
    // negative. Reuse the exact phase, amplitude variation and crest bias.
    // Like the existing slope, treat the noise amplitude/warp as locally constant.
    float crestShape = waveSin - 4.0 * crestBias * waveCosDouble;
    amplitude *= contributionWeight;
    crestCurvature += amplitude * waveNumber * waveNumber * crestShape;
    displacement.y += amplitude * (waveSin - crestBias * waveCosDouble);
    displacement.xz += direction
        * (amplitude * saturate(choppiness) * waveCos);
    heightDerivative += direction
        * (amplitude * waveNumber
            * (waveCos + 2.0 * crestBias * waveSinDouble));
}

void MotuAccumulateOceanWaveTransition(
    float4 outgoingPattern,
    float4 incomingPattern,
    float baseAmplitude,
    float choppiness,
    float amplitudeScale,
    float2 worldPosition,
    inout float3 displacement,
    inout float2 heightDerivative,
    inout float crestCurvature)
{
    float blend = saturate(_OceanWaveTransition);
    MotuAccumulateOceanWave(
        outgoingPattern, baseAmplitude, choppiness,
        amplitudeScale, 1.0 - blend, worldPosition,
        displacement, heightDerivative, crestCurvature);
    [branch]
    if (blend > 0.0)
    {
        MotuAccumulateOceanWave(
            incomingPattern, baseAmplitude, choppiness,
            amplitudeScale, blend, worldPosition,
            displacement, heightDerivative, crestCurvature);
    }
}

float MotuOceanWaveAmplitudeScale(
    float2 windAdvectedPosition,
    float wavelength,
    float longestWavelength,
    float2 noiseOffset)
{
    // Keep the same number of noise patches per crest spacing in every band:
    // broad swell gets broad height variation, short waves get finer variation.
    // Sample the unwarped world field so moving/recentering the mesh cannot
    // move the patches, and use offsets to decorrelate the four wave bands.
    float noiseWorldSize = max(_WaveNoiseWorldSize, 256.0)
        * max(wavelength, 0.25) / longestWavelength;
    float noise = tex2Dlod(
        _MotuWindNoise,
        float4(windAdvectedPosition / noiseWorldSize + noiseOffset, 0.0, 0.0)).r;
    return 1.0 + (noise * 2.0 - 1.0) * saturate(_WaveAmplitudeVariation);
}

void MotuEvaluateOceanWaveField(
    float2 worldPosition,
    out float3 displacement,
    out float2 heightDerivative,
    out float crestCurvature)
{
    crestCurvature = 0.0;
    displacement = 0.0;
    heightDerivative = 0.0;
    float noiseWorldSize = max(_WaveNoiseWorldSize, 256.0);
    float2 windAdvectedPosition = MOTU_OCEAN_SAMPLE_WIND_POSITION(worldPosition);
    float2 broadUv = windAdvectedPosition / noiseWorldSize;
    float2 detailUv = windAdvectedPosition / (noiseWorldSize * 0.37)
        + float2(0.371, 0.619);
    float2 broadNoise = tex2Dlod(
        _MotuWindNoise,
        float4(broadUv, 0.0, 0.0)).rg;
    float2 detailNoise = tex2Dlod(
        _MotuWindNoise,
        float4(detailUv, 0.0, 0.0)).gr;
    float2 domainWarp = ((broadNoise - 0.5) * 1.35
        + (detailNoise - 0.5) * 0.45)
        * max(_WaveDomainWarp, 0.0);
    float longestWavelength = max(
        max(max(_OceanWave0.z, _OceanWave1.z),
            max(_OceanWave2.z, _OceanWave3.z)),
        0.25);
    float4 amplitudeScales = float4(
        MotuOceanWaveAmplitudeScale(
            windAdvectedPosition, _OceanWave0.z, longestWavelength,
            float2(0.0, 0.0)),
        MotuOceanWaveAmplitudeScale(
            windAdvectedPosition, _OceanWave1.z, longestWavelength,
            float2(0.173, 0.419)),
        MotuOceanWaveAmplitudeScale(
            windAdvectedPosition, _OceanWave2.z, longestWavelength,
            float2(0.371, 0.619)),
        MotuOceanWaveAmplitudeScale(
            windAdvectedPosition, _OceanWave3.z, longestWavelength,
            float2(0.733, 0.281)));

    MotuAccumulateOceanWaveTransition(
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveFrom0, _OceanWaveSpeeds.x),
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveTo0, _OceanWaveSpeeds.x),
        _OceanWave0.w,
        _OceanWaveChoppiness.x,
        amplitudeScales.x,
        worldPosition + domainWarp,
        displacement,
        heightDerivative,
        crestCurvature);
    MotuAccumulateOceanWaveTransition(
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveFrom1, _OceanWaveSpeeds.y),
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveTo1, _OceanWaveSpeeds.y),
        _OceanWave1.w,
        _OceanWaveChoppiness.y,
        amplitudeScales.y,
        worldPosition + float2(-domainWarp.y, domainWarp.x) * 0.82,
        displacement,
        heightDerivative,
        crestCurvature);
    MotuAccumulateOceanWaveTransition(
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveFrom2, _OceanWaveSpeeds.z),
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveTo2, _OceanWaveSpeeds.z),
        _OceanWave2.w,
        _OceanWaveChoppiness.z,
        amplitudeScales.z,
        worldPosition - domainWarp * 0.61,
        displacement,
        heightDerivative,
        crestCurvature);
    MotuAccumulateOceanWaveTransition(
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveFrom3, _OceanWaveSpeeds.w),
        MOTU_OCEAN_SAMPLE_PATTERN(_OceanWaveTo3, _OceanWaveSpeeds.w),
        _OceanWave3.w,
        _OceanWaveChoppiness.w,
        amplitudeScales.w,
        worldPosition + float2(domainWarp.y, -domainWarp.x) * 1.13,
        displacement,
        heightDerivative,
        crestCurvature);

}

void MotuEvaluateOceanWaveField(float2 worldPosition, out float3 displacement,
    out float2 heightDerivative)
{
    float unusedCurvature;
    MotuEvaluateOceanWaveField(worldPosition, displacement, heightDerivative, unusedCurvature);
}

void MotuEvaluateOnshoreWaveField(
    float2 worldPosition,
    out float3 displacement,
    out float2 heightDerivative,
    out float influence,
    out float breakerFoam,
    out float crestCurvature)
{
    displacement = 0.0;
    heightDerivative = 0.0;
    influence = 0.0;
    breakerFoam = 0.0;
    crestCurvature = 0.0;
    [branch]
    if (_OnshoreWaveEnabled <= 0.0001)
    {
        return;
    }

    float2 onshoreDirection;
    float coastalCoordinate;
    MotuOceanOnshoreField(
        worldPosition,
        onshoreDirection,
        influence,
        coastalCoordinate);
    [branch]
    if (influence <= 0.0001)
    {
        return;
    }
    MotuAccumulateOnshoreWave(
        onshoreDirection,
        influence,
        coastalCoordinate,
        saturate(MotuOceanCoastalData(worldPosition).b) * MotuSeaMaskDepthMetres,
        displacement,
        heightDerivative,
        breakerFoam,
        crestCurvature);
}

void MotuEvaluateOnshoreWaveField(float2 worldPosition, out float3 displacement,
    out float2 heightDerivative, out float influence, out float breakerFoam)
{
    float unusedCurvature;
    MotuEvaluateOnshoreWaveField(worldPosition, displacement, heightDerivative,
        influence, breakerFoam, unusedCurvature);
}

void MotuEvaluateOceanWaveDisplacement(
    float2 worldPosition,
    float localRadius,
    out float3 displacement)
{
    displacement = 0.0;
    float distanceFade = 1.0 - smoothstep(
        _WaveFadeStart,
        max(_WaveFadeEnd, _WaveFadeStart + 0.001),
        localRadius);
    float geometryWeight = distanceFade * saturate(_GeometricWaves);
    [branch]
    if (geometryWeight <= 0.0001)
    {
        return;
    }
    float4 coastalData = MotuOceanCoastalData(worldPosition);
    float combinedAttenuation = coastalData.r;
    float2 unusedBaseDerivative;
    MotuEvaluateOceanWaveField(
        worldPosition,
        displacement,
        unusedBaseDerivative);
    displacement *= combinedAttenuation;

    float3 onshoreDisplacement;
    float2 unusedOnshoreDerivative;
    float unusedOnshoreInfluence;
    float unusedBreakerFoamSlope;
    MotuEvaluateOnshoreWaveField(
        worldPosition,
        onshoreDisplacement,
        unusedOnshoreDerivative,
        unusedOnshoreInfluence,
        unusedBreakerFoamSlope);
    float depthWaveScale = MotuOceanDepthWaveScale(coastalData);
    displacement = (displacement + onshoreDisplacement)
        * geometryWeight
        * depthWaveScale;
}

float MotuOceanLargestWaveHeight()
{
    float4 amplitudes = max(float4(_OceanWave0.w, _OceanWave1.w,
        _OceanWave2.w, _OceanWave3.w), 0.0);
    float4 peaks = amplitudes * (1.0 + 0.22 * saturate(_OceanWaveChoppiness))
        * (1.0 + saturate(_WaveAmplitudeVariation));
    float largest = max(max(peaks.x, peaks.y), max(peaks.z, peaks.w));
    float shorePeak = max(_OnshoreWaveParameters.y, 0.0)
        * (1.0 + 0.22 * saturate(_OnshoreWaveParameters.w))
        * saturate(_OnshoreWaveEnabled);
    return max(largest, shorePeak) * max(_MotuWeatherWind.w, 0.0);
}

float MotuOceanHeightTranslucency(float heightAboveSeaPlane)
{
    float maximumHeight = MotuOceanLargestWaveHeight();
    if (maximumHeight <= 0.0001) return 0.0;
    float height = saturate(heightAboveSeaPlane / maximumHeight);
    // Mostly proportional to actual height, with a flat tangent at sea level
    // to avoid bringing back a sharp border between lit crests and dark troughs.
    return height * smoothstep(0.0, 0.15, height);
}

float2 MotuFoamCellSeed(float2 cell)
{
    float3 seed = frac(float3(cell.x, cell.y, cell.x) * 0.1031);
    seed += dot(seed, seed.yzx + 33.33);
    return frac((seed.xx + seed.yz) * seed.zy);
}

float2 MotuOceanFoamUv(float2 worldPosition)
{
    float2 uv = (worldPosition - _OceanFoamTravel.xy)
        / max(_WhitecapNoiseWorldSize, 0.5);
    float strength = max(_WhitecapDistortionStrength, 0.0);
    [branch]
    if (strength <= 0.0001) return uv;

    float frequency = 4.0 * clamp(_WhitecapDistortionScale, 0.1, 1.0);
    float2 q = uv * frequency;
    float2 cell = floor(q);
    float2 local = frac(q);
    float phaseSin;
    float phaseCos;
    sincos(_OceanFoamTravel.z, phaseSin, phaseCos);
    float2 pull = 0.0;
    float weightSum = 0.0;
    // Soft cellular attraction bends the one foam texture into rounded pockets.
    // Blend nearby sites instead of switching abruptly at Voronoi boundaries.
    [unroll]
    for (int y = -1; y <= 1; y++)
    {
        [unroll]
        for (int x = -1; x <= 1; x++)
        {
            float2 offset = float2(x, y);
            float2 seed = MotuFoamCellSeed(cell + offset) * 2.0 - 1.0;
            float2 orbit = float2(seed.x * phaseCos - seed.y * phaseSin,
                seed.x * phaseSin + seed.y * phaseCos);
            float2 centre = offset + 0.5 + seed * 0.10 + orbit * 0.09;
            float2 delta = centre - local;
            float distanceSquared = dot(delta, delta);
            // Sites stay at least 0.27 cells inside their tile. A radius of
            // 1.2 therefore vanishes before any site leaves this 3x3 search.
            float support = saturate(1.0 - distanceSquared / 1.44);
            float weight = support * support * support * exp2(-6.0 * distanceSquared);
            pull += delta * weight;
            weightSum += weight;
        }
    }
    return uv + pull / max(weightSum, 0.0001) * (strength / frequency);
}

float MotuOceanFoamPatches(float2 worldPosition)
{
    // One noise lookup for coverage: there is no second breakup/opacity layer.
    float foamNoise = tex2Dlod(_MotuWindNoise,
        float4(MotuOceanFoamUv(worldPosition), 0.0, 0.0)).r;
    float threshold = 1.0 - saturate(_WhitecapCoverage);
    return smoothstep(threshold - 0.12, threshold + 0.12, foamNoise);
}

void MotuEvaluateOceanWaveNormal(
    float2 worldPosition,
    out float3 worldNormal,
    out float whitecap,
    out float crestResponse)
{
    worldNormal = float3(0.0, 1.0, 0.0);
    whitecap = 0.0;
    crestResponse = 0.0;
    float geometricWaveWeight = saturate(_GeometricWaves);
    [branch]
    if (geometricWaveWeight <= 0.0001)
    {
        return;
    }
    float4 coastalData = MotuOceanCoastalData(worldPosition);
    float normalAttenuation = coastalData.r;
    float3 baseDisplacement;
    float2 baseHeightDerivative;
    float baseCurvature;
    MotuEvaluateOceanWaveField(
        worldPosition,
        baseDisplacement,
        baseHeightDerivative,
        baseCurvature);

    float3 onshoreDisplacement;
    float2 onshoreHeightDerivative;
    float onshoreInfluence;
    float breakerFoam;
    float onshoreCurvature;
    MotuEvaluateOnshoreWaveField(
        worldPosition,
        onshoreDisplacement,
        onshoreHeightDerivative,
        onshoreInfluence,
        breakerFoam,
        onshoreCurvature);
    float surfaceWaveAllowance = max(normalAttenuation, onshoreInfluence);
    [branch]
    if (surfaceWaveAllowance <= 0.0001)
    {
        return;
    }
    float3 combinedDisplacement = baseDisplacement * normalAttenuation
        + onshoreDisplacement;
    float depthWaveScale = MotuOceanDepthWaveScale(coastalData);
    combinedDisplacement *= depthWaveScale;
    float2 heightDerivative = (
        baseHeightDerivative * normalAttenuation
        + onshoreHeightDerivative)
        * geometricWaveWeight
        * depthWaveScale;
    worldNormal = normalize(float3(
        -heightDerivative.x,
        1.0,
        -heightDerivative.y));

    crestResponse = MotuOceanHeightTranslucency(combinedDisplacement.y * geometricWaveWeight);

    float maximumAmplitude = max(
        MotuOceanMaximumWaveHeight() * depthWaveScale,
        0.001);
    float normalizedHeight = saturate(
        0.5 + combinedDisplacement.y / (2.0 * maximumAmplitude));
    float threshold = clamp(_WhitecapHeightThreshold, 0.5, 0.98);
    float crest = smoothstep(
        threshold,
        min(threshold + 0.16, 0.999),
        normalizedHeight);
    float slopeThreshold = saturate(_WhitecapSlopeThreshold);
    float slope = length(heightDerivative);
    float breakingSlope = smoothstep(
        slopeThreshold,
        slopeThreshold + 0.24,
        slope);

    float brokenPatches = MotuOceanFoamPatches(worldPosition);
    float slopeWeight = lerp(0.35, 1.0, breakingSlope);
    float ordinaryWhitecap = crest
        * slopeWeight
        * brokenPatches;
    float breakerWhitecap = breakerFoam * geometricWaveWeight * brokenPatches;
    whitecap = max(ordinaryWhitecap, breakerWhitecap)
        * max(_WhitecapStrength, 0.0);
}

void MotuEvaluateOceanWaveNormal(float2 worldPosition, out float3 worldNormal,
    out float whitecap)
{
    float unusedResponse;
    MotuEvaluateOceanWaveNormal(worldPosition, worldNormal, whitecap, unusedResponse);
}

#endif
