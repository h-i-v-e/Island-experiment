using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandFernSettings
{
    [Tooltip("Show LOD 0 ferns around tree trunks without regenerating the island.")]
    [SerializeField] private bool showFerns = true;

    [Tooltip("Clear ground retained between bark and the first fern roots. Regenerate to apply.")]
    [Range(0f, 2f)]
    [SerializeField] private float barkClearanceMetres = 0.18f;

    [Tooltip("Outer radius of each trunk's fern bed. Regenerate to apply.")]
    [Range(0.25f, 8f)]
    [SerializeField] private float outerRadiusMetres = 1.65f;

    [Tooltip("Minimum spacing between fern crowns. Regenerate to apply.")]
    [Range(0.2f, 4f)]
    [SerializeField] private float spacingMetres = 0.58f;

    [Tooltip("Physical size of coherent understory patches. Regenerate to apply.")]
    [Range(2f, 50f)]
    [SerializeField] private float patchSizeMetres = 12f;

    [Tooltip("Higher thresholds leave more tree trunks without ferns. Regenerate to apply.")]
    [Range(0f, 1f)]
    [SerializeField] private float coverageThreshold = 0.28f;

    [Tooltip("Minimum fern frond length. Regenerate to apply.")]
    [Range(0.2f, 2f)]
    [SerializeField] private float minimumLengthMetres = 0.45f;

    [Tooltip("Maximum fern frond length. Regenerate to apply.")]
    [Range(0.2f, 3f)]
    [SerializeField] private float maximumLengthMetres = 1.15f;

    [Tooltip("Steepest forest ground on which ferns may grow. Regenerate to apply.")]
    [Range(0f, 60f)]
    [SerializeField] private float maximumSlopeDegrees = 34f;

    [SerializeField] private Color baseColour = new Color(0.055f, 0.18f, 0.045f, 1f);
    [SerializeField] private Color tipColour = new Color(0.24f, 0.48f, 0.12f, 1f);

    [Tooltip("Multiplier applied to the shared grass wind field.")]
    [Range(0f, 8f)]
    [SerializeField] private float windStrength = 1.8f;

    public bool ShowFerns { get => showFerns; set => showFerns = value; }
    internal float BarkClearanceMetres => Mathf.Clamp(barkClearanceMetres, 0f, 2f);
    internal float OuterRadiusMetres => Mathf.Clamp(outerRadiusMetres, 0.25f, 8f);
    internal float SpacingMetres => Mathf.Clamp(spacingMetres, 0.2f, 4f);
    internal float PatchSizeMetres => Mathf.Clamp(patchSizeMetres, 2f, 50f);
    internal float CoverageThreshold => Mathf.Clamp01(coverageThreshold);
    internal float MinimumLengthMetres => Mathf.Clamp(minimumLengthMetres, 0.2f, 2f);
    internal float MaximumLengthMetres => Mathf.Max(maximumLengthMetres, MinimumLengthMetres);
    internal float MaximumSlopeDegrees => Mathf.Clamp(maximumSlopeDegrees, 0f, 60f);
    internal Color BaseColour => baseColour;
    internal Color TipColour => tipColour;
    internal float WindStrength => Mathf.Clamp(windStrength, 0f, 8f);
}
