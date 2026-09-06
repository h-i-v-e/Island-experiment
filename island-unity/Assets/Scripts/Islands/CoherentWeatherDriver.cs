using UnityEngine;

public class CoherentWeatherDriver : WorldWeatherDriver
{
    private float strengthOffset = 0.0f;
    private float directionOffset = 0.0f;
    private float elapsedSeconds;

    [SerializeField]
    private int seed = 42;

    readonly struct DirectionAndSpeed
    {
        internal readonly Vector2 direction;
        internal readonly float speed;

        public DirectionAndSpeed(Vector2 direction, float speed)
        {
            this.direction = direction;
            this.speed = speed;
        }
    }

    private DirectionAndSpeed ReadWind(float delta)
    {
        var speed = Mathf.Clamp01(Mathf.PerlinNoise1D(strengthOffset + delta));
        return new DirectionAndSpeed(DirectionFromNoise(directionOffset + delta), speed * speed * 20f);
    }

    private void Awake()
    {
        // Huge float coordinates make Perlin noise overflow and cannot retain
        // small time increments. Keep the seeded starting coordinates local.
        var random = new System.Random(seed);
        strengthOffset = (float)random.NextDouble() * 1024f;
        directionOffset = (float)random.NextDouble() * 1024f;
        elapsedSeconds = 0f;
    }

    private static Vector2 Rotate(Vector2 v, float delta) {
        return new Vector2(
            v.x * Mathf.Cos(delta) - v.y * Mathf.Sin(delta),
            v.x * Mathf.Sin(delta) + v.y * Mathf.Cos(delta)
        );
    }

    private static Vector2 DirectionFromNoise(float deltaTime)
    {
        return Rotate(Vector2.up, Mathf.PerlinNoise1D(deltaTime) * 2f * Mathf.PI);
    }

    public override void UpdateWeather(ref WorldWeatherState weather, float deltaTime)
    {
        elapsedSeconds += Mathf.Max(deltaTime, 0f);
        var fine = ReadWind(elapsedSeconds * 0.01f);
        var minor = ReadWind(elapsedSeconds * 0.005f);
        var major = ReadWind(elapsedSeconds * 0.001f);
        var dominant = ReadWind(elapsedSeconds * 0.0005f);
        weather.WindDirection = dominant.direction;
        weather.Waves.OnshoreWaveAmplitudeMetres = dominant.speed * 3.2f;
        weather.Waves.Wave0.AmplitudeMetres = dominant.speed * 3.2f;
        weather.Waves.Wave1.AmplitudeMetres = major.speed * 1.6f;
        weather.Waves.Wave1.Direction = major.direction;
        weather.Waves.Wave2.AmplitudeMetres = minor.speed * 0.4f;
        weather.Waves.Wave2.Direction = minor.direction;
        weather.Waves.Wave3.AmplitudeMetres = fine.speed * 0.1f;
    }
}
