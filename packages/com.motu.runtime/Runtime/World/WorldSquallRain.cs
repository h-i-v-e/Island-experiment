using UnityEngine;

namespace Motu.World
{
    // Shared by nearby droplets and camera surface wetness. Rain falls along
    // the wind, independently of the squall's own wandering travel direction.
    internal static class WorldSquallRain
    {
        internal const float FallSpeed = 38f;
        internal const float ParticleLifetime = 1.35f;
        internal static Vector2 Drift(Vector2 wind) => Vector2.ClampMagnitude(wind * .85f, 40f);

        internal static float IntensityAt(WorldSquallState state, SquallWeatherSettings settings,
            Vector3 viewer, Vector2 drift, float cloudBaseWorldY)
        {
            if (!settings.Enabled || !state.Active || state.Strength <= .001f || viewer.y >= cloudBaseWorldY) return 0;
            var diameter = Mathf.Clamp(state.DiameterMetres, SquallWeatherSettings.MinimumSupportedDiameterMetres,
                SquallWeatherSettings.MaximumSupportedDiameterMetres);
            var delta = new Vector2(viewer.x - state.Center.x, viewer.z - state.Center.z);
            delta -= drift * ((cloudBaseWorldY - viewer.y) / FallSpeed);
            var coverage = Mathf.SmoothStep(0, 1, 1 - delta.magnitude / (diameter * .5f));
            var activity = state.SpeedMultiplier / Mathf.Max(.001f, settings.MaximumSpeedMultiplier);
            return coverage * state.Strength * Mathf.Max(0, settings.RainIntensity) * Mathf.Lerp(.4f, 1, activity);
        }
    }
}
