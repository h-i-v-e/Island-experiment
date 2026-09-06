using System;
using UnityEngine;

[Serializable]
public sealed class WorldEnvironmentSettings
{
    [Tooltip("Seed for global sky and weather variation. Island seeds belong to the request factory.")]
    [SerializeField] private int seed = 8675309;
    [SerializeField] private Color zenithColour = new Color(0.49f, 0.68f, 0.82f, 1f);
    [SerializeField] private Color distanceHazeColour = new Color(0.62f, 0.60f, 0.54f, 1f);
    [Range(0.00005f, 0.003f)]
    [SerializeField] private float distanceHazeDensity = 0.00055f;
    [SerializeField] private bool showDistanceHaze = true;
    [SerializeField] private bool showSea = true;
    [SerializeField] private float seaLevelMetres;
    [SerializeField] private Material seaMaterial;
    [Tooltip("Optional global coherent noise shared by ocean waves and vegetation wind. Red and green drive structure; blue supplies broad grass colour variation. A suitable texture is generated when unset.")]
    [SerializeField] private Texture2D weatherNoise;
    [SerializeField] private OceanWaveProfile oceanWaveProfile;
    [Tooltip("Global horizontal wind direction in world X/Z coordinates.")]
    [SerializeField] private Vector2 windDirection = new Vector2(1f, 0.25f);
    [Tooltip("Global wind speed in metres per second. This drives clouds, vegetation, and ocean waves.")]
    [Range(0f, 40f)]
    [SerializeField] private float windSpeedMetresPerSecond = 9f;
    [SerializeField] private Light sunlight;
    [Range(0.25f, 240f)] [SerializeField] private float sunCycleDurationMinutes = 20f;
    [Range(1f, 20f)] [SerializeField] private float midnightToNoonClockRateRatio = 10f;
    [Range(-80f, 80f)] [SerializeField] private float sunLatitudeDegrees = -36f;
    [Range(0f, 24f)] [SerializeField] private float startingSolarTimeHours = 8f;
    [Range(0f, 4f)] [SerializeField] private float middaySunIntensity = 1.25f;
    [Range(0f, 45f)] [SerializeField] private float moonEquatorOffsetDegrees = 22f;
    [Range(0f, 1f)] [SerializeField] private float startingMoonPhase = 0.5f;
    [Range(0f, 1f)] [SerializeField] private float fullMoonLightIntensity = 0.14f;
    [Range(0f, 1f)] [SerializeField] private float starDensity = 0.18f;
    [Range(0f, 4f)] [SerializeField] private float starBrightness = 1.35f;
    [Range(0.02f, 0.12f)] [SerializeField] private float starSize = 0.052f;

    public int Seed => seed;
    public Color ZenithColour => zenithColour;
    public Color DistanceHazeColour => distanceHazeColour;
    public float DistanceHazeDensity => Mathf.Clamp(distanceHazeDensity, 0.00005f, 0.003f);
    public bool ShowDistanceHaze => showDistanceHaze;
    public bool ShowSea => showSea;
    public float SeaLevelMetres => float.IsFinite(seaLevelMetres) ? seaLevelMetres : 0f;
    public Material SeaMaterial => seaMaterial;
    public Texture2D WeatherNoise => weatherNoise;
    public OceanWaveProfile OceanWaveProfile => oceanWaveProfile;
    public Vector2 WindDirection
    {
        get => windDirection;
        set => windDirection = value;
    }
    public float WindSpeedMetresPerSecond
    {
        get => Mathf.Clamp(windSpeedMetresPerSecond, 0f, 40f);
        set => windSpeedMetresPerSecond = Mathf.Clamp(value, 0f, 40f);
    }
    public Light Sunlight { get => sunlight; internal set => sunlight = value; }
    public float SunCycleDurationMinutes => Mathf.Clamp(sunCycleDurationMinutes, 0.25f, 240f);
    public float MidnightToNoonClockRateRatio => Mathf.Clamp(midnightToNoonClockRateRatio, 1f, 20f);
    public float SunLatitudeDegrees => Mathf.Clamp(sunLatitudeDegrees, -80f, 80f);
    public float StartingSolarTimeHours => Mathf.Repeat(startingSolarTimeHours, 24f);
    public float MiddaySunIntensity => Mathf.Clamp(middaySunIntensity, 0f, 4f);
    public float MoonEquatorOffsetDegrees => Mathf.Clamp(moonEquatorOffsetDegrees, 0f, 45f);
    public float StartingMoonPhase => Mathf.Repeat(startingMoonPhase, 1f);
    public float FullMoonLightIntensity => Mathf.Clamp01(fullMoonLightIntensity);
    public float StarDensity => Mathf.Clamp01(starDensity);
    public float StarBrightness => Mathf.Clamp(starBrightness, 0f, 4f);
    public float StarSize => Mathf.Clamp(starSize, 0.02f, 0.12f);

    internal void AssignSceneReferences(Light sun, Material sea)
    {
        sunlight = sun;
        seaMaterial = sea;
    }
}
