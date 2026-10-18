using UnityEngine;

namespace Motu.World
{
    public sealed partial class OceanSurfaceController
    {
        public OceanIslandShelterField IslandShelter { get; private set; }

        internal OceanIslandShelterField EnsureIslandShelter()
        {
            if (IslandShelter == null) IslandShelter = GetComponent<OceanIslandShelterField>();
            if (IslandShelter == null) IslandShelter = gameObject.AddComponent<OceanIslandShelterField>();
            IslandShelter.Configure(this);
            return IslandShelter;
        }

        private void ResetIslandShelter()
        {
            if (IslandShelter == null) IslandShelter = GetComponent<OceanIslandShelterField>();
            if (IslandShelter != null) IslandShelter.ResetHistory();
        }
    }
}
