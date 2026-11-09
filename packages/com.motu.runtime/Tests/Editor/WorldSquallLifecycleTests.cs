using System;
using System.Reflection;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldSquallLifecycleTests
    {
        private const BindingFlags Hidden = BindingFlags.Instance | BindingFlags.NonPublic;
        private GameObject root, target;
        private WorldEnvironmentController environment;
        private EnvironmentHostState baseline;
        private float timeScale;

        [SetUp]
        public void SetUp()
        {
            timeScale = Time.timeScale;
            baseline = new EnvironmentHostState();
            target = new GameObject("Squall lifecycle target");
            target.transform.position = new Vector3(430, 3, -210);
            root = new GameObject("Squall lifecycle environment");
            environment = root.AddComponent<WorldEnvironmentController>();
            environment.Initialize(new WorldEnvironmentSettings().WithSeed(7129), new IslandCloudSettings(), 64, 128, target.transform);
            environment.SquallSpawnTarget = target.transform;
            var weather = environment.Weather;
            weather.WindDirection = new Vector2(.6f, .8f);
            weather.WindSpeedMetresPerSecond = 9;
            var settings = SquallWeatherSettings.Default;
            settings.MaximumActiveSqualls = 1;
            settings.AutomaticSpawning = false;
            settings.FadeSeconds = 0;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
        }

        [TearDown]
        public void TearDown()
        {
            Time.timeScale = timeScale;
            if (environment != null)
            {
                Invoke("ReleaseEnvironment");
                Invoke("DisposeSqualls");
            }
            Object.DestroyImmediate(root);
            Object.DestroyImmediate(target);
            baseline?.Restore();
        }

        [Test]
        public void SeededHorizonSpawnsCoverAllBearingsAndNeverReplaceALiveCell()
        {
            var settings = environment.Weather.Squalls;
            int quadrants = 0;
            for (int i = 0; i < 24; i++)
            {
                Assert.IsTrue(environment.SpawnSquall());
                var cell = environment.Squall;
                var offset = cell.Center - target.transform.position;
                Assert.That(cell.DiameterMetres, Is.InRange(200f, 1000f));
                Assert.That(new Vector2(offset.x, offset.z).magnitude,
                    Is.InRange(settings.MinimumSpawnDistanceMetres - .001f, settings.MaximumSpawnDistanceMetres + .001f));
                quadrants |= 1 << ((offset.x >= 0 ? 1 : 0) + (offset.z >= 0 ? 2 : 0));
                Assert.IsFalse(environment.SpawnSquall(target.transform.position, 500));
                Assert.That(environment.Squall.Seed, Is.EqualTo(cell.Seed));
                Assert.That(environment.Squall.Center, Is.EqualTo(cell.Center));
                environment.ClearSquall();
            }
            Assert.That(quadrants, Is.EqualTo(15), "Random horizon bearings must include every quadrant.");
        }

        [Test]
        public void MaximumSizeSpawnsAFullThousandMetrePhysicalFootprint()
        {
            Assert.That(environment.Weather.Squalls.MinimumDiameterMetres, Is.EqualTo(200));
            Assert.That(environment.Weather.Squalls.MaximumDiameterMetres, Is.EqualTo(1000));
            Assert.That(environment.SpawnSquall(target.transform.position, 1000), Is.True);
            Assert.That(environment.Squall.DiameterMetres, Is.EqualTo(1000));
            Assert.That(environment.Squall.Field.RadiusMetres, Is.EqualTo(500));
        }

        [TestCase(100f, 200f)]
        [TestCase(1200f, 1000f)]
        public void ExplicitSpawnsRespectTheSupportedDiameterRange(float requested, float expected)
        {
            Assert.That(environment.SpawnSquall(target.transform.position, requested), Is.True);
            Assert.That(environment.Squall.DiameterMetres, Is.EqualTo(expected));
            Assert.That(environment.Squall.Field.RadiusMetres, Is.EqualTo(expected * .5f));
        }

        [Test]
        public void TopSpeedTravelsAtEightTimesAmbientWindAndKeepsPeakLocalWeather()
        {
            var weather = environment.Weather;
            weather.Squalls.MinimumSpeedMultiplier = weather.Squalls.MaximumSpeedMultiplier = 8;
            weather.Squalls.MaximumHeadingDeviationDegrees = 0;
            environment.ApplyWeather(weather);
            environment.SpawnSquall(target.transform.position, 1000);
            var before = environment.Squall.Center;
            environment.AdvanceSqualls(2);
            var cell = environment.Squall;
            Assert.That(Vector3.Distance(before, cell.Center), Is.EqualTo(144f).Within(.001f));
            Assert.That(cell.Field.Velocity.magnitude, Is.EqualTo(72f).Within(.001f));
            Assert.That(environment.WindSpeedAt(cell.Center), Is.EqualTo(9 * weather.Squalls.WindMultiplier).Within(.001f));
        }

        [Test]
        public void CoherentSpeedDrivesTravelAndLocalWeatherWithoutChangingAmbientWind()
        {
            Assert.IsTrue(environment.SpawnSquall(target.transform.position, 300));
            var before = environment.Squall;
            environment.AdvanceSqualls(2);
            var after = environment.Squall;
            var travel = WorldSquallMotion.Displacement(before.Seed, 0, 2, environment.Weather.Squalls);
            var offset = WorldSquallMotion.AlongWind(travel, environment.WindDirection) * 9;
            var expected = new Vector3(offset.x, 0, offset.y);
            Assert.That(Vector3.Distance(after.Center - before.Center, expected), Is.LessThan(.0002f));
            float gain = Mathf.Lerp(1, environment.Weather.Squalls.WindMultiplier,
                after.SpeedMultiplier / environment.Weather.Squalls.MaximumSpeedMultiplier);
            Assert.That(environment.WindSpeedAt(after.Center), Is.EqualTo(9 * gain).Within(.0001f));
            Assert.That(environment.WindSpeedAt(after.Center + Vector3.right * 301), Is.EqualTo(9));
            Assert.That(environment.WindSpeedMetresPerSecond, Is.EqualTo(9));
            var heading = WorldSquallMotion.AlongWind(WorldSquallMotion.Direction(after.Seed, after.AgeSeconds,
                environment.Weather.Squalls), environment.WindDirection);
            Assert.That(after.Field.Velocity, Is.EqualTo(heading * (9 * after.SpeedMultiplier)));
            Assert.That(Mathf.Abs(after.SpeedMultiplier - before.SpeedMultiplier), Is.LessThan(.1f));
            Assert.That(Shader.GetGlobalVector("_MotuSquall"), Is.EqualTo(after.Field.ShaderBounds));
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(after.Field.ShaderWind));
        }

        [Test]
        public void LiveSettingsRefreshWindAndDisableClearsBothGlobals()
        {
            Assert.IsTrue(environment.SpawnSquall(target.transform.position, 300));
            int seed = environment.Squall.Seed;
            var weather = environment.Weather;
            weather.WindDirection = Vector2.left;
            weather.WindSpeedMetresPerSecond = 5;
            var settings = weather.Squalls;
            settings.WindMultiplier = 3;
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 1;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            Assert.That(environment.Squall.Seed, Is.EqualTo(seed));
            Assert.That(environment.WindSpeedAt(environment.Squall.Center), Is.EqualTo(15));
            var heading = environment.Squall.Field.Velocity.normalized;
            Assert.That(Vector2.Angle(heading, Vector2.left), Is.LessThanOrEqualTo(45.001f));
            Assert.That(environment.Squall.Field.Velocity.magnitude, Is.EqualTo(5).Within(.00001f));
            settings.Enabled = false;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            Assert.IsFalse(environment.Squall.Active);
            Assert.IsFalse(environment.SpawnSquall());
            Assert.That(Shader.GetGlobalVector("_MotuSquall"), Is.EqualTo(Vector4.zero));
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(Vector4.zero));
            Assert.That(environment.WindSpeedAt(target.transform.position), Is.EqualTo(5));
        }

        [Test]
        public void ExpiryWaitsForNextScheduledSpawnAndClearAllowsANewCell()
        {
            var weather = environment.Weather;
            var settings = weather.Squalls;
            settings.LifetimeSeconds = 5;
            settings.AutomaticSpawning = true;
            settings.MinimumSpawnIntervalSeconds = settings.MaximumSpawnIntervalSeconds = .25f;
            weather.Squalls = settings;
            environment.ApplyWeather(weather);
            environment.ClearSquall();
            environment.AdvanceSqualls(.24f);
            Assert.IsFalse(environment.Squall.Active);
            environment.AdvanceSqualls(.02f);
            Assert.IsTrue(environment.Squall.Active);
            target.transform.position = environment.Squall.Center + Vector3.right * 10000;
            environment.AdvanceSqualls(5);
            Assert.IsFalse(environment.Squall.Active, "Expired cells must not be replaced in the same tick.");
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(Vector4.zero));
            environment.AdvanceSqualls(.26f);
            Assert.IsTrue(environment.Squall.Active);
            environment.ClearSquall();
            Assert.IsTrue(environment.SpawnSquall(target.transform.position, 100));
        }

        [Test]
        public void ShorterLiveLifetimeExpiresAnExistingCellWithoutRespawning()
        {
            Assert.IsTrue(environment.SpawnSquall(target.transform.position, 300));
            environment.AdvanceSqualls(8);
            var weather = environment.Weather;
            var settings = weather.Squalls;
            settings.LifetimeSeconds = 5;
            weather.Squalls = settings;
            target.transform.position = environment.Squall.Center + Vector3.right * 10000;
            environment.ApplyWeather(weather);
            environment.AdvanceSqualls(0);
            Assert.IsFalse(environment.Squall.Active);
            Assert.That(Shader.GetGlobalVector("_MotuSquall"), Is.EqualTo(Vector4.zero));
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(Vector4.zero));
        }

        [Test]
        public void ExpiredStormRemainsReachableAndKeepsWeatherWhileSailingThroughIt()
        {
            var weather = environment.Weather;
            weather.Squalls.LifetimeSeconds = 5;
            weather.Squalls.FadeSeconds = 1;
            weather.Squalls.MinimumSpeedMultiplier = weather.Squalls.MaximumSpeedMultiplier = 0;
            environment.ApplyWeather(weather);
            Assert.IsTrue(environment.SpawnSquall(target.transform.position + Vector3.forward * 1000, 300));
            environment.AdvanceSqualls(20);
            Assert.IsTrue(environment.Squall.Active);
            Assert.That(environment.Squall.Strength, Is.EqualTo(1));
            var center = environment.Squall.Center;
            target.transform.position = center;
            environment.AdvanceSqualls(10);
            Assert.That(environment.Squall.Center, Is.EqualTo(center));
            Assert.That(environment.Squall.Strength, Is.EqualTo(1), "Entering an old storm must not start its retirement fade.");
            target.transform.position = center + Vector3.forward * 10000;
            environment.AdvanceSqualls(1.1f);
            Assert.IsFalse(environment.Squall.Active, "Old storms may retire once they are beyond encounter range.");
        }

        [Test]
        public void NoiseIsSmoothBoundedAndTravelDoesNotDependOnFramePartitions()
        {
            var settings = environment.Weather.Squalls;
            float minimum = settings.MaximumSpeedMultiplier, maximum = 0, sum = 0;
            for (int i = 0; i < 6000; i++)
            {
                float age = i * .02f;
                float speed = WorldSquallMotion.Speed(7129, age, settings);
                minimum = Mathf.Min(minimum, speed); maximum = Mathf.Max(maximum, speed);
                Assert.That(speed, Is.InRange(0f, 8f));
                Assert.That(Mathf.Abs(speed - WorldSquallMotion.Speed(7129, age + .02f, settings)), Is.LessThan(.008f));
                sum += WorldSquallMotion.Travel(7129, age, (i + 1) * .02f, settings);
            }
            Assert.That(maximum - minimum, Is.GreaterThan(.5f));
            Assert.That(sum, Is.EqualTo(WorldSquallMotion.Travel(7129, 0, 120, settings)).Within(.01f));
            Assert.That(WorldSquallMotion.Speed(9, 45 - .001f, settings),
                Is.EqualTo(WorldSquallMotion.Speed(9, 45 + .001f, settings)).Within(.00001f));
        }

        [Test]
        public void LegacySettingsGainNewControlsAndSpawnDistancesStayVisible()
        {
            var legacy = JsonUtility.FromJson<SquallWeatherSettings>(
                "{\"WindMultiplier\":2,\"HorizonStartMetres\":1000,\"VisibilityDistanceMetres\":4000,\"MinimumSpawnDistanceMetres\":9000,\"MaximumSpawnDistanceMetres\":10000}").Validated();
            Assert.That(legacy.MaximumSpeedMultiplier, Is.EqualTo(8));
            Assert.That(legacy.SpeedVariationSeconds, Is.EqualTo(45));
            Assert.That(legacy.CloudFringeFraction, Is.GreaterThan(.5f));
            Assert.That(legacy.DistantOpacity, Is.LessThan(.3f));
            Assert.That(legacy.WindMultiplier, Is.EqualTo(2), "Preserve authored peak wind settings.");
            Assert.That(legacy.MaximumSpawnDistanceMetres, Is.LessThan(legacy.VisibilityDistanceMetres));
        }

        [Test]
        public void HeadingNoiseIsSmoothSeededAndAlwaysWithinTheDownwindCone()
        {
            var settings = environment.Weather.Squalls;
            float minimum = 45, maximum = -45;
            for (var i = 0; i < 6000; i++)
            {
                var age = i * .2f;
                var heading = WorldSquallMotion.Direction(7129, age, settings);
                var angle = Vector2.SignedAngle(Vector2.right, heading);
                Assert.That(angle, Is.InRange(-45.001f, 45.001f));
                Assert.That(heading.magnitude, Is.EqualTo(1f).Within(.000001f));
                var next = WorldSquallMotion.Direction(7129, age + .02f, settings);
                Assert.That(Mathf.Abs(Vector2.SignedAngle(heading, next)), Is.LessThan(.04f));
                minimum = Mathf.Min(minimum, angle);
                maximum = Mathf.Max(maximum, angle);
            }
            Assert.That(minimum, Is.LessThan(-10));
            Assert.That(maximum, Is.GreaterThan(10));
            Assert.That(WorldSquallMotion.Direction(7129, 30, settings),
                Is.Not.EqualTo(WorldSquallMotion.Direction(7130, 30, settings)), "Schools of storms must not share one heading sequence.");
        }

        [Test]
        public void CurvedTravelMatchesFineFramesAcrossIndependentNoiseBoundaries()
        {
            var settings = environment.Weather.Squalls;
            settings.SpeedVariationSeconds = 3.7f;
            settings.HeadingVariationSeconds = 5.3f;
            settings.MinimumSpeedMultiplier = .4f;
            double x = 0, y = 0;
            for (var i = 0; i < 5000; i++)
            {
                var travel = WorldSquallMotion.Displacement(7129, i * .02f, (i + 1) * .02f, settings);
                x += travel.x; y += travel.y;
            }
            var combined = WorldSquallMotion.Displacement(7129, 0, 100, settings);
            Assert.That(Vector2.Distance(combined, new Vector2((float)x, (float)y)), Is.LessThan(.001f));
            foreach (var age in new[] { 3.7f, 5.3f, 45f, 90f })
            {
                var velocity = WorldSquallMotion.Direction(7129, age, settings) * WorldSquallMotion.Speed(7129, age, settings);
                float start = age - .002f, end = age + .002f;
                var drift = WorldSquallMotion.Displacement(7129, start, end, settings) / (end - start);
                Assert.That(Vector2.Distance(drift, velocity), Is.LessThan(.003f));
            }
            settings.MaximumHeadingDeviationDegrees = 0;
            Assert.That(WorldSquallMotion.Displacement(7129, 0, 100, settings),
                Is.EqualTo(new Vector2(WorldSquallMotion.Travel(7129, 0, 100, settings), 0)));
        }

        [Test]
        public void RainFollowsTheWeatherWindEvenWhenTheStormStopsTravelling()
        {
            var weather = environment.Weather;
            weather.WindDirection = Vector2.left;
            weather.WindSpeedMetresPerSecond = 9;
            weather.Squalls.MinimumSpeedMultiplier = weather.Squalls.MaximumSpeedMultiplier = 0;
            environment.ApplyWeather(weather);
            Assert.That(environment.SpawnSquall(target.transform.position, 400), Is.True);
            environment.AdvanceSqualls(.1f);
            Invoke("LateUpdate");
            Assert.That(environment.Squall.Field.Velocity, Is.EqualTo(Vector2.zero));
            var drops = root.GetComponentInChildren<ParticleSystem>();
            Assert.That(drops, Is.Not.Null);
            var particles = new ParticleSystem.Particle[1800];
            var count = drops.GetParticles(particles);
            Assert.That(count, Is.GreaterThan(0));
            for (var i = 0; i < count; i++)
            {
                Assert.That(particles[i].velocity.x, Is.InRange(-10f, -5f), "A stopped storm still has the ambient weather wind.");
                Assert.That(particles[i].velocity.z, Is.EqualTo(0).Within(.001f));
                Assert.That(particles[i].velocity.y, Is.LessThan(-30));
            }
        }

        [Test]
        public void ExistingStormUsesTheNewDownwindConeAndPublishesItsActualDrift()
        {
            environment.SpawnSquall(target.transform.position, 300);
            var weather = environment.Weather;
            weather.WindDirection = Vector2.left;
            weather.WindSpeedMetresPerSecond = 10;
            weather.Squalls.MinimumSpeedMultiplier = weather.Squalls.MaximumSpeedMultiplier = 1;
            environment.ApplyWeather(weather);
            var before = environment.Squall;
            environment.AdvanceSqualls(.01f);
            var after = environment.Squall;
            var displacement = new Vector2(after.Center.x - before.Center.x, after.Center.z - before.Center.z);
            Assert.That(Vector2.Angle(displacement, Vector2.left), Is.LessThanOrEqualTo(45.01f));
            Assert.That(Vector2.Angle(after.Field.Velocity, Vector2.left), Is.LessThanOrEqualTo(45.001f));
            Assert.That(Vector2.Distance(displacement / .01f, after.Field.Velocity), Is.LessThan(.01f));
            var published = Shader.GetGlobalVector("_MotuSquallWind");
            Assert.That(new Vector2(published.y, published.z), Is.EqualTo(after.Field.Velocity));
            Assert.That(after.Field.Velocity.magnitude, Is.EqualTo(10).Within(.00001f));
        }

        [Test]
        public void OldSettingsGainWanderingAndAuthoredZeroDeviationSurvivesValidation()
        {
            var settings = SquallWeatherSettings.Default;
            settings.Version = 2;
            settings.MaximumHeadingDeviationDegrees = settings.HeadingVariationSeconds = 0;
            settings.MaximumActiveSqualls = 2;
            settings = settings.Validated();
            Assert.That(settings.MaximumHeadingDeviationDegrees, Is.EqualTo(45));
            Assert.That(settings.HeadingVariationSeconds, Is.EqualTo(90));
            Assert.That(settings.MaximumActiveSqualls, Is.EqualTo(2));
            settings.MaximumHeadingDeviationDegrees = 0;
            settings.HeadingVariationSeconds = 20;
            settings = JsonUtility.FromJson<SquallWeatherSettings>(JsonUtility.ToJson(settings)).Validated();
            Assert.That(settings.MaximumHeadingDeviationDegrees, Is.Zero);
            Assert.That(settings.HeadingVariationSeconds, Is.EqualTo(20));
            settings.MaximumHeadingDeviationDegrees = 90;
            Assert.That(settings.Validated().MaximumHeadingDeviationDegrees, Is.EqualTo(45));
        }

        [Test]
        public void SpawnSequenceIsDeterministicAndDoesNotConsumeUnityRandom()
        {
            var randomBefore = UnityEngine.Random.state;
            try
            {
                UnityEngine.Random.InitState(31415);
                var expectedState = UnityEngine.Random.state;
                var cells = new WorldSquallState[8];
                for (int i = 0; i < cells.Length; i++)
                {
                    environment.ClearSquall();
                    Assert.IsTrue(environment.SpawnSquall());
                    cells[i] = environment.Squall;
                }
                Assert.That(UnityEngine.Random.state, Is.EqualTo(expectedState));
                environment.ClearSquall();
                // Reset only the controller's private stream, like reinitializing this seed.
                typeof(WorldEnvironmentController).GetField("squallRandom", Hidden).SetValue(environment, null);
                Invoke("InitializeSqualls");
                for (int i = 0; i < cells.Length; i++)
                {
                    environment.ClearSquall();
                    Assert.IsTrue(environment.SpawnSquall());
                    Assert.That(environment.Squall.Center, Is.EqualTo(cells[i].Center));
                    Assert.That(environment.Squall.DiameterMetres, Is.EqualTo(cells[i].DiameterMetres));
                    Assert.That(environment.Squall.Seed, Is.EqualTo(cells[i].Seed));
                }
                Assert.That(UnityEngine.Random.state, Is.EqualTo(expectedState));
            }
            finally { UnityEngine.Random.state = randomBefore; }
        }

        [Test]
        public void PausedUpdateFreezesCellAndHostReleaseRecapturesBothGlobals()
        {
            // Release and resume actual ownership methods synchronously: editor audits
            // do not receive MonoBehaviour lifecycle callbacks outside Play mode.
            Invoke("ReleaseEnvironment");
            var hostBounds = new Vector4(1, 2, 3, 4);
            var hostWind = new Vector4(5, 6, 7, 8);
            Shader.SetGlobalVector("_MotuSquall", hostBounds);
            Shader.SetGlobalVector("_MotuSquallWind", hostWind);
            Invoke("ResumeEnvironment");
            Assert.IsTrue(environment.SpawnSquall(target.transform.position, 200));
            var before = environment.Squall;
            Time.timeScale = 0;
            Invoke("Update");
            Assert.That(environment.Squall.Center, Is.EqualTo(before.Center));
            Assert.That(environment.Squall.AgeSeconds, Is.EqualTo(before.AgeSeconds));
            Time.timeScale = timeScale;
            Invoke("ReleaseEnvironment");
            Assert.That(Shader.GetGlobalVector("_MotuSquall"), Is.EqualTo(hostBounds));
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(hostWind));
            var nextBounds = new Vector4(11, 12, 13, 14);
            var nextWind = new Vector4(15, 16, 17, 18);
            Shader.SetGlobalVector("_MotuSquall", nextBounds);
            Shader.SetGlobalVector("_MotuSquallWind", nextWind);
            Invoke("ResumeEnvironment");
            Assert.That(environment.Squall.Center, Is.EqualTo(before.Center));
            Assert.That(Shader.GetGlobalVector("_MotuSquall"), Is.EqualTo(before.Field.ShaderBounds));
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(before.Field.ShaderWind));
            Invoke("ReleaseEnvironment");
            Assert.That(Shader.GetGlobalVector("_MotuSquall"), Is.EqualTo(nextBounds));
            Assert.That(Shader.GetGlobalVector("_MotuSquallWind"), Is.EqualTo(nextWind));
        }

        private void Invoke(string name) => typeof(WorldEnvironmentController).GetMethod(name, Hidden).Invoke(environment, null);
    }
}
