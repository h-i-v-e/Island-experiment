#ifndef MOTU_OCEAN_WAVES_INCLUDED
#define MOTU_OCEAN_WAVES_INCLUDED

sampler2D _WaveAttenuationTex;
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
    float2 worldPosition,
    float time,
    inout float3 displacement,
    inout float2 heightDerivative)
{
    float2 direction = normalize(wave.xy + float2(1.0e-6, 0.0));
    float wavelength = max(wave.z, 0.25);
    float amplitude = max(wave.w, 0.0);
    float waveNumber = 6.28318530718 / wavelength;
    float phase = dot(direction, worldPosition) * waveNumber
        + time * max(speed, 0.0) * waveNumber;
    float waveSin;
    float waveCos;
    sincos(phase, waveSin, waveCos);
    displacement.y += amplitude * waveSin;
    displacement.xz += direction
        * (amplitude * saturate(choppiness) * waveCos);
    heightDerivative += direction * (amplitude * waveNumber * waveCos);
}

void MotuEvaluateOceanWaveField(
    float2 worldPosition,
    out float3 displacement,
    out float2 heightDerivative)
{
    displacement = 0.0;
    heightDerivative = 0.0;

    MotuAccumulateOceanWave(
        _OceanWave0,
        _OceanWaveSpeeds.x,
        _OceanWaveChoppiness.x,
        worldPosition,
        _Time.y,
        displacement,
        heightDerivative);
    MotuAccumulateOceanWave(
        _OceanWave1,
        _OceanWaveSpeeds.y,
        _OceanWaveChoppiness.y,
        worldPosition,
        _Time.y,
        displacement,
        heightDerivative);
    MotuAccumulateOceanWave(
        _OceanWave2,
        _OceanWaveSpeeds.z,
        _OceanWaveChoppiness.z,
        worldPosition,
        _Time.y,
        displacement,
        heightDerivative);
    MotuAccumulateOceanWave(
        _OceanWave3,
        _OceanWaveSpeeds.w,
        _OceanWaveChoppiness.w,
        worldPosition,
        _Time.y,
        displacement,
        heightDerivative);

}

void MotuEvaluateOceanWaveDisplacement(
    float2 worldPosition,
    float localRadius,
    out float3 displacement)
{
    float2 unusedHeightDerivative;
    MotuEvaluateOceanWaveField(
        worldPosition,
        displacement,
        unusedHeightDerivative);
    float distanceFade = 1.0 - smoothstep(
        _WaveFadeStart,
        max(_WaveFadeEnd, _WaveFadeStart + 0.001),
        localRadius);
    float geometryAttenuation = MotuOceanWaveAttenuation(worldPosition)
        * distanceFade
        * saturate(_GeometricWaves);
    displacement *= geometryAttenuation;
}

void MotuEvaluateOceanWaveNormal(
    float2 worldPosition,
    out float3 worldNormal)
{
    float3 unusedDisplacement;
    float2 heightDerivative;
    MotuEvaluateOceanWaveField(
        worldPosition,
        unusedDisplacement,
        heightDerivative);
    float normalAttenuation = MotuOceanWaveAttenuation(worldPosition)
        * saturate(_GeometricWaves);
    heightDerivative *= normalAttenuation;
    worldNormal = normalize(float3(
        -heightDerivative.x,
        1.0,
        -heightDerivative.y));
}

#endif
