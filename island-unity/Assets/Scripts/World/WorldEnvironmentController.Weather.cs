using System;

public sealed partial class WorldEnvironmentController
{
    private WorldWeatherState weather;

    public WorldWeatherState Weather => weather;
    public WorldWeatherDriver WeatherDriver { get; set; }

    /// <summary>Apply runtime wind and wave behaviour without changing authored assets or mesh layout.</summary>
    public void ApplyWeather(WorldWeatherState value)
    {
        if (environmentSettings == null)
        {
            throw new InvalidOperationException(
                "The world environment must be initialized before setting weather.");
        }
        weather = value.Validated();
        ocean.ApplyWaveWeather(weather.Waves);
        ApplyWeatherWind();
    }

    private void UpdateWeatherDriver(float deltaTime)
    {
        var driver = WeatherDriver;
        if (driver == null || !driver.isActiveAndEnabled)
        {
            return;
        }
        var updated = weather;
        driver.UpdateWeather(ref updated, deltaTime);
        ApplyWeather(updated);
    }
}
