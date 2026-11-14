using System;
using Motu.Interop;
using UnityEngine;
using UnityEngine.Serialization;

namespace Motu.Settings
{
    [Serializable]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandForestSettings")]
    public sealed class IslandForestSettings
    {
        // Values are copied; authored Unity asset references intentionally remain shared.
        internal IslandForestSettings Copy() => (IslandForestSettings)MemberwiseClone();

        [Tooltip("Show streamed forest foliage and wood without regenerating the island.")]
        [SerializeField] private bool showForests = true;

        [Tooltip("Physical size of the coherent forest noise patches. Regenerate to apply.")]
        [Min(32f)]
        [SerializeField] private float forestPatchSizeMetres = 200f;

        [Tooltip("Normalized coherent-noise coverage threshold. Higher values produce fewer trees. Regenerate to apply.")]
        [Range(0f, 1f)]
        [SerializeField] public float forestNoiseThreshold = 0.62f;

        [Tooltip("Physical height at and above which forest placement is rejected. Regenerate to apply.")]
        [Min(0.01f)]
        [SerializeField] public float snowlineMetres = 100f;

        [Tooltip("Minimum uniform scale for accepted trees. Regenerate to apply.")]
        [Min(0.01f)]
        [SerializeField] private float minimumTreeScale = 1f;

        [Tooltip("Maximum uniform scale for accepted trees. Regenerate to apply.")]
        [Min(0.01f)]
        [SerializeField] private float maximumTreeScale = 3f;

        [Tooltip("Fraction of trees near which a fallen log is attempted. Unsuitable or obstructed positions are skipped. Regenerate to apply.")]
        [Range(0f, 1f)]
        [SerializeField] private float fallenLogDensity = 0.18f;

        [Header("Forest Species")]
        [Tooltip("Standalone island latitude. The coherent archipelago supplies its own geographic latitude.")]
        [Range(-90f, 90f)] [SerializeField] private float forestLatitudeDegrees = 40f;
        [Tooltip("Temperature decrease between sea level and the forest snowline. Higher values favour upland conifers.")]
        [Range(0f, 40f)] [SerializeField] private float altitudeCooling = 22f;
        [Tooltip("Shared prototypes per species. Broadleaf trees, conifers and palms each receive this many variants.")]
        [Range(1, 8)] [SerializeField] private int variantsPerSpecies = 4;

        public float ForestLatitudeDegrees { get => forestLatitudeDegrees; set => forestLatitudeDegrees = Mathf.Clamp(value, -90f, 90f); }
        public float AltitudeCooling => Mathf.Clamp(altitudeCooling, 0f, 40f);
        internal HybridForestNative.Options HybridOptions => new HybridForestNative.Options { latitudeDegrees = ForestLatitudeDegrees,
                altitudeCooling = Mathf.Clamp(altitudeCooling, 0f, 40f),
                variantsPerSpecies = (uint)Mathf.Clamp(variantsPerSpecies, 1, 8) };

        internal float FallenLogDensity => Mathf.Clamp01(fallenLogDensity);
        public bool ShowForests { get => showForests; set => showForests = value; }
        internal float ForestPatchSizeMetres => Mathf.Max(forestPatchSizeMetres, 32f);
        internal float ForestNoiseThreshold => Mathf.Clamp01(forestNoiseThreshold);
        internal float SnowlineMetres => Mathf.Max(snowlineMetres, 0.01f);
        internal float MinimumTreeScale => Mathf.Max(minimumTreeScale, 0.01f);
        internal float MaximumTreeScale => Mathf.Max(maximumTreeScale, MinimumTreeScale);
    }
}
