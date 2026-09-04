using System;
using UnityEngine;

[Serializable]
public sealed class IslandParameterVariationSettings
{
    [SerializeField] private bool enabled = true;
    [Range(0f, 0.5f)] [SerializeField] private float maximumHeightVariation = 0.30f;
    [Range(0f, 0.25f)] [SerializeField] private float waterRatioVariation = 0.14f;
    [Range(0f, 0.75f)] [SerializeField] private float slopeVariation = 0.35f;
    [Range(0f, 0.75f)] [SerializeField] private float noiseFrequencyVariation = 0.40f;
    [Range(0f, 0.75f)] [SerializeField] private float noiseStrengthVariation = 0.30f;
    [Range(0f, 0.75f)] [SerializeField] private float landMassOffsetVariation = 0.35f;
    [Range(0f, 0.75f)] [SerializeField] private float erosionVariation = 0.30f;

    public bool Enabled => enabled;
    internal float MaximumHeightVariation => maximumHeightVariation;
    internal float WaterRatioVariation => waterRatioVariation;
    internal float SlopeVariation => slopeVariation;
    internal float NoiseFrequencyVariation => noiseFrequencyVariation;
    internal float NoiseStrengthVariation => noiseStrengthVariation;
    internal float LandMassOffsetVariation => landMassOffsetVariation;
    internal float ErosionVariation => erosionVariation;
}
