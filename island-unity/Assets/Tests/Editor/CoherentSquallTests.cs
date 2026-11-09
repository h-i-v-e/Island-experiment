using Motu.Gameplay;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class CoherentSquallTests
    {
        [Test]
        public void RealCoherentWeatherDriverOwnsFormationAndTheLiveCellLimit()
        {
            var baseline = new EnvironmentHostState();
            var root = new GameObject("Coherent squall driver integration");
            var target = new GameObject("Coherent squall target");
            try
            {
                var environment = root.AddComponent<WorldEnvironmentController>();
                environment.Initialize(new WorldEnvironmentSettings().WithSeed(319), new IslandCloudSettings(), 64, 128, target.transform);
                environment.SquallSpawnTarget = target.transform;
                var driver = root.AddComponent<CoherentWeatherDriver>();
                environment.WeatherDriver = driver;
                var settings = SquallWeatherSettings.Default;
                settings.MinimumSpawnIntervalSeconds = settings.MaximumSpawnIntervalSeconds = .1f;
                settings.FadeSeconds = 0;
                driver.Squalls = settings;
                environment.UpdateWeatherDriver(0);
                Assert.That(driver.ControlsSquallSpawning, Is.True);
                Assert.That(environment.Weather.Squalls.MaximumActiveSqualls, Is.EqualTo(3));
                driver.SquallFormationThreshold = 1;
                var weather = environment.Weather;
                weather.Clouds.Coverage = 0; weather.WindSpeedMetresPerSecond = 0;
                environment.ApplyWeather(weather);
                environment.AdvanceSqualls(100);
                Assert.That(environment.ActiveSquallCount, Is.Zero, "The environment timer must defer to coherent formation.");
                driver.SquallFormationThreshold = 0;
                environment.AdvanceSqualls(100);
                Assert.That(environment.ActiveSquallCount, Is.Zero, "A zero threshold still requires actual weather activity.");
                weather.Clouds.Coverage = 1; weather.WindSpeedMetresPerSecond = 25;
                environment.ApplyWeather(weather);
                for (var i = 0; i < 3; i++) environment.AdvanceSqualls(.11f);
                Assert.That(environment.ActiveSquallCount, Is.EqualTo(3));
                settings.MaximumActiveSqualls = 1;
                driver.Squalls = settings;
                environment.UpdateWeatherDriver(0);
                Assert.That(environment.ActiveSquallCount, Is.EqualTo(1));
                driver.enabled = false;
                environment.ClearSquall();
                environment.AdvanceSqualls(100);
                Assert.That(environment.ActiveSquallCount, Is.Zero);
            }
            finally
            {
                Object.DestroyImmediate(root); Object.DestroyImmediate(target);
                baseline.Restore();
            }
        }

        [Test]
        public void SquallActivityFollowsTheSameSeededChangingWeatherAsCloudsAndWind()
        {
            var root = new GameObject("Coherent weather activity");
            try
            {
                var driver = root.AddComponent<CoherentWeatherDriver>();
                var weather = WorldWeatherState.FromEnvironment(new WorldEnvironmentSettings());
                var minimum = 1f;
                var maximum = 0f;
                for (var time = 0; time < 60000; time += 60)
                {
                    driver.UpdateWeather(ref weather, 60);
                    var activity = driver.GetSquallActivity(weather);
                    var severity = Mathf.Clamp01(weather.Clouds.Coverage) * .65f
                        + Mathf.Clamp01(weather.WindSpeedMetresPerSecond / 25f) * .35f;
                    var expected = Mathf.SmoothStep(0, 1,
                        Mathf.InverseLerp(driver.SquallFormationThreshold, 1, severity));
                    Assert.That(activity, Is.EqualTo(expected).Within(.00001f));
                    minimum = Mathf.Min(minimum, activity);
                    maximum = Mathf.Max(maximum, activity);
                }
                Assert.That(minimum, Is.LessThan(.01f), "Fair portions of the weather cycle suppress squalls.");
                Assert.That(maximum, Is.GreaterThan(.3f), "Worsening weather increases squall support.");
            }
            finally { Object.DestroyImmediate(root); }
        }
    }
}
