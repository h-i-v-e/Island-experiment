using System;
using UnityEngine;
using UnityEngine.Serialization;

namespace Motu.Settings
{
    [Serializable]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandRiverSettings")]
    public sealed class IslandRiverSettings
    {
        // Values are copied; authored Unity asset references intentionally remain shared.
        internal IslandRiverSettings Copy() => (IslandRiverSettings)MemberwiseClone();

        [Tooltip("Minimum upstream catchment required for a river source, in hectares. Higher values create fewer rivers.")]
        [Range(0.01f, 10f)]
        [SerializeField] private float sourceCatchmentHectares = 0.05f;

        [Tooltip("Catchment-area penalty applied to steep river sources. Higher values make steep sources less likely. Regenerate to apply.")]
        [Range(1f, 8f)]
        [FormerlySerializedAs("steepSourceMultiplier")]
        [SerializeField] private float steepSourceCatchmentMultiplier = 4f;

        [Tooltip("Bias towards selecting high river sources. Regenerate to apply.")]
        [Range(0f, 20f)]
        [SerializeField] private float sourceElevationBoost = 20f;

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

        public float SourceCatchmentHectares
        {
            get => Mathf.Clamp(sourceCatchmentHectares, 0.01f, 10f);
            set => sourceCatchmentHectares = Mathf.Clamp(value, 0.01f, 10f);
        }
        public float SteepSourceCatchmentMultiplier
        {
            get => Mathf.Clamp(steepSourceCatchmentMultiplier, 1f, 8f);
            set => steepSourceCatchmentMultiplier = Mathf.Clamp(value, 1f, 8f);
        }
        public float SourceElevationBoost
        {
            get => Mathf.Clamp(sourceElevationBoost, 0f, 20f);
            set => sourceElevationBoost = Mathf.Clamp(value, 0f, 20f);
        }
        public float SourceWidthMetres
        {
            get => Mathf.Max(sourceWidthMetres, 0.25f);
            set => sourceWidthMetres = Mathf.Max(value, 0.25f);
        }
        public float MaximumWidthMetres
        {
            get => Mathf.Max(maximumWidthMetres, SourceWidthMetres);
            set => maximumWidthMetres = Mathf.Max(value, SourceWidthMetres);
        }
        public float SourceDepthMetres
        {
            get => Mathf.Max(sourceDepthMetres, 0.05f);
            set => sourceDepthMetres = Mathf.Max(value, 0.05f);
        }
        public float MaximumDepthMetres
        {
            get => Mathf.Max(maximumDepthMetres, SourceDepthMetres);
            set => maximumDepthMetres = Mathf.Max(value, SourceDepthMetres);
        }
    }
}
