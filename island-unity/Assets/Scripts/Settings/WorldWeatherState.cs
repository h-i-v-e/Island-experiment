using UnityEngine;
using Motu.Settings;

namespace Motu.World
{
    /// <summary>Editable ocean behaviour. Mesh and mask layout remain in the ocean profile.</summary>
    public struct OceanWaveWeatherSettings
    {
        public bool Enabled;
        public float DepthAllowancePower;
        public float DistanceAllowancePower;
        public float NoiseWorldSizeMetres;
        public float DomainWarpMetres;
        public float AmplitudeVariation;
        public Color WhitecapColour;
        public float WhitecapStrength;
        public float WhitecapHeightThreshold;
        public float WhitecapSlopeThreshold;
        public float WhitecapCoverage;
        public float WhitecapNoiseWorldSizeMetres;
        public float WhitecapFineNoiseScale;
        public float WhitecapCounterflowSpeed;
        public bool OnshoreWaveEnabled;
        public float OnshoreWaveWavelengthMetres;
        public float OnshoreWaveAmplitudeMetres;
        public float OnshoreWaveSpeedMetresPerSecond;
        public float OnshoreWaveChoppiness;
        public float OnshoreWaveLeadingEdgeSharpness;
        public float OnshoreWaveSharpeningDistanceMetres;
        public float OnshoreWaveBreakingStartDepthMetres;
        public float OnshoreWaveBreakingFullDepthMetres;
        public OceanWaveComponent Wave0;
        public OceanWaveComponent Wave1;
        public OceanWaveComponent Wave2;
        public OceanWaveComponent Wave3;

        internal void ValidateFinite()
        {
            WeatherValueValidation.RequireFinite(DepthAllowancePower, nameof(DepthAllowancePower));
            WeatherValueValidation.RequireFinite(DistanceAllowancePower, nameof(DistanceAllowancePower));
            WeatherValueValidation.RequireFinite(NoiseWorldSizeMetres, nameof(NoiseWorldSizeMetres));
            WeatherValueValidation.RequireFinite(DomainWarpMetres, nameof(DomainWarpMetres));
            WeatherValueValidation.RequireFinite(AmplitudeVariation, nameof(AmplitudeVariation));
            WeatherValueValidation.RequireFinite(WhitecapColour, nameof(WhitecapColour));
            WeatherValueValidation.RequireFinite(WhitecapStrength, nameof(WhitecapStrength));
            WeatherValueValidation.RequireFinite(WhitecapHeightThreshold, nameof(WhitecapHeightThreshold));
            WeatherValueValidation.RequireFinite(WhitecapSlopeThreshold, nameof(WhitecapSlopeThreshold));
            WeatherValueValidation.RequireFinite(WhitecapCoverage, nameof(WhitecapCoverage));
            WeatherValueValidation.RequireFinite(WhitecapNoiseWorldSizeMetres, nameof(WhitecapNoiseWorldSizeMetres));
            WeatherValueValidation.RequireFinite(WhitecapFineNoiseScale, nameof(WhitecapFineNoiseScale));
            WeatherValueValidation.RequireFinite(WhitecapCounterflowSpeed, nameof(WhitecapCounterflowSpeed));
            WeatherValueValidation.RequireFinite(OnshoreWaveWavelengthMetres, nameof(OnshoreWaveWavelengthMetres));
            WeatherValueValidation.RequireFinite(OnshoreWaveAmplitudeMetres, nameof(OnshoreWaveAmplitudeMetres));
            WeatherValueValidation.RequireFinite(OnshoreWaveSpeedMetresPerSecond, nameof(OnshoreWaveSpeedMetresPerSecond));
            WeatherValueValidation.RequireFinite(OnshoreWaveChoppiness, nameof(OnshoreWaveChoppiness));
            WeatherValueValidation.RequireFinite(OnshoreWaveLeadingEdgeSharpness, nameof(OnshoreWaveLeadingEdgeSharpness));
            WeatherValueValidation.RequireFinite(OnshoreWaveSharpeningDistanceMetres, nameof(OnshoreWaveSharpeningDistanceMetres));
            WeatherValueValidation.RequireFinite(OnshoreWaveBreakingStartDepthMetres, nameof(OnshoreWaveBreakingStartDepthMetres));
            WeatherValueValidation.RequireFinite(OnshoreWaveBreakingFullDepthMetres, nameof(OnshoreWaveBreakingFullDepthMetres));
            Wave0.ValidateFinite(nameof(Wave0));
            Wave1.ValidateFinite(nameof(Wave1));
            Wave2.ValidateFinite(nameof(Wave2));
            Wave3.ValidateFinite(nameof(Wave3));
        }

        public static OceanWaveWeatherSettings Default => OceanWaveRuntimeSettings.Default.Weather;
    }

    /// <summary>A value snapshot of the world's runtime wind, wave and cloud controls.</summary>
    public struct WorldWeatherState
    {
        public Vector2 WindDirection;
        public float WindSpeedMetresPerSecond;
        public float WindGustSizeMetres;
        public float TreeWindBasePinHeightMetres;
        public float TreeWindFullBendHeightMetres;
        public OceanWaveWeatherSettings Waves;
        public CloudWeatherSettings Clouds;

        public static WorldWeatherState FromEnvironment(
            WorldEnvironmentSettings settings, IslandCloudSettings clouds = null)
        {
            if (settings == null) throw new System.ArgumentNullException(nameof(settings));
            return new WorldWeatherState
            {
                Clouds = clouds != null ? CloudWeatherSettings.FromSettings(clouds) : CloudWeatherSettings.Default,
                WindDirection = settings.WindDirection,
                WindSpeedMetresPerSecond = settings.WindSpeedMetresPerSecond,
                WindGustSizeMetres = settings.WindGustSizeMetres,
                TreeWindBasePinHeightMetres = settings.TreeWindBasePinHeightMetres,
                TreeWindFullBendHeightMetres = settings.TreeWindFullBendHeightMetres,
                Waves = settings.OceanWaveProfile != null
                    ? settings.OceanWaveProfile.ToRuntimeSettings().Weather
                    : OceanWaveWeatherSettings.Default,
            }.Validated();
        }

        internal WorldWeatherState Validated()
        {
            WeatherValueValidation.RequireFinite(WindDirection, nameof(WindDirection));
            WeatherValueValidation.RequireFinite(WindSpeedMetresPerSecond, nameof(WindSpeedMetresPerSecond));
            WeatherValueValidation.RequireFinite(WindGustSizeMetres, nameof(WindGustSizeMetres));
            WeatherValueValidation.RequireFinite(TreeWindBasePinHeightMetres, nameof(TreeWindBasePinHeightMetres));
            WeatherValueValidation.RequireFinite(TreeWindFullBendHeightMetres, nameof(TreeWindFullBendHeightMetres));
            var result = this;
            result.WindDirection = WindDirection.sqrMagnitude > 0.000001f
                ? WindDirection.normalized : Vector2.right;
            result.WindSpeedMetresPerSecond = Mathf.Clamp(WindSpeedMetresPerSecond, 0f, 40f);
            result.WindGustSizeMetres = Mathf.Clamp(WindGustSizeMetres, 1f, 64f);
            result.TreeWindBasePinHeightMetres = Mathf.Clamp(TreeWindBasePinHeightMetres, 0f, 4f);
            result.TreeWindFullBendHeightMetres = Mathf.Clamp(
                TreeWindFullBendHeightMetres,
                Mathf.Max(result.TreeWindBasePinHeightMetres + 0.01f, 1f),
                24f);
            result.Waves = OceanWaveRuntimeSettings.Default.WithWeather(Waves).Weather;
            result.Clouds = Clouds.Validated();
            return result;
        }
    }

    internal static class WeatherValueValidation
    {
        internal static void RequireFinite(float value, string name)
        {
            if (!float.IsFinite(value))
            {
                throw new System.ArgumentOutOfRangeException(
                    name, value, "Runtime weather values must be finite.");
            }
        }

        internal static void RequireFinite(Vector2 value, string name)
        {
            RequireFinite(value.x, name);
            RequireFinite(value.y, name);
            RequireFinite(value.sqrMagnitude, name);
        }

        internal static void RequireFinite(Color value, string name)
        {
            RequireFinite(value.r, name);
            RequireFinite(value.g, name);
            RequireFinite(value.b, name);
            RequireFinite(value.a, name);
        }
    }
}
