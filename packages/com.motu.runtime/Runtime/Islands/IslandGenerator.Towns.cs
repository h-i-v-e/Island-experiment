using Motu.Towns;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        private TownRenderer Town => islandRuntime != null ? islandRuntime.Town : null;
        public int GeneratedTownBuildingCount => Town != null ? Town.BuildingCount : 0;
        public int GeneratedTownRoadCount => Town != null ? Town.RoadCount : 0;
        public int GeneratedTownFrontageCount => Town != null ? Town.RoadFrontageCount : 0;
        public int GeneratedTownBridgeCandidateCount => Town != null ? Town.BridgeCandidateCount : 0;

        public bool TryGetTownBridgeCandidate(int index, out Vector3 from, out Vector3 to, out float width)
        {
            if (Town != null) return Town.TryGetBridgeCandidate(index, out from, out to, out width);
            from = to = default;
            width = 0f;
            return false;
        }

        public bool TryGetTownInspectionPoint(out Vector3 point)
        {
            var town = Town;
            point = town != null ? town.transform.TransformPoint(town.InspectionLocalPosition) : default;
            return town != null;
        }
    }
}
