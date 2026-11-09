using UnityEngine;

namespace Motu.World
{
    // Independent seeded cubic noise channels smoothly vary speed and heading.
    // Displacements integrate velocity across noise boundaries, including long frames.
    internal static class WorldSquallMotion
    {
        private static readonly double[] QuadratureNodes = { .1834346424956498, .5255324099163290, .7966664774136267, .9602898564975363 };
        private static readonly double[] QuadratureWeights = { .3626837833783620, .3137066458778873, .2223810344533745, .1012285362903763 };

        internal static float Speed(int seed, float age, SquallWeatherSettings settings)
            => Mathf.Lerp(settings.MinimumSpeedMultiplier, settings.MaximumSpeedMultiplier,
                Noise(seed, age, settings.SpeedVariationSeconds));

        /// <summary>Unit heading in the downwind/crosswind basis, always within the authored cone.</summary>
        internal static Vector2 Direction(int seed, float age, SquallWeatherSettings settings)
        {
            var noise = Noise(seed ^ 0x4A17D, age, settings.HeadingVariationSeconds);
            var angle = (noise * 2f - 1f) * settings.MaximumHeadingDeviationDegrees * Mathf.Deg2Rad;
            return new Vector2(Mathf.Cos(angle), Mathf.Sin(angle));
        }

        internal static Vector2 AlongWind(Vector2 motion, Vector2 downwind) =>
            downwind * motion.x + new Vector2(-downwind.y, downwind.x) * motion.y;

        internal static Vector2 Displacement(int seed, float start, float end, SquallWeatherSettings settings)
        {
            if (settings.MaximumHeadingDeviationDegrees <= 0f)
                return new Vector2(Travel(seed, start, end, settings), 0f);
            double from = start, forward = 0, sideways = 0;
            while (from < end)
            {
                var to = System.Math.Min(end, System.Math.Min(
                    NextBoundary(from, settings.SpeedVariationSeconds), NextBoundary(from, settings.HeadingVariationSeconds)));
                IntegrateVelocity(seed, from, to, settings, ref forward, ref sideways);
                from = to;
            }
            return new Vector2((float)forward, (float)sideways);
        }

        private static double NextBoundary(double age, float period) =>
            (System.Math.Floor(age / period) + 1.0) * period;

        private static void IntegrateVelocity(int seed, double from, double to, SquallWeatherSettings settings,
            ref double forward, ref double sideways)
        {
            var half = (to - from) * .5;
            var middle = (from + to) * .5;
            // Eight-point Gaussian quadrature preserves smooth curved travel
            // across different frame partitions without per-frame allocation.
            for (var i = 0; i < QuadratureNodes.Length; i++)
            {
                var offset = half * QuadratureNodes[i];
                var early = Velocity(seed, (float)(middle - offset), settings);
                var late = Velocity(seed, (float)(middle + offset), settings);
                var weight = half * QuadratureWeights[i];
                forward += (early.x + late.x) * weight;
                sideways += (early.y + late.y) * weight;
            }
        }

        private static Vector2 Velocity(int seed, float age, SquallWeatherSettings settings) =>
            Direction(seed, age, settings) * Speed(seed, age, settings);

        private static float Noise(int seed, float age, float period)
        {
            float phase = age / period;
            int cell = Mathf.FloorToInt(phase);
            float t = phase - cell;
            return Mathf.Lerp(Value(seed, cell), Value(seed, cell + 1), t * t * (3 - 2 * t));
        }

        internal static float Travel(int seed, float start, float end, SquallWeatherSettings settings)
        {
            float period = settings.SpeedVariationSeconds;
            double from = start / (double)period, to = end / (double)period, area = 0;
            while (from < to)
            {
                int cell = (int)System.Math.Floor(from);
                double next = System.Math.Min(to, cell + 1.0);
                double a = from - cell, b = next - cell;
                float lower = Value(seed, cell), upper = Value(seed, cell + 1);
                area += lower * (b - a) + (upper - lower) * (Integral(b) - Integral(a));
                from = next;
            }
            return settings.MinimumSpeedMultiplier * (end - start)
                + (settings.MaximumSpeedMultiplier - settings.MinimumSpeedMultiplier) * (float)(area * period);
        }

        private static double Integral(double t) => t * t * t * (1 - .5 * t);

        private static float Value(int seed, int cell)
        {
            uint value = unchecked((uint)seed ^ (uint)cell * 0x9e3779b9u);
            value ^= value >> 16; value *= 0x7feb352du;
            value ^= value >> 15; value *= 0x846ca68bu;
            value ^= value >> 16;
            return (value & 0xffffffu) / 16777215f;
        }
    }
}
