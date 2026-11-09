using UnityEngine;

namespace Motu.World
{
    /// <summary>A continuous speed-weighted strike clock, independent of visibility.</summary>
    internal sealed class WorldSquallLightning
    {
        private int seed, lastStrikeCycle = -1;
        private double phase;
        private float previousAge;
        private bool initialized;
        internal float StrikeAge { get; private set; } = float.NegativeInfinity;
        internal uint StrikeSeed { get; private set; }
        internal bool IsCurrent(WorldSquallState state) => initialized && state.Seed == seed && state.AgeSeconds >= previousAge;

        internal void Reset()
        {
            initialized = false;
            StrikeAge = float.NegativeInfinity;
        }

        internal bool Advance(WorldSquallState state, SquallWeatherSettings settings)
        {
            if (!initialized || state.Seed != seed || state.AgeSeconds < previousAge)
            {
                initialized = true;
                seed = state.Seed;
                previousAge = state.AgeSeconds;
                phase = 0;
                lastStrikeCycle = -1;
                StrikeAge = float.NegativeInfinity;
            }
            var from = previousAge;
            previousAge = state.AgeSeconds;
            if (previousAge <= from || state.Field.Velocity.sqrMagnitude <= .000001f
                || settings.MaximumSpeedMultiplier <= 0f) return false;
            var before = phase;
            phase += PhaseTravel(from, previousAge, settings);
            var candidate = (int)System.Math.Floor(phase);
            if (phase < candidate + StrikeOffset(candidate)) candidate--;
            if (candidate < 0 || candidate <= lastStrikeCycle) return false;
            lastStrikeCycle = candidate;
            StrikeSeed = Hash(unchecked((uint)seed) ^ unchecked((uint)candidate) * 0x9e3779b9u);
            StrikeAge = FindStrikeAge(from, previousAge, candidate + StrikeOffset(candidate) - before, settings);
            return true;
        }

        private double PhaseTravel(float from, float to, SquallWeatherSettings settings) =>
            WorldSquallMotion.Travel(seed, from, to, settings)
            / (settings.MaximumSpeedMultiplier * Mathf.Max(.2f, settings.LightningIntervalSeconds));

        private float FindStrikeAge(float from, float to, double phaseTravel, SquallWeatherSettings settings)
        {
            var low = from;
            var high = to;
            for (var i = 0; i < 16; i++)
            {
                var middle = (low + high) * .5f;
                if (PhaseTravel(from, middle, settings) < phaseTravel) low = middle;
                else high = middle;
            }
            return (low + high) * .5f;
        }

        private float StrikeOffset(int cycle)
        {
            var value = Hash(unchecked((uint)seed) ^ unchecked((uint)cycle) * 0x9e3779b9u);
            return .18f + (value & 0xffffffu) / 16777216f * .35f;
        }

        private static uint Hash(uint value)
        {
            unchecked { value ^= value >> 16; value *= 0x7feb352du; value ^= value >> 15; value *= 0x846ca68bu; return value ^ (value >> 16); }
        }
    }
}
