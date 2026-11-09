using System;
using UnityEngine;

namespace Motu.World
{
    /// <summary>Travelling storm cells with a bounded active count. Distances use world metres.</summary>
    [Serializable]
    public struct SquallWeatherSettings
    {
        public const int MaximumSupportedSqualls = 3;
        public const float MinimumSupportedDiameterMetres = 200f;
        public const float MaximumSupportedDiameterMetres = 1000f;
        public const float MaximumSupportedSpeedMultiplier = 8f;
        [HideInInspector] public int Version;
        public bool Enabled;
        public bool AutomaticSpawning;
        [Range(1, MaximumSupportedSqualls)] public int MaximumActiveSqualls;
        [Range(MinimumSupportedDiameterMetres, MaximumSupportedDiameterMetres)] public float MinimumDiameterMetres;
        [Range(MinimumSupportedDiameterMetres, MaximumSupportedDiameterMetres)] public float MaximumDiameterMetres;
        [Min(.1f)] public float MinimumSpawnIntervalSeconds;
        [Min(.1f)] public float MaximumSpawnIntervalSeconds;
        [Min(10f)] public float MinimumSpawnDistanceMetres;
        [Min(10f)] public float MaximumSpawnDistanceMetres;
        [Range(0f, MaximumSupportedSpeedMultiplier)] public float MinimumSpeedMultiplier;
        [Range(0f, MaximumSupportedSpeedMultiplier)] public float MaximumSpeedMultiplier;
        [Min(1f)] public float SpeedVariationSeconds;
        [Tooltip("Smooth heading variation either side of the current downwind direction.")]
        [Range(0f, 45f)] public float MaximumHeadingDeviationDegrees;
        [Min(1f)] public float HeadingVariationSeconds;
        [Tooltip("Minimum lifetime. A storm remains alive while it can be encountered.")]
        [Min(5f)] public float LifetimeSeconds;
        [Min(0f)] public float FadeSeconds;
        [Range(0f, .5f)] public float EdgeFraction;
        [Range(1f, 5f)] public float WindMultiplier;
        [Min(2f)] public float CloudBaseMetres;
        [Min(5f)] public float CloudThicknessMetres;
        [Range(0f, 1f)] public float CloudDarkness;
        [Range(0f, 1.5f)] public float CloudFringeFraction;
        [Range(0f, 1f)] public float DistantOpacity;
        [Range(0f, 4f)] public float RainIntensity;
        [Tooltip("Strike interval at maximum travel speed. Frequency scales with speed, reaching zero when stopped.")]
        [Min(.2f)] public float LightningIntervalSeconds;
        [Range(0f, 20f)] public float LightningBrightness;
        [Min(100f)] public float HorizonStartMetres;
        [Min(101f)] public float VisibilityDistanceMetres;

        public static SquallWeatherSettings Default => new SquallWeatherSettings
        {
            Version = 3, Enabled = true, AutomaticSpawning = true, MaximumActiveSqualls = 3,
            MinimumDiameterMetres = MinimumSupportedDiameterMetres, MaximumDiameterMetres = MaximumSupportedDiameterMetres,
            MinimumSpawnIntervalSeconds = 45f, MaximumSpawnIntervalSeconds = 150f,
            MinimumSpawnDistanceMetres = 2400f, MaximumSpawnDistanceMetres = 3200f,
            MinimumSpeedMultiplier = 0f, MaximumSpeedMultiplier = MaximumSupportedSpeedMultiplier, SpeedVariationSeconds = 45f,
            MaximumHeadingDeviationDegrees = 45f, HeadingVariationSeconds = 90f,
            LifetimeSeconds = 600f, FadeSeconds = 6f,
            EdgeFraction = .25f, WindMultiplier = 4f,
            CloudBaseMetres = 60f, CloudThicknessMetres = 100f, CloudDarkness = .9f,
            CloudFringeFraction = .65f, DistantOpacity = .18f,
            RainIntensity = 1f, LightningIntervalSeconds = 2.5f, LightningBrightness = 5f,
            HorizonStartMetres = 1000f, VisibilityDistanceMetres = 4000f,
        };

        internal SquallWeatherSettings Validated()
        {
            var result = this;
            if (result.Version < 3) result.Upgrade();
            result.MaximumActiveSqualls = Mathf.Clamp(result.MaximumActiveSqualls, 1, MaximumSupportedSqualls);
            result.MinimumDiameterMetres = Clamp(MinimumDiameterMetres, MinimumSupportedDiameterMetres, MaximumSupportedDiameterMetres);
            result.MaximumDiameterMetres = Clamp(MaximumDiameterMetres, result.MinimumDiameterMetres, MaximumSupportedDiameterMetres);
            result.MinimumSpawnIntervalSeconds = Clamp(MinimumSpawnIntervalSeconds, .1f, 3600f);
            result.MaximumSpawnIntervalSeconds = Clamp(MaximumSpawnIntervalSeconds, result.MinimumSpawnIntervalSeconds, 3600f);
            result.HorizonStartMetres = Clamp(HorizonStartMetres, 100f, 100000f);
            result.VisibilityDistanceMetres = Clamp(VisibilityDistanceMetres, result.HorizonStartMetres + 1f, 200000f);
            float spawnLimit = result.VisibilityDistanceMetres * .85f;
            result.MinimumSpawnDistanceMetres = Clamp(MinimumSpawnDistanceMetres, 10f, spawnLimit);
            result.MaximumSpawnDistanceMetres = Clamp(MaximumSpawnDistanceMetres, result.MinimumSpawnDistanceMetres, spawnLimit);
            result.MinimumSpeedMultiplier = Clamp(result.MinimumSpeedMultiplier, 0f, MaximumSupportedSpeedMultiplier);
            result.MaximumSpeedMultiplier = Clamp(result.MaximumSpeedMultiplier, result.MinimumSpeedMultiplier, MaximumSupportedSpeedMultiplier);
            result.SpeedVariationSeconds = Clamp(result.SpeedVariationSeconds, 1f, 600f);
            result.MaximumHeadingDeviationDegrees = Clamp(result.MaximumHeadingDeviationDegrees, 0f, 45f);
            result.HeadingVariationSeconds = Clamp(result.HeadingVariationSeconds, 1f, 600f);
            result.LifetimeSeconds = Clamp(LifetimeSeconds, 5f, 3600f);
            result.FadeSeconds = Clamp(FadeSeconds, 0f, result.LifetimeSeconds * .45f);
            result.EdgeFraction = Clamp(EdgeFraction, 0f, .5f);
            result.WindMultiplier = Clamp(WindMultiplier, 1f, 5f);
            result.CloudBaseMetres = Clamp(CloudBaseMetres, 2f, 500f);
            result.CloudThicknessMetres = Clamp(CloudThicknessMetres, 5f, 1000f);
            result.CloudDarkness = Clamp(CloudDarkness, 0f, 1f);
            result.CloudFringeFraction = Clamp(result.CloudFringeFraction, 0f, 1.5f);
            result.DistantOpacity = Clamp(result.DistantOpacity, 0f, 1f);
            result.RainIntensity = Clamp(RainIntensity, 0f, 4f);
            result.LightningIntervalSeconds = Clamp(LightningIntervalSeconds, .2f, 60f);
            result.LightningBrightness = Clamp(LightningBrightness, 0f, 20f);
            return result;
        }

        private void Upgrade()
        {
            var defaults = Default;
            if (Version == 0)
            {
                MinimumSpeedMultiplier = defaults.MinimumSpeedMultiplier;
                MaximumSpeedMultiplier = defaults.MaximumSpeedMultiplier;
                SpeedVariationSeconds = defaults.SpeedVariationSeconds;
                CloudFringeFraction = defaults.CloudFringeFraction;
                DistantOpacity = defaults.DistantOpacity;
            }
            if (Version < 2 && MaximumActiveSqualls <= 0) MaximumActiveSqualls = defaults.MaximumActiveSqualls;
            MaximumHeadingDeviationDegrees = defaults.MaximumHeadingDeviationDegrees;
            HeadingVariationSeconds = defaults.HeadingVariationSeconds;
            Version = defaults.Version;
        }

        private static float Clamp(float value, float minimum, float maximum)
        {
            WeatherValueValidation.RequireFinite(value, nameof(SquallWeatherSettings));
            return Mathf.Clamp(value, minimum, maximum);
        }
    }
}
