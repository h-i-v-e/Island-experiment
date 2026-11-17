using Motu.Towns;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        private TownRenderer Town => islandRuntime != null
            ? islandRuntime.GetComponentInChildren<TownRenderer>() : null;
        public int GeneratedTownBuildingCount => Town != null ? Town.BuildingCount : 0;

        public bool TryGetTownInspectionPoint(out Vector3 point)
        {
            var town = Town;
            point = town != null ? town.transform.TransformPoint(town.InspectionLocalPosition) : default;
            return town != null;
        }
    }
}
