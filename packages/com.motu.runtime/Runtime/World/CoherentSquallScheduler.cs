using System;
using UnityEngine;

namespace Motu.World
{
    /// <summary>Forms the population supported by the current coherent weather.
    /// Improving conditions reset formation; worsening conditions increase its rate.</summary>
    public sealed class CoherentSquallScheduler
    {
        private float formationProgress;

        public void Reset() => formationProgress = 0f;

        public void Advance(WorldEnvironmentController environment, float deltaTime,
            float minimumWeatherActivity)
        {
            WeatherValueValidation.RequireFinite(deltaTime, nameof(deltaTime));
            WeatherValueValidation.RequireFinite(minimumWeatherActivity, nameof(minimumWeatherActivity));
            if (deltaTime < 0f) throw new ArgumentOutOfRangeException(nameof(deltaTime));
            if (environment == null || !environment.IsInstalled || !environment.isActiveAndEnabled) return;
            var weather = environment.Weather;
            var settings = weather.Squalls;
            var activity = WeatherActivity(weather, minimumWeatherActivity);
            var desiredCount = Mathf.CeilToInt(activity * settings.MaximumActiveSqualls);
            if (!settings.Enabled || !settings.AutomaticSpawning
                || environment.ActiveSquallCount >= desiredCount)
            {
                Reset();
                return;
            }
            var interval = Mathf.Lerp(settings.MaximumSpawnIntervalSeconds,
                settings.MinimumSpawnIntervalSeconds, activity);
            formationProgress = Mathf.Min(1f, formationProgress + deltaTime / interval);
            // At most one spawn per update; a long frame cannot create a burst.
            if (formationProgress >= 1f && environment.SpawnSquall()) Reset();
        }

        public static float WeatherActivity(WorldWeatherState weather, float minimumWeatherActivity)
        {
            var cloud = Mathf.Clamp01(weather.Clouds.Coverage);
            var wind = Mathf.Clamp01(weather.WindSpeedMetresPerSecond / 25f);
            var activity = cloud * .65f + wind * .35f;
            return Mathf.SmoothStep(0f, 1f,
                Mathf.InverseLerp(Mathf.Clamp01(minimumWeatherActivity), 1f, activity));
        }
    }
}
