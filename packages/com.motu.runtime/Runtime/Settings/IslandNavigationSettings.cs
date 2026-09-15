using System;
using UnityEngine;

namespace Motu.Settings
{
    [Serializable]
    public sealed class IslandNavigationSettings
    {
        [SerializeField] private bool enabled = true;
        [SerializeField, Min(0.05f)] private float agentRadius = 0.5f;
        [SerializeField, Min(0.1f)] private float agentHeight = 2f;
        [SerializeField, Min(0f)] private float stepHeight = 0.4f;
        [SerializeField, Range(0f, 60f)] private float maximumSlope = 45f;
        [Tooltip("Zero uses one third of the agent radius. Smaller values increase build time and memory.")]
        [SerializeField, Min(0f)] private float voxelSize;
        [SerializeField, Range(32, 256)] private int tileSize = 128;
        [Tooltip("Approximate width of each sequential navigation bake in metres, rounded to whole NavMesh tiles.")]
        [SerializeField, Range(32f, 256f)] private float chunkSize = 128f;
        [Tooltip("Limit parallel work inside each bake to reduce transient memory and leave CPU time for the host.")]
        [SerializeField, Range(1, 4)] private int maximumBuildWorkers = 1;
        [Tooltip("Minimum local ground height for land navigation. Zero excludes submerged ocean floor.")]
        [SerializeField] private float minimumGroundHeight;
        [Tooltip("Exclude river-bed triangles using the generated river material mask.")]
        [SerializeField] private bool excludeRiverBeds = true;
        [SerializeField] private bool logBuildStatistics = true;

        public bool Enabled { get => enabled; set => enabled = value; }
        public float AgentRadius { get => agentRadius; set => agentRadius = value; }
        public float AgentHeight { get => agentHeight; set => agentHeight = value; }
        public float StepHeight { get => stepHeight; set => stepHeight = value; }
        public float MaximumSlope { get => maximumSlope; set => maximumSlope = value; }
        public float VoxelSize { get => voxelSize; set => voxelSize = value; }
        public int TileSize { get => tileSize; set => tileSize = value; }
        public float ChunkSize { get => chunkSize; set => chunkSize = value; }
        public int MaximumBuildWorkers { get => maximumBuildWorkers; set => maximumBuildWorkers = value; }
        public float MinimumGroundHeight { get => minimumGroundHeight; set => minimumGroundHeight = value; }
        public bool ExcludeRiverBeds { get => excludeRiverBeds; set => excludeRiverBeds = value; }
        public bool LogBuildStatistics { get => logBuildStatistics; set => logBuildStatistics = value; }
        internal IslandNavigationSettings Copy() => (IslandNavigationSettings)MemberwiseClone();

        public void Validate()
        {
            if (!float.IsFinite(agentRadius) || agentRadius < 0.05f
                || !float.IsFinite(agentHeight) || agentHeight < agentRadius * 2f
                || !float.IsFinite(stepHeight) || stepHeight < 0f || stepHeight >= agentHeight
                || !float.IsFinite(maximumSlope) || maximumSlope < 0f || maximumSlope > 60f
                || !float.IsFinite(voxelSize) || voxelSize < 0f
                || !float.IsFinite(chunkSize) || chunkSize < 32f || chunkSize > 256f
                || maximumBuildWorkers < 1 || maximumBuildWorkers > 4
                || !float.IsFinite(minimumGroundHeight) || tileSize < 32 || tileSize > 256)
                throw new ArgumentException("Invalid island navigation agent dimensions or build settings.");
        }
    }
}
