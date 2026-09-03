using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandReedSettings
{
    [Tooltip("Show LOD 0 riverbank reeds and rushes without regenerating the island.")]
    [SerializeField] private bool showReeds = true;

    [Tooltip("Maximum distance across the immediate dry river bank. Regenerate to apply.")]
    [Range(0.25f, 10f)]
    [SerializeField] private float bankWidthMetres = 0.8f;

    [Tooltip("Physical size of coherent reed patches. Regenerate to apply.")]
    [Range(2f, 50f)]
    [SerializeField] private float patchSizeMetres = 8f;

    [Tooltip("Higher thresholds create fewer and more separated reed patches. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float coverageThreshold = 0.18f;

    [Tooltip("Minimum spacing between clump roots. Regenerate to apply.")]
    [Range(0.2f, 3f)]
    [SerializeField] private float spacingMetres = 0.36f;

    [Tooltip("Fraction of the outer bank strip occupied by shorter rushes. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float rushRatio = 0.45f;

    [Tooltip("Minimum clump height. Regenerate to apply.")]
    [Range(0.2f, 3f)]
    [SerializeField] private float minimumHeightMetres = 0.65f;

    [Tooltip("Maximum clump height. Regenerate to apply.")]
    [Range(0.2f, 4f)]
    [SerializeField] private float maximumHeightMetres = 2.1f;

    [Tooltip("Steepest bank on which reeds may be planted. Regenerate to apply.")]
    [Range(0f, 60f)]
    [SerializeField] private float maximumSlopeDegrees = 32f;

    [Tooltip("Dark green-brown used at the base and for rushes.")]
    [SerializeField] private Color baseColour = new Color(0.12f, 0.24f, 0.055f, 1f);

    [Tooltip("Sunlit yellow-green used toward reed tips.")]
    [SerializeField] private Color tipColour = new Color(0.38f, 0.48f, 0.09f, 1f);

    [Tooltip("Multiplier applied to the shared grass wind field.")]
    [Range(0f, 8f)]
    [SerializeField] private float windStrength = 3f;

    public bool ShowReeds { get => showReeds; set => showReeds = value; }
    internal float BankWidthMetres => Mathf.Clamp(bankWidthMetres, 0.25f, 10f);
    internal float PatchSizeMetres => Mathf.Clamp(patchSizeMetres, 2f, 50f);
    internal float CoverageThreshold => Mathf.Clamp01(coverageThreshold);
    internal float SpacingMetres => Mathf.Clamp(spacingMetres, 0.2f, 3f);
    internal float RushRatio => Mathf.Clamp01(rushRatio);
    internal float MinimumHeightMetres => Mathf.Clamp(minimumHeightMetres, 0.2f, 3f);
    internal float MaximumHeightMetres => Mathf.Max(maximumHeightMetres, MinimumHeightMetres);
    internal float MaximumSlopeDegrees => Mathf.Clamp(maximumSlopeDegrees, 0f, 60f);
    internal Color BaseColour => baseColour;
    internal Color TipColour => tipColour;
    internal float WindStrength => Mathf.Clamp(windStrength, 0f, 8f);
}
