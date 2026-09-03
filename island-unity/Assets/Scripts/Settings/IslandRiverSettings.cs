using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandRiverSettings
{
    [Tooltip("Minimum upstream catchment required for a river source, in hectares. Higher values create fewer rivers.")]
    [Range(0.01f, 10f)]
    [SerializeField] private float sourceCatchmentHectares = 0.05f;

    [Tooltip("Bias towards selecting steep river sources. Regenerate to apply.")]
    [Range(1f, 8f)]
    [SerializeField] private float steepSourceMultiplier = 4f;

    [Tooltip("Bias towards selecting high river sources. Regenerate to apply.")]
    [Range(0f, 20f)]
    [SerializeField] private float sourceElevationBoost = 9f;

    [Tooltip("Full river width at low-flow sources, in metres.")]
    [Min(0.25f)]
    [SerializeField] private float sourceWidthMetres = 2f;

    [Tooltip("Full river width at maximum accumulated flow, in metres.")]
    [Min(0.25f)]
    [SerializeField] private float maximumWidthMetres = 14f;

    [Tooltip("Nominal river depth at low-flow sources, in metres.")]
    [Min(0.05f)]
    [SerializeField] private float sourceDepthMetres = 0.35f;

    [Tooltip("Nominal river depth at maximum accumulated flow, in metres.")]
    [Min(0.05f)]
    [SerializeField] private float maximumDepthMetres = 2f;

    internal float SourceCatchmentHectares => Mathf.Clamp(sourceCatchmentHectares, 0.01f, 10f);
    internal float SteepSourceMultiplier => Mathf.Clamp(steepSourceMultiplier, 1f, 8f);
    internal float SourceElevationBoost => Mathf.Clamp(sourceElevationBoost, 0f, 20f);
    internal float SourceWidthMetres => Mathf.Max(sourceWidthMetres, 0.25f);
    internal float MaximumWidthMetres => Mathf.Max(maximumWidthMetres, SourceWidthMetres);
    internal float SourceDepthMetres => Mathf.Max(sourceDepthMetres, 0.05f);
    internal float MaximumDepthMetres => Mathf.Max(maximumDepthMetres, SourceDepthMetres);
}
