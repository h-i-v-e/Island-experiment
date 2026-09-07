using UnityEngine;

/// <summary>
/// Assign a component derived from this class to the world's Weather Driver slot.
/// It runs on the main thread before the environment applies wind, waves and clouds.
/// </summary>
public abstract class WorldWeatherDriver : MonoBehaviour
{
    /// <summary>
    /// Edit the current runtime state in place. deltaTime uses unscaled seconds,
    /// matching the environment clock. Disabled drivers leave the last state active.
    /// </summary>
    public abstract void UpdateWeather(ref WorldWeatherState weather, float deltaTime);
}
