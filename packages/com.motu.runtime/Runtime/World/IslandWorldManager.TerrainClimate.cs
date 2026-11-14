using UnityEngine;

namespace Motu.World
{
    public sealed partial class IslandWorldManager
    {
        /// <summary>Read the climate already assigned to this island without generating or loading it.</summary>
        public bool TryGetTerrainClimate(Vector3 worldPosition, out float latitudeDegrees,
            out float snowlineMetres, out float altitudeCooling)
        {
            latitudeDegrees = snowlineMetres = altitudeCooling = 0;
            if (!managedIslands.TryGetValue(WorldToCell(worldPosition), out var entry)) return false;
            var forest = entry.GenerationRequest.Forest;
            if (forest == null) return false;
            latitudeDegrees = forest.ForestLatitudeDegrees;
            snowlineMetres = Mathf.Max(.01f, forest.snowlineMetres);
            altitudeCooling = forest.AltitudeCooling;
            return true;
        }
    }
}
