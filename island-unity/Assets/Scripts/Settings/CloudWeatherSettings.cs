using UnityEngine;

/// <summary>Runtime cloud appearance. Weather-map seed and resolution remain authored settings.</summary>
public struct CloudWeatherSettings
{
    public bool Enabled;
    public float Coverage;
    public float Density;
    public float AltitudeMetres;
    public float VerticalThicknessMetres;
    public float WorldSizeMetres;
    public float BroadNoiseScale;
    public float BroadNoiseStrength;
    public float DetailStrength;
    public float ErosionStrength;
    public Color DayColour;
    public Color SunsetColour;
    public Color NightColour;
    public float ShadowStrength;
    public float AmbientShadowStrength;
    public float CelestialObscurationStrength;
    public float LowElevationShadowFade;

    public static CloudWeatherSettings Default => FromSettings(new IslandCloudSettings());

    public static CloudWeatherSettings FromSettings(IslandCloudSettings settings)
    {
        if (settings == null) throw new System.ArgumentNullException(nameof(settings));
        return new CloudWeatherSettings
        {
            Enabled = settings.Enabled,
            Coverage = settings.Coverage,
            Density = settings.Density,
            AltitudeMetres = settings.AltitudeMetres,
            VerticalThicknessMetres = settings.VerticalThicknessMetres,
            WorldSizeMetres = settings.WorldSizeMetres,
            BroadNoiseScale = settings.BroadNoiseScale,
            BroadNoiseStrength = settings.BroadNoiseStrength,
            DetailStrength = settings.DetailStrength,
            ErosionStrength = settings.ErosionStrength,
            DayColour = settings.DayColour,
            SunsetColour = settings.SunsetColour,
            NightColour = settings.NightColour,
            ShadowStrength = settings.ShadowStrength,
            AmbientShadowStrength = settings.AmbientShadowStrength,
            CelestialObscurationStrength = settings.CelestialObscurationStrength,
            LowElevationShadowFade = settings.LowElevationShadowFade,
        }.Validated();
    }

    internal CloudWeatherSettings Validated()
    {
        WeatherValueValidation.RequireFinite(Coverage, nameof(Coverage));
        WeatherValueValidation.RequireFinite(Density, nameof(Density));
        WeatherValueValidation.RequireFinite(AltitudeMetres, nameof(AltitudeMetres));
        WeatherValueValidation.RequireFinite(VerticalThicknessMetres, nameof(VerticalThicknessMetres));
        WeatherValueValidation.RequireFinite(WorldSizeMetres, nameof(WorldSizeMetres));
        WeatherValueValidation.RequireFinite(BroadNoiseScale, nameof(BroadNoiseScale));
        WeatherValueValidation.RequireFinite(BroadNoiseStrength, nameof(BroadNoiseStrength));
        WeatherValueValidation.RequireFinite(DetailStrength, nameof(DetailStrength));
        WeatherValueValidation.RequireFinite(ErosionStrength, nameof(ErosionStrength));
        WeatherValueValidation.RequireFinite(DayColour, nameof(DayColour));
        WeatherValueValidation.RequireFinite(SunsetColour, nameof(SunsetColour));
        WeatherValueValidation.RequireFinite(NightColour, nameof(NightColour));
        WeatherValueValidation.RequireFinite(ShadowStrength, nameof(ShadowStrength));
        WeatherValueValidation.RequireFinite(AmbientShadowStrength, nameof(AmbientShadowStrength));
        WeatherValueValidation.RequireFinite(CelestialObscurationStrength, nameof(CelestialObscurationStrength));
        WeatherValueValidation.RequireFinite(LowElevationShadowFade, nameof(LowElevationShadowFade));
        var result = this;
        result.Coverage = Mathf.Clamp01(Coverage);
        result.Density = Mathf.Clamp(Density, 0f, 8f);
        result.AltitudeMetres = Mathf.Clamp(AltitudeMetres, 50f, 1800f);
        result.VerticalThicknessMetres = Mathf.Clamp(VerticalThicknessMetres, 25f, 1000f);
        result.WorldSizeMetres = Mathf.Clamp(WorldSizeMetres, 100f, 8000f);
        result.BroadNoiseScale = Mathf.Clamp(BroadNoiseScale, 2f, 16f);
        result.BroadNoiseStrength = Mathf.Clamp01(BroadNoiseStrength);
        result.DetailStrength = Mathf.Clamp01(DetailStrength);
        result.ErosionStrength = Mathf.Clamp01(ErosionStrength);
        result.ShadowStrength = Mathf.Clamp01(ShadowStrength);
        result.AmbientShadowStrength = Mathf.Clamp(AmbientShadowStrength, 0f, 0.5f);
        result.CelestialObscurationStrength = Mathf.Clamp(CelestialObscurationStrength, 0f, 2f);
        result.LowElevationShadowFade = Mathf.Clamp(LowElevationShadowFade, 0.01f, 0.35f);
        return result;
    }
}
