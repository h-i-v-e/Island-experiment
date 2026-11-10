using System;
using Motu.World;
using UnityEngine;

namespace Motu.Rendering
{
    internal struct SquallWetnessState
    {
        internal float Amount { get; private set; }
        internal float Rain { get; private set; }
        private float remainingDrops;

        internal void Advance(float rain, float deltaTime, float wettingSeconds, float dryingSeconds)
        {
            if (!float.IsFinite(rain) || rain < 0)
                throw new ArgumentOutOfRangeException(nameof(rain));
            if (!float.IsFinite(deltaTime) || deltaTime < 0)
                throw new ArgumentOutOfRangeException(nameof(deltaTime));
            Rain = Mathf.Clamp01(rain);
            if (rain > 0)
            {
                remainingDrops = WorldSquallRain.ParticleLifetime;
                // Even drizzle eventually wets a surface completely. Intensity
                // changes the accumulation rate, rather than capping wetness.
                Amount = Mathf.MoveTowards(Amount, 1,
                    deltaTime * rain / Mathf.Max(.1f, wettingSeconds));
                return;
            }
            var dryTime = Mathf.Max(0, deltaTime - remainingDrops);
            remainingDrops = Mathf.Max(0, remainingDrops - deltaTime);
            Amount = Mathf.MoveTowards(Amount, 0, dryTime / Mathf.Max(.1f, dryingSeconds));
        }
    }
}
