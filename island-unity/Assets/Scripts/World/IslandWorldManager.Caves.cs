using Motu.Islands;
using Motu.Streaming;
using UnityEngine;

namespace Motu.World
{
    public sealed partial class IslandWorldManager
    {
        /// <summary>Count installed cave mouths and locate the nearest one,
        /// including dormant islands that can be activated on arrival.</summary>
        public int GetCaveAvailability(Vector3 position, out CaveStreamer nearest, out int entranceIndex)
        {
            nearest = null;
            entranceIndex = -1;
            var count = 0;
            var nearestDistance = float.PositiveInfinity;
            foreach (var entry in managedIslands.Values)
            {
                var runtime = entry.Generator != null ? entry.Generator.Runtime : null;
                if (runtime == null || (runtime.State != IslandRuntimeState.Active
                    && runtime.State != IslandRuntimeState.Dormant)) continue;
                var caves = runtime.Caves;
                if (caves == null) continue;
                for (var index = 0; index < caves.CaveCount; index++)
                {
                    if (!caves.TryGetEntrance(index, out var mouth, out _)) continue;
                    count++;
                    var distance = (mouth - position).sqrMagnitude;
                    if (distance >= nearestDistance) continue;
                    nearestDistance = distance;
                    nearest = caves;
                    entranceIndex = index;
                }
            }
            return count;
        }
    }
}
