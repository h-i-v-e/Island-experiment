using Motu.Settings;
using UnityEngine.AI;

namespace Motu.Navigation
{
    internal static class IslandNavigationBuildSettings
    {
        internal static NavMeshBuildSettings BuildSettings(this IslandNavigationSettings configuration, int agentTypeId)
        {
            configuration.Validate();
            var settings = NavMesh.GetSettingsByID(agentTypeId);
            settings.agentRadius = configuration.AgentRadius;
            settings.agentHeight = configuration.AgentHeight;
            settings.agentClimb = configuration.StepHeight;
            settings.agentSlope = configuration.MaximumSlope;
            settings.overrideVoxelSize = true;
            settings.voxelSize = configuration.VoxelSize > 0f ? configuration.VoxelSize : configuration.AgentRadius / 3f;
            settings.overrideTileSize = true;
            settings.tileSize = configuration.TileSize;
            settings.buildHeightMesh = true;
            settings.maxJobWorkers = (uint)configuration.MaximumBuildWorkers;
            settings.minRegionArea = 2f;
            return settings;
        }
    }
}
