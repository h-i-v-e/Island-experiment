using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Motu.Islands;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ShipRouteAvoidanceTests
    {
        private const BindingFlags Private = BindingFlags.Instance | BindingFlags.NonPublic;
        private readonly List<TerrainTileStreamer> streamers = new List<TerrainTileStreamer>();
        private GameObject host;
        private IslandWorldManager world;

        [SetUp]
        public void SetUp()
        {
            host = new GameObject("Ship route fixture");
            host.SetActive(false);
            world = host.AddComponent<IslandWorldManager>();
        }

        [TearDown]
        public void TearDown()
        {
            foreach (var streamer in streamers) streamer.Dispose();
            streamers.Clear();
            Object.DestroyImmediate(host);
        }

        [Test]
        public void UnloadedCellsArePermittedWithoutDiscoveryOrGeneration()
        {
            var result = Route(Vector2.zero, 0f);
            Assert.That(result.CanProceed, Is.True);
            Assert.That(result.HeadingDegrees, Is.EqualTo(0f));
            Assert.That(world.KnownIslandCount, Is.Zero);
        }

        [Test]
        public void HorizonOnlyGenerationRemainsUnloaded()
        {
            Install(Vector2Int.zero, null, preparing: true);
            Assert.That(Route(Vector2.zero, 0f).CanProceed, Is.True);
        }

        [Test]
        public void PreparingDetailedIslandBlocksAllHeadings()
        {
            Install(Vector2Int.zero, null, preparing: true, resident: true);
            Assert.That(Route(Vector2.zero, 0f).CanProceed, Is.False);
        }

        [Test]
        public void ShallowsAcrossTheSweptPathCauseAnAlternateCourse()
        {
            Install(Vector2Int.zero, (x, z) => z > 100f && Math.Abs(x) < 100f ? -1f : -50f);
            var result = Route(Vector2.zero, 0f, preferredTurn: -1);
            Assert.That(result.CanProceed, Is.True);
            Assert.That(result.TurnSign, Is.EqualTo(-1));
            Assert.That(Mathf.Abs(Mathf.DeltaAngle(0f, result.HeadingDegrees)), Is.GreaterThan(0f));
            Assert.That(ShipRouteAvoidance.IsHeadingClear(world, Vector2.zero, 0f, 0f, 20f, 3f, 300f), Is.False);
            Assert.That(ShipRouteAvoidance.IsHeadingClear(world, Vector2.zero, result.HeadingDegrees, 0f, 20f, 3f, 300f), Is.True);
        }

        [Test]
        public void InsufficientDepthBlocksEveryHeading()
        {
            Install(Vector2Int.zero, (x, z) => -1f);
            Assert.That(Route(Vector2.zero, 0f).CanProceed, Is.False);
        }

        [Test]
        public void DeepResidentWaterIsPassable()
        {
            Install(Vector2Int.zero, (x, z) => -50f);
            Assert.That(Route(Vector2.zero, 0f).CanProceed, Is.True);
        }

        [Test]
        public void UnpreparedResidentGeneratorBlocksEveryHeading()
        {
            Install(Vector2Int.zero, null, resident: true);
            Assert.That(Route(Vector2.zero, 0f).CanProceed, Is.False);
        }

        [Test]
        public void DeepWaterAndUnloadedNeighbourDoNotHideResidentLand()
        {
            Install(Vector2Int.zero, (x, z) => z > 900f ? 4f : -50f);
            var result = Route(new Vector2(0f, 850f), 0f);
            Assert.That(result.CanProceed, Is.True);
            Assert.That(Mathf.Abs(Mathf.DeltaAngle(0f, result.HeadingDegrees)), Is.GreaterThan(0f));
        }

        [Test]
        public void HullLengthExtendsTheSweptFootprintBeyondTheCentrePath()
        {
            Install(Vector2Int.zero, (x, z) => z > 50f ? 4f : -50f);
            Assert.That(ShipRouteAvoidance.IsHeadingClear(world, Vector2.zero, 0f, 0f, 10f, 3f, 10f), Is.True);
            Assert.That(ShipRouteAvoidance.IsHeadingClear(world, Vector2.zero, 0f, 0f, 10f, 3f, 10f, 100f), Is.False);
        }

        [Test]
        public void NoGoConeIsRespectedOnBothCandidateSides()
        {
            var result = ShipRouteAvoidance.Evaluate(world, Vector2.zero, 0f, 4f, 0f,
                20f, 3f, 300f, Vector2.down, 45f, -1);
            Assert.That(result.CanProceed, Is.True);
            Assert.That(result.TurnSign, Is.EqualTo(-1));
            Assert.That(Vector2.Angle(Direction(result.HeadingDegrees), Vector2.up), Is.GreaterThanOrEqualTo(45f - .001f));
        }

        private ShipRouteAvoidanceResult Route(Vector2 position, float heading, int preferredTurn = 0) =>
            ShipRouteAvoidance.Evaluate(world, position, heading, 4f, 0f, 20f, 3f, 300f,
                Vector2.zero, 45f, preferredTurn);

        private static Vector2 Direction(float heading) =>
            new Vector2(Mathf.Sin(heading * Mathf.Deg2Rad), Mathf.Cos(heading * Mathf.Deg2Rad));

        private void Install(Vector2Int cell, Func<float, float, float> height, bool preparing = false, bool resident = false)
        {
            var profile = new IslandGenerationProfile(new IslandGenerationSettings { UseSnapshotCache = false },
                new IslandRiverSettings(), new IslandForestSettings(), new IslandReedSettings(),
                new IslandFernSettings(), new IslandRenderingSettings(), null);
            var request = IslandGenerationRequest.FromOwnedProfile(42, cell, profile, default);
            var entryType = typeof(IslandWorldManager).GetNestedType("IslandRuntimeEntry", BindingFlags.NonPublic);
            var entry = Activator.CreateInstance(entryType, Private, null, new object[] { request }, null);
            if (height != null || resident)
            {
                var child = new GameObject("Prepared island");
                child.transform.SetParent(host.transform, false);
                child.transform.SetPositionAndRotation(new Vector3(cell.x * 2000f, 0f, cell.y * 2000f), request.WorldRotation);
                var generator = child.AddComponent<IslandGenerator>();
                generator.Configure(request);
                if (height != null)
                {
                    var streamer = child.AddComponent<TerrainTileStreamer>();
                    streamer.worldSize = 2000f;
                    streamer.colliderHeightMap = TerrainFootprintTests.MakeMap(2000f, 2, height);
                    streamers.Add(streamer);
                    typeof(IslandGenerator).GetField("terrainStreamer", Private).SetValue(generator, streamer);
                }
                entryType.GetProperty("Generator", Private).SetValue(entry, generator);
            }
            entryType.GetProperty("Generating", Private).SetValue(entry, preparing);
            var entries = (IDictionary)typeof(IslandWorldManager).GetField("managedIslands", Private).GetValue(world);
            entries.Add(cell, entry);
        }
    }
}
