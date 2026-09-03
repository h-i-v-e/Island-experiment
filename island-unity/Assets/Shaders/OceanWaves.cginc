#ifndef MOTU_OCEAN_WAVES_INCLUDED
#define MOTU_OCEAN_WAVES_INCLUDED

sampler2D _WaveAttenuationTex;
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

float MotuOceanWaveAttenuation(float2 worldPosition)
{
    float2 uv = (worldPosition - _WaveAttenuationWorldRect.xy)
        * _WaveAttenuationWorldRect.zw;
    float inside = step(0.0, uv.x)
        * step(0.0, uv.y)
        * step(uv.x, 1.0)
        * step(uv.y, 1.0);
    float attenuation = tex2Dlod(
        _WaveAttenuationTex,
        float4(saturate(uv), 0.0, 0.0)).r;
    return lerp(1.0, attenuation, inside);
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
    float geometryAttenuation = MotuOceanWaveAttenuation(worldPosition)
        * distanceFade
        * saturate(_GeometricWaves);
    [branch]
    if (geometryAttenuation <= 0.0001)
    {
        return;
    }
    float2 unusedHeightDerivative;
    MotuEvaluateOceanWaveField(
        worldPosition,
        displacement,
        unusedHeightDerivative);
    displacement *= geometryAttenuation;
}

void MotuEvaluateOceanWaveNormal(
    float2 worldPosition,
    out float3 worldNormal)
{
    worldNormal = float3(0.0, 1.0, 0.0);
    float normalAttenuation = MotuOceanWaveAttenuation(worldPosition)
        * saturate(_GeometricWaves);
    [branch]
    if (normalAttenuation <= 0.0001)
    {
        return;
    }
    float3 unusedDisplacement;
    float2 heightDerivative;
    MotuEvaluateOceanWaveField(
        worldPosition,
        unusedDisplacement,
        heightDerivative);
    heightDerivative *= normalAttenuation;
    worldNormal = normalize(float3(
        -heightDerivative.x,
        1.0,
        -heightDerivative.y));
}

#endif
