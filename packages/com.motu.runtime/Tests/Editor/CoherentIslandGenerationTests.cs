using System;
using System.Reflection;
using Motu.Islands;
using Motu.Settings;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoherentIslandGenerationTests
    {
        private static CoherentIslandFactory CreateFactory(GameObject host, int seed)
        {
            var factory = host.AddComponent<CoherentIslandFactory>();
            factory.Seed = seed;
            SetField(factory, "riverSettings", new IslandRiverSettings());
            SetField(factory, "renderingSettings", new IslandRenderingSettings());
            SetField(factory, "reedSettings", new IslandReedSettings());
            SetField(factory, "fernSettings", new IslandFernSettings());
            return factory;
        }

        [Test]
        public void PreviouslySubmergedCellRetainsItsSeedAndUsesANewSnapshot()
        {
            var host = new GameObject("Submerged island regression");
            try
            {
                var factory = CreateFactory(host, 1840);
                var cell = new Vector2Int(2, 0);
                var request = factory.CreateIslandGenerationRequest(cell);
                Assert.That(request, Is.Not.Null);
                Assert.That(request.RandomSeed, Is.EqualTo(677934609));
                Assert.That(request.Options.landMassOffset, Is.Zero);
                Assert.That(request.Generation.WaterRatio, Is.EqualTo(.8995754f).Within(.000001f));
                Assert.That(request.Generation.MaximumHeightMetres, Is.EqualTo(424.73986f).Within(.001f));
                Assert.That(request.Generation.HydraulicErosionStrength, Is.EqualTo(5.8884554f).Within(.000001f));

                var previous = request.Profile.Clone();
                previous.Generation.LandMassOffset = -.3995282f;
                var oldRequest = new IslandGenerationRequest(
                    request.RandomSeed, cell, previous, request.MaterialColours);
                Assert.That(oldRequest.Options.landMassOffset, Is.LessThan(0f),
                    "Explicitly authored submerged terrain must remain supported.");
                Assert.That(request.SnapshotPath, Is.Not.EqualTo(oldRequest.SnapshotPath),
                    "The submerged cached geometry must not be reused.");
                Assert.That(factory.CreateIslandGenerationRequest(cell).SnapshotPath, Is.EqualTo(request.SnapshotPath));

                var random = new System.Random(request.RandomSeed);
                for (var i = 0; i < 10; i++) random.NextDouble();
                var forestThreshold = Mathf.Lerp(.3f, .9f, (float)random.NextDouble());
                Assert.That(request.Forest.forestNoiseThreshold, Is.EqualTo(forestThreshold),
                    "Removing the offset must not shift subsequent forest samples.");
            }
            finally { Object.DestroyImmediate(host); }
        }

        [TestCase(42)]
        [TestCase(1840)]
        [TestCase(2026)]
        public void DiscoveredIslandsRetainTheirArchipelagoOffsetsWhenLandSurvives(int seed)
        {
            var host = new GameObject("Coherent island coverage");
            try
            {
                var factory = CreateFactory(host, seed);
                var count = 0;
                var archipelagos = 0;
                for (var y = -6; y <= 6; y++)
                for (var x = -6; x <= 6; x++)
                {
                    var cell = new Vector2Int(x, y);
                    if (!factory.HasIsland(cell)) continue;
                    var request = factory.CreateIslandGenerationRequest(cell);
                    var random = new System.Random(request.RandomSeed);
                    for (var i = 0; i < 8; i++) random.NextDouble();
                    var sampledOffset = Mathf.Min(0f, .5f - (float)random.NextDouble());
                    Assert.That(request.Options.landMassOffset,
                        Is.EqualTo(sampledOffset).Or.EqualTo(0f), cell.ToString());
                    if (request.Options.landMassOffset < 0f) archipelagos++;
                    Assert.That(request.Generation.WaterRatio, Is.InRange(.75f, .95f));
                    count++;
                }
                Assert.That(count, Is.GreaterThan(0));
                Assert.That(archipelagos, Is.GreaterThan(0), "Surviving archipelagos must keep their offsets.");
                Assert.That(factory.HasIsland(Vector2Int.zero), Is.False);
            }
            finally { Object.DestroyImmediate(host); }
        }

        private static void SetField(object target, string name, object value) =>
            target.GetType().GetField(name, BindingFlags.Instance | BindingFlags.NonPublic).SetValue(target, value);
    }
}
