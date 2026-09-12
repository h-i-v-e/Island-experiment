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

        private DirectionAndSpeed ReadWind(float delta)
        {
            return new DirectionAndSpeed(DirectionFromNoise(directionOffset + delta), Mathf.Clamp01(Mathf.PerlinNoise1D(strengthOffset + delta)));
        }

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

        private static Vector2 DirectionFromNoise(float time)
        {
            return Rotate(Vector2.up, Mathf.PerlinNoise1D(time) * 2f * Mathf.PI);
        }

        private static void UpdateWave(ref OceanWaveComponent component, DirectionAndSpeed dAndP, float scale, float sharpOffset)
        {
            var speed = dAndP.speed * dAndP.speed;
            component.AmplitudeMetres = speed * scale;
            component.Direction = dAndP.direction;
            component.Choppiness = Mathf.Lerp(sharpOffset, 1f, speed);
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
            var fine = ReadWind(125f + elapsedSeconds * 0.0125f);
            var minor = ReadWind(25f + elapsedSeconds * 0.0025f);
            var major = ReadWind(5f + elapsedSeconds * 0.0005f);
            var dominant = ReadWind(elapsedSeconds * 0.0001f);
            weather.WindDirection = dominant.direction;
            var windspeed = dominant.speed * dominant.speed;
            weather.WindSpeedMetresPerSecond = windspeed * 40f;
            weather.Waves.OnshoreWaveAmplitudeMetres = windspeed * 5f;
            UpdateWave(ref weather.Waves.Wave0, dominant, 5f, 0.5f);
            UpdateWave(ref weather.Waves.Wave1, major, 3f, 0f);
            UpdateWave(ref weather.Waves.Wave2, minor, 0.4f, 0.25f);
            UpdateWave(ref weather.Waves.Wave3, fine, 0.3f, 0f);
            UpdateClouds(ref weather.Clouds, elapsedSeconds);
        }
    }
}
