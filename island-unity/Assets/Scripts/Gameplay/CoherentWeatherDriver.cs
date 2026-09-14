using UnityEngine;
using Motu.Settings;
using Motu.World;
using System;

namespace Motu.Gameplay
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "CoherentWeatherDriver")]
    public class CoherentWeatherDriver : WorldWeatherDriver
    {
        private float strengthOffset = 0.0f;
        private float directionOffset = 0.0f;
        private float elapsedSeconds;

        private float cloudOffset;

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

        /*private DirectionAndSpeed ReadWind(float delta)
        {
            return new DirectionAndSpeed(DirectionFromNoise(directionOffset + delta), Mathf.Clamp01(Mathf.PerlinNoise1D(strengthOffset + delta)));
        }*/

        private void Awake() => ResetSequence();

        internal void ResetSequence()
        {
            // Huge float coordinates make Perlin noise overflow and cannot retain
            // small time increments. Keep the seeded starting coordinates local.
            var random = new System.Random(seed);
            strengthOffset = (float)random.NextDouble() * 1024f;
            directionOffset = (float)random.NextDouble() * 1024f;
            cloudOffset = (float)random.NextDouble() * 1024f;
            elapsedSeconds = 0f;
        }

        private static Vector2 Rotate(Vector2 v, float delta) {
            return new Vector2(
                v.x * Mathf.Cos(delta) - v.y * Mathf.Sin(delta),
                v.x * Mathf.Sin(delta) + v.y * Mathf.Cos(delta)
            );
        }

        private Vector2 DirectionFromNoise(float time)
        {
            return Rotate(Vector2.up, Mathf.PerlinNoise1D(directionOffset + time) * 2f * Mathf.PI);
        }

        private static void UpdateWave(ref OceanWaveComponent component, float windspeed, Vector2 direction, float dp)
        {
            component.Direction = direction;
            component.Choppiness = Mathf.Pow(windspeed, 0.5f) * dp;
        }

        private static void UpdateWave(ref OceanWaveComponent component, float windspeed, Vector2 dominantDirecton, Vector2 direction)
        {
            UpdateWave(ref component, windspeed, direction, Mathf.Abs(Vector2.Dot(dominantDirecton, direction)));
        }

        private void UpdateClouds(ref CloudWeatherSettings clouds, float time)
        {
            var density = Mathf.PerlinNoise1D(cloudOffset + time * 0.005f);
            clouds.Density = density * 8f;
            clouds.Coverage = (density + Mathf.PerlinNoise1D(cloudOffset + time * 0.001f)) * 0.5f;
            clouds.ShadowStrength = density;
        }

        public override void UpdateWeather(ref WorldWeatherState weather, float deltaTime)
        {
            elapsedSeconds += Mathf.Max(deltaTime, 0f);
            var fine = DirectionFromNoise(125f + elapsedSeconds * 0.0125f);
            var minor = DirectionFromNoise(25f + elapsedSeconds * 0.0025f);
            var major = DirectionFromNoise(5f + elapsedSeconds * 0.0005f);
            var offset = elapsedSeconds * 0.0001f;
            var dominant = DirectionFromNoise(offset);
            weather.WindDirection = dominant;
            var windspeed = Mathf.Clamp01(Mathf.PerlinNoise1D(strengthOffset + offset));
            weather.WindSpeedMetresPerSecond = windspeed * windspeed * 20f;
            weather.Waves.OnshoreWaveAmplitudeMetres = 4f;
            UpdateWave(ref weather.Waves.Wave0, windspeed, dominant, 1f);
            UpdateWave(ref weather.Waves.Wave1, windspeed, dominant, major);
            UpdateWave(ref weather.Waves.Wave2, windspeed, dominant, minor);
            UpdateWave(ref weather.Waves.Wave3, windspeed, dominant, fine);
            UpdateClouds(ref weather.Clouds, elapsedSeconds);
        }
    }
}
