using UnityEngine;

namespace Motu.World
{
    /// <summary>Physical storm position; horizon rendering never changes this footprint.</summary>
    public readonly struct WorldSquallState
    {
        public bool Active => DiameterMetres > 0f;
        public Vector3 Center { get; }
        public float DiameterMetres { get; }
        public float AgeSeconds { get; }
        public float Strength { get; }
        public float WeatherStrength { get; }
        public int Seed { get; }
        public WorldSquallField Field { get; }
        public float SpeedMultiplier { get; }

        internal WorldSquallState(Vector3 center, float diameter, float age, float strength,
            int seed, WorldSquallField field, float speedMultiplier = 1f, float weatherStrength = 1f)
        {
            Center = center; DiameterMetres = diameter; AgeSeconds = age;
            Strength = strength; Seed = seed; Field = field; SpeedMultiplier = speedMultiplier;
            WeatherStrength = weatherStrength;
        }
    }
}
