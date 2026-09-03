#ifndef MOTU_OCEAN_WAVES_INCLUDED
#define MOTU_OCEAN_WAVES_INCLUDED

sampler2D _WaveAttenuationTex;
sampler2D _WaveOnshoreTex;
sampler2D _NoiseTex;
float4 _WaveAttenuationWorldRect;
float4 _OceanWave0;
float4 _OceanWave1;
float4 _OceanWave2;
float4 _OceanWave3;
float4 _OceanWaveSpeeds;
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
float _WhitecapFineNoiseScale;
float _WhitecapCounterflowSpeed;
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
    float maximumChoppiness = max(
        max(_OceanWaveChoppiness.x, _OceanWaveChoppiness.y),
        max(_OceanWaveChoppiness.z, _OceanWaveChoppiness.w));
    maximumChoppiness = max(
        maximumChoppiness,
        saturate(_OnshoreWaveParameters.w));
    return max(
        (directionalAmplitude + onshoreAmplitude)
            * (1.0 + 0.22 * saturate(maximumChoppiness)),
        0.001);
}

float MotuOceanDepthWaveScale(float4 coastalData)
{
    static const float SeaMaskDepthMetres = 5.0;
    float waterDepth = saturate(coastalData.b) * SeaMaskDepthMetres;
    return saturate(waterDepth / MotuOceanMaximumWaveHeight());
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
    out float height,
    out float distanceDerivative,
    out float breakerFoamSlope)
{
    static const float Pi = 3.14159265359;
    static const float TwoPi = 6.28318530718;
    static const float MinimumLeadingFraction = 0.04;
    float maximumSharpness = saturate(_OnshoreWaveBreaking.x);
    float sharpeningDistance = max(_OnshoreWaveBreaking.y, 0.25);
    float distanceProgress = saturate(coastDistance / sharpeningDistance);
    float shoreProximity = 1.0
        - distanceProgress * distanceProgress
            * (3.0 - 2.0 * distanceProgress);
    float proximityDerivative = 0.0;
    [flatten]
    if (coastDistance > 0.0 && coastDistance < sharpeningDistance)
    {
        proximityDerivative = -6.0
            * distanceProgress
            * (1.0 - distanceProgress)
            / sharpeningDistance;
    }

    // At 0.5 the two faces reproduce an ordinary sine wave. Moving only the
    // leading fraction toward zero compresses the shore-facing rise while the
    // rear face expands to retain the complete wavelength.
    float leadingFraction = lerp(
        0.5,
        MinimumLeadingFraction,
        maximumSharpness * shoreProximity);
    float leadingFractionDerivative = (MinimumLeadingFraction - 0.5)
        * maximumSharpness
        * proximityDerivative;
    float cycle = frac(phase / TwoPi + 0.25);
    float cycleDerivative = 1.0 / max(wavelength, 1.0);
    float leadingSlopeExcess = Pi
        * max(1.0 / leadingFraction - 2.0, 0.0)
        / max(wavelength, 1.0);

    [flatten]
    if (cycle < leadingFraction)
    {
        float leadingProgress = cycle / leadingFraction;
        float angle = Pi * cycle / leadingFraction;
        float angleDerivative = Pi
            * (cycleDerivative * leadingFraction
                - cycle * leadingFractionDerivative)
            / (leadingFraction * leadingFraction);
        height = -cos(angle);
        distanceDerivative = sin(angle) * angleDerivative;
        breakerFoamSlope = leadingSlopeExcess
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
    height = cos(rearAngle);
    distanceDerivative = -sin(rearAngle)
        * Pi
        * rearProgressDerivative;
    breakerFoamSlope = leadingSlopeExcess
        * (1.0 - smoothstep(0.0, 0.12, rearProgress));
}

void MotuAccumulateOnshoreWave(
    float2 onshoreDirection,
    float influence,
    float coastalCoordinate,
    inout float3 displacement,
    inout float2 heightDerivative,
    out float breakerFoamSlope)
{
    float wavelength = max(_OnshoreWaveParameters.x, 1.0);
    float amplitude = max(_OnshoreWaveParameters.y, 0.0)
        * max(influence, 0.0);
    float speed = max(_OnshoreWaveParameters.z, 0.0);
    float choppiness = saturate(_OnshoreWaveParameters.w);
    float waveNumber = 6.28318530718 / wavelength;

    // The composed attenuation is a continuous coast-relative coordinate:
    // zero at the shore and one in open water. Treating its transition as a
    // sixteen-metre band keeps the phase continuous as the direction bends
    // around bays and headlands. Positive time travels towards coordinate 0.
    float coastDistance = saturate(coastalCoordinate) * 16.0;
    float phase = coastDistance * waveNumber + _Time.y * speed * waveNumber;
    float waveSin;
    float waveCos;
    sincos(phase, waveSin, waveCos);
    float breakerHeight;
    float breakerDistanceDerivative;
    MotuEvaluateOnshoreBreakerShape(
        phase,
        coastDistance,
        wavelength,
        breakerHeight,
        breakerDistanceDerivative,
        breakerFoamSlope);
    breakerFoamSlope *= amplitude;
    float waveSinDouble = 2.0 * waveSin * waveCos;
    float waveCosDouble = waveCos * waveCos - waveSin * waveSin;
    float crestBias = choppiness * 0.22;
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
    float4 wave,
    float speed,
    float choppiness,
    float amplitudeScale,
    float2 worldPosition,
    float time,
    inout float3 displacement,
    inout float2 heightDerivative)
{
    float2 direction = normalize(wave.xy + float2(1.0e-6, 0.0));
    float wavelength = max(wave.z, 0.25);
    float amplitude = max(wave.w, 0.0) * max(amplitudeScale, 0.0);
    float waveNumber = 6.28318530718 / wavelength;
    float phase = dot(direction, worldPosition) * waveNumber
        + time * max(speed, 0.0) * waveNumber;
    float waveSin;
    float waveCos;
    sincos(phase, waveSin, waveCos);
    float waveSinDouble = 2.0 * waveSin * waveCos;
    float waveCosDouble = waveCos * waveCos - waveSin * waveSin;
    float crestBias = saturate(choppiness) * 0.22;
    displacement.y += amplitude * (waveSin - crestBias * waveCosDouble);
    displacement.xz += direction
        * (amplitude * saturate(choppiness) * waveCos);
    heightDerivative += direction
        * (amplitude * waveNumber
            * (waveCos + 2.0 * crestBias * waveSinDouble));
}

void MotuEvaluateOceanWaveField(
    float2 worldPosition,
    out float3 displacement,
    out float2 heightDerivative)
{
    displacement = 0.0;
    heightDerivative = 0.0;
    float noiseWorldSize = max(_WaveNoiseWorldSize, 256.0);
    float2 broadUv = worldPosition / noiseWorldSize;
    float2 detailUv = worldPosition / (noiseWorldSize * 0.37)
        + float2(0.371, 0.619);
    float2 broadNoise = tex2Dlod(
        _NoiseTex,
        float4(broadUv, 0.0, 0.0)).rg;
    float2 detailNoise = tex2Dlod(
        _NoiseTex,
        float4(detailUv, 0.0, 0.0)).gr;
    float2 domainWarp = ((broadNoise - 0.5) * 1.35
        + (detailNoise - 0.5) * 0.45)
        * max(_WaveDomainWarp, 0.0);
    float amplitudeVariation = saturate(_WaveAmplitudeVariation);
    float4 amplitudeScales = 1.0
        + (float4(
            broadNoise.x,
            broadNoise.y,
            detailNoise.x,
            detailNoise.y) - 0.5)
        * (2.0 * amplitudeVariation);

    MotuAccumulateOceanWave(
        _OceanWave0,
        _OceanWaveSpeeds.x,
        _OceanWaveChoppiness.x,
        amplitudeScales.x,
        worldPosition + domainWarp,
        _Time.y,
        displacement,
        heightDerivative);
    MotuAccumulateOceanWave(
        _OceanWave1,
        _OceanWaveSpeeds.y,
        _OceanWaveChoppiness.y,
        amplitudeScales.y,
        worldPosition + float2(-domainWarp.y, domainWarp.x) * 0.82,
        _Time.y,
        displacement,
        heightDerivative);
    MotuAccumulateOceanWave(
        _OceanWave2,
        _OceanWaveSpeeds.z,
        _OceanWaveChoppiness.z,
        amplitudeScales.z,
        worldPosition - domainWarp * 0.61,
        _Time.y,
        displacement,
        heightDerivative);
    MotuAccumulateOceanWave(
        _OceanWave3,
        _OceanWaveSpeeds.w,
        _OceanWaveChoppiness.w,
        amplitudeScales.w,
        worldPosition + float2(domainWarp.y, -domainWarp.x) * 1.13,
        _Time.y,
        displacement,
        heightDerivative);

}

void MotuEvaluateOnshoreWaveField(
    float2 worldPosition,
    out float3 displacement,
    out float2 heightDerivative,
    out float influence,
    out float breakerFoamSlope)
{
    displacement = 0.0;
    heightDerivative = 0.0;
    influence = 0.0;
    breakerFoamSlope = 0.0;
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
        displacement,
        heightDerivative,
        breakerFoamSlope);
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

void MotuEvaluateOceanWaveNormal(
    float2 worldPosition,
    out float3 worldNormal,
    out float whitecap)
{
    worldNormal = float3(0.0, 1.0, 0.0);
    whitecap = 0.0;
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
    MotuEvaluateOceanWaveField(
        worldPosition,
        baseDisplacement,
        baseHeightDerivative);

    float3 onshoreDisplacement;
    float2 onshoreHeightDerivative;
    float onshoreInfluence;
    float breakerFoamSlope;
    MotuEvaluateOnshoreWaveField(
        worldPosition,
        onshoreDisplacement,
        onshoreHeightDerivative,
        onshoreInfluence,
        breakerFoamSlope);
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

    float noiseWorldSize = max(_WhitecapNoiseWorldSize, 0.5);
    float2 primaryDirection = normalize(
        _OceanWave0.xy + float2(1.0e-6, 0.0));
    float2 foamTravel = primaryDirection
        * (_Time.y * max(_OceanWaveSpeeds.x, 0.0));
    float2 foamUv = (worldPosition - foamTravel) / noiseWorldSize;
    float foamNoiseA = tex2Dlod(
        _NoiseTex,
        float4(foamUv, 0.0, 0.0)).r;
    float2 foamUvB = float2(-foamUv.y, foamUv.x) * 1.73
        + float2(0.217, 0.683);
    float foamNoiseB = tex2Dlod(
        _NoiseTex,
        float4(foamUvB, 0.0, 0.0)).g;
    float broadFoamNoise = lerp(foamNoiseA, foamNoiseB, 0.38);
    float coverageThreshold = 1.0 - saturate(_WhitecapCoverage);
    float broadPatches = smoothstep(
        coverageThreshold - 0.12,
        coverageThreshold + 0.12,
        broadFoamNoise);

    float fineNoiseScale = clamp(_WhitecapFineNoiseScale, 0.1, 1.0);
    float counterflowSpeed = max(_WhitecapCounterflowSpeed, 0.0);
    float2 fineTravel = -primaryDirection
        * (_Time.y * max(_OceanWaveSpeeds.x, 0.0) * counterflowSpeed);
    float2 fineWorldPosition = worldPosition - fineTravel;
    float2 fineFoamUv = float2(
        fineWorldPosition.x * 0.819 - fineWorldPosition.y * 0.574,
        fineWorldPosition.x * 0.574 + fineWorldPosition.y * 0.819)
        / (noiseWorldSize * fineNoiseScale)
        + float2(0.413, 0.127);
    float fineFoamNoise = tex2Dlod(
        _NoiseTex,
        float4(fineFoamUv, 0.0, 0.0)).g;
    float finePatches = smoothstep(0.32, 0.68, fineFoamNoise);
    float brokenPatches = broadPatches * lerp(0.28, 1.0, finePatches);
    float slopeWeight = lerp(0.35, 1.0, breakingSlope);
    float ordinaryWhitecap = crest
        * slopeWeight
        * brokenPatches;
    float breakerSlope = breakerFoamSlope
        * geometricWaveWeight
        * depthWaveScale;
    float breakerWhitecap = smoothstep(
        slopeThreshold * 0.5,
        slopeThreshold + 0.65,
        breakerSlope)
        * brokenPatches;
    whitecap = max(ordinaryWhitecap, breakerWhitecap)
        * max(_WhitecapStrength, 0.0);
}

#endif
