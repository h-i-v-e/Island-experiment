using UnityEngine;

namespace Motu.Islands
{
    struct RandomTools
    {
        public static uint Mix(uint value)
        {
            value ^= value >> 16;
            value *= 0x7feb352du;
            value ^= value >> 15;
            value *= 0x846ca68bu;
            value ^= value >> 16;
            return value;
        }

        public static int SeedForCell(int factorySeed, Vector2Int worldCell)
        {
            unchecked
            {
                var value = (uint)factorySeed ^ 0x27d4eb2fu;
                value ^= (uint)worldCell.x * 0x9e3779b9u;
                value = (value << 17) | (value >> 15);
                value ^= (uint)worldCell.y * 0x85ebca6bu;
                var islandSeed = (int)(RandomTools.Mix(value) & 0x7fffffffu);
                return islandSeed != 0 ? islandSeed : 1;
            }
        }

        public static float RandomPositiveFloat(System.Random random)
        {
            return Mathf.Abs((float)random.NextDouble());
        }
    }
}
