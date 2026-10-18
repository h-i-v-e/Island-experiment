using System;
using System.Reflection;
using Motu.Islands;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ArchipelagoLightingTests
    {
        private const BindingFlags PrivateInstance = BindingFlags.Instance | BindingFlags.NonPublic;

        private sealed class Fixture : IDisposable
        {
            private readonly GameObject root = new GameObject("Archipelago lighting fixture");
            private readonly GameObject environmentObject = new GameObject("World lighting fixture");
            private readonly GameObject player = new GameObject("Observer");
            private readonly GameObject sunObject = new GameObject("Fixture sun");
            internal readonly CoherentIslandFactory Factory;
            internal readonly IslandWorldManager Manager;
            internal readonly WorldEnvironmentController Environment;
            internal readonly Light Sun;

            internal Fixture()
            {
                // Route through the manager without starting island generation.
                root.SetActive(false);
                Factory = root.AddComponent<CoherentIslandFactory>();
                Manager = root.AddComponent<IslandWorldManager>();
                Manager.ConfigureIslandGenerationRequestFactory(Factory);
                Environment = environmentObject.AddComponent<WorldEnvironmentController>();
                Sun = sunObject.AddComponent<Light>();
                Sun.type = LightType.Directional;
                SetField(Manager, "worldEnvironment", Environment);
                SetField(Manager, "nextDiscoveryTime", float.PositiveInfinity);
            }

            internal void MoveTo(float z)
            {
                player.transform.position = new Vector3(123f, 7f, z);
                Manager.SetStreamingTarget(player.transform);
                SetField(Manager, "nextDiscoveryTime", float.PositiveInfinity);
            }

            internal void WalkTo(float z)
            {
                player.transform.position = new Vector3(-456f, 17f, z);
                Invoke(Manager, "Update");
            }

            public void Dispose()
            {
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(environmentObject);
                Object.DestroyImmediate(player);
                Object.DestroyImmediate(sunObject);
            }
        }

        [TestCase(-64000f, 0f)]
        [TestCase(-32000f, 0f)]
        [TestCase(-16000f, 20f)]
        [TestCase(0f, 40f)]
        [TestCase(16000f, 60f)]
        [TestCase(32000f, 80f)]
        [TestCase(64000f, 80f)]
        public void PlayerPositionMapsContinuouslyFromOverheadToLowNoonSun(float z, float latitude)
        {
            using var fixture = new Fixture();
            fixture.MoveTo(z);
            Assert.That(fixture.Environment.SunLatitudeDegrees, Is.EqualTo(latitude).Within(.0001f));
            var sun = CelestialLighting.EvaluateSun(12f, fixture.Environment.SunLatitudeDegrees, 1.25f);
            var elevation = Mathf.Asin(sun.LocalDirection.y) * Mathf.Rad2Deg;
            Assert.That(elevation, Is.EqualTo(90f - latitude).Within(.001f));
        }

        [Test]
        public void WalkingAndRadiusEditsUpdateLatitudeWithoutMutatingAuthoredSettings()
        {
            using var fixture = new Fixture();
            var authoredLatitude = fixture.Manager.GlobalEnvironmentSettings.SunLatitudeDegrees;
            fixture.MoveTo(-32000f);
            fixture.WalkTo(16000f);
            Assert.That(fixture.Environment.SunLatitudeDegrees, Is.EqualTo(60f).Within(.0001f));
            SetField(fixture.Factory, "archipelegoRadius", 8);
            fixture.WalkTo(16000f);
            Assert.That(fixture.Environment.SunLatitudeDegrees, Is.EqualTo(80f).Within(.0001f));
            Assert.That(fixture.Manager.GlobalEnvironmentSettings.SunLatitudeDegrees, Is.EqualTo(authoredLatitude));

            // Switching to a non-coherent source restores the fixed world latitude.
            fixture.Manager.IslandGenerationRequestFactory = null;
            fixture.WalkTo(16000f);
            Assert.That(fixture.Environment.SunLatitudeOverrideDegrees, Is.Null);
        }

        [Test]
        public void LatitudeChangesReachTheSkySunLightAndMoonOrbit()
        {
            using var fixture = new Fixture();
            var settings = new WorldEnvironmentSettings();
            SetField(settings, "sunlight", fixture.Sun);
            SetField(settings, "startingSolarTimeHours", 12f);
            SetField(settings, "startingMoonPhase", 0f);
            SetField(settings, "moonEquatorOffsetDegrees", 22f);
            fixture.MoveTo(-32000f);
            fixture.Environment.Initialize(settings, new IslandCloudSettings(), 64f, 128f, null);

            try
            {
                foreach (var z in new[] { -32000f, 0f, 32000f })
                {
                    fixture.WalkTo(z);
                    Invoke(fixture.Environment, "UpdateSolarLighting", 0f);
                    var latitude = fixture.Environment.SunLatitudeDegrees;
                    var sunDirection = (Vector3)fixture.Environment.SkyMaterial.GetVector("_SunDirection");
                    var moonDirection = (Vector3)fixture.Environment.SkyMaterial.GetVector("_MoonDirection");
                    Assert.That(Vector3.Angle(sunDirection, -RenderSettings.sun.transform.forward), Is.LessThan(.03f));
                    Assert.That(Mathf.Asin(sunDirection.y) * Mathf.Rad2Deg,
                        Is.EqualTo(90f - latitude).Within(.001f));
                    Assert.That(Mathf.Asin(moonDirection.y) * Mathf.Rad2Deg,
                        Is.EqualTo(90f - Mathf.Max(latitude - 22f, 0f)).Within(.001f));
                }

                SetField(settings, "moonEquatorOffsetDegrees", 0f);
                Invoke(fixture.Environment, "UpdateSolarLighting", 0f);
                Assert.That(Vector3.Angle(
                    fixture.Environment.SkyMaterial.GetVector("_SunDirection"),
                    fixture.Environment.SkyMaterial.GetVector("_MoonDirection")), Is.LessThan(.03f));

                fixture.Environment.SunLatitudeOverrideDegrees = null;
                Invoke(fixture.Environment, "UpdateSolarLighting", 0f);
                Assert.That(fixture.Environment.SunLatitudeDegrees, Is.EqualTo(settings.SunLatitudeDegrees));
                Assert.That(settings.SunLatitudeDegrees, Is.EqualTo(-36f));
            }
            finally
            {
                fixture.Environment.enabled = false;
            }
        }

        [Test]
        public void RadiusRejectsFarCoordinatesEvenWhenIntSquaresWouldOverflow()
        {
            using var fixture = new Fixture();
            SetField(fixture.Factory, "islandThreshold", 0f);
            Assert.That(fixture.Factory.HasIsland(new Vector2Int(16, 0)), Is.True);
            Assert.That(fixture.Factory.HasIsland(new Vector2Int(16, 1)), Is.False);
            Assert.That(fixture.Factory.HasIsland(new Vector2Int(65536, 0)), Is.False);
            Assert.That(fixture.Factory.HasIsland(new Vector2Int(int.MinValue, int.MinValue)), Is.False);
            SetField(fixture.Factory, "archipelegoRadius", 0);
            Assert.That(fixture.Factory.ArchipelagoRadius, Is.EqualTo(1));
            Assert.That(fixture.Factory.GetSunLatitudeDegrees(2000f), Is.EqualTo(80f));
        }

        private static void SetField(object target, string name, object value) =>
            target.GetType().GetField(name, PrivateInstance).SetValue(target, value);

        private static void Invoke(object target, string name, params object[] arguments) =>
            target.GetType().GetMethod(name, PrivateInstance).Invoke(target, arguments);
    }
}
