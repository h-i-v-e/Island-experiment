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
    public sealed class TerrainFootprintWorldTests
    {
        private const BindingFlags PrivateInstance = BindingFlags.Instance | BindingFlags.NonPublic;
        private readonly List<TerrainTileStreamer> streamers = new List<TerrainTileStreamer>();
        private GameObject host;
        private IslandWorldManager world;

        private sealed class IslandSource : IIslandGenerationRequestFactory
        {
            private readonly Func<Vector2Int, bool> hasIsland;
            internal int GenerationRequests { get; private set; }

            internal IslandSource(Func<Vector2Int, bool> hasIsland) => this.hasIsland = hasIsland;

            public bool HasIsland(Vector2Int cell) => hasIsland(cell);

            public IslandGenerationRequest CreateIslandGenerationRequest(Vector2Int cell)
            {
                GenerationRequests++;
                throw new InvalidOperationException("Terrain footprint queries must never request generation.");
            }
        }

        [SetUp]
        public void CreateInactiveWorld()
        {
            host = new GameObject("CPU terrain footprint world");
            host.SetActive(false);
            world = host.AddComponent<IslandWorldManager>();
        }

        [TearDown]
        public void DestroyWorld()
        {
            foreach (var streamer in streamers) streamer.Dispose();
            streamers.Clear();
            Object.DestroyImmediate(host);
        }

        [Test]
        public void KnownOpenSeaIsCompleteWithoutTerrainOrGeneration()
        {
            var source = SetSource(_ => false);
            var sample = Query(new TerrainFootprint(new Vector2(-1000f, 1000f), new Vector2(60f, 80f), 27f));

            Assert.That(sample.HasTerrain, Is.False);
            Assert.That(sample.RequestedArea, Is.EqualTo(4800d).Within(.1d));
            Assert.That(sample.OpenSeaArea, Is.EqualTo(sample.RequestedArea).Within(.1d));
            Assert.That(sample.TerrainArea, Is.Zero);
            Assert.That(sample.UnknownArea, Is.EqualTo(0d).Within(.1d));
            Assert.That(sample.IsDryLand, Is.False);
            Assert.That(source.GenerationRequests, Is.Zero);
            Assert.That(world.KnownIslandCount, Is.Zero);
        }

        [Test]
        public void MissingFactoryAndUnloadedIslandRemainUnknown()
        {
            var footprint = new TerrainFootprint(Vector2.zero, new Vector2(20f, 30f));
            var status = world.QueryTerrainFootprint(footprint, out var absentFactory);
            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Incomplete));
            Assert.That(absentFactory.UnknownArea, Is.EqualTo(600d).Within(.0001d));
            Assert.That(absentFactory.OpenSeaArea, Is.Zero);

            var source = SetSource(_ => true);
            status = world.QueryTerrainFootprint(footprint, out var unloadedIsland);
            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Incomplete));
            Assert.That(unloadedIsland.UnknownArea, Is.EqualTo(600d).Within(.0001d));
            Assert.That(unloadedIsland.HasTerrain, Is.False);
            Assert.That(unloadedIsland.IsDryLand, Is.False);
            Assert.That(source.GenerationRequests, Is.Zero);
            Assert.That(world.KnownIslandCount, Is.Zero);
        }

        [Test]
        public void CrossingAnUnloadedIslandAndKnownSeaKeepsTheirAreasDistinct()
        {
            SetSource(cell => cell == Vector2Int.zero);
            var status = world.QueryTerrainFootprint(new TerrainFootprint(new Rect(980f, -10f, 40f, 20f)),
                out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Incomplete));
            Assert.That(sample.UnknownArea, Is.EqualTo(400d).Within(.001d));
            Assert.That(sample.OpenSeaArea, Is.EqualTo(400d).Within(.001d));
            Assert.That(sample.TerrainArea, Is.Zero);
            Assert.That(sample.IsDryLand, Is.False);
        }

        [TestCase(0, false)]
        [TestCase(-1, true)]
        public void RectangleAcrossASectionSeamCombinesBothCpuMaps(int westCellX, bool southCoast)
        {
            SetSource(_ => true);
            InstallIsland(new Vector2Int(westCellX, 0), 3f, southCoast);
            InstallIsland(new Vector2Int(westCellX + 1, 0), 12f, southCoast);
            var seamX = westCellX * 2000f + 1000f;
            var sample = Query(new TerrainFootprint(new Rect(seamX - 20f, -20f, 40f, 40f)));

            Assert.That(sample.TerrainArea, Is.EqualTo(1600d).Within(.01d));
            Assert.That(sample.UnknownArea, Is.EqualTo(0d).Within(.01d));
            Assert.That(sample.OpenSeaArea, Is.Zero);
            Assert.That(sample.MinimumHeight, Is.EqualTo(3f).Within(.0001f));
            Assert.That(sample.MaximumHeight, Is.EqualTo(12f).Within(.0001f));
            Assert.That(sample.CoverageFraction, Is.EqualTo(1f));
            Assert.That(sample.IsDryLand, Is.True);
        }

        [Test]
        public void ExactSectionBoundaryDoesNotIncludeTheUnloadedNeighbour()
        {
            SetSource(_ => true);
            InstallIsland(Vector2Int.zero, 6f);
            var sample = Query(new TerrainFootprint(new Rect(960f, -20f, 40f, 40f)));

            Assert.That(sample.TerrainArea, Is.EqualTo(1600d).Within(.001d));
            Assert.That(sample.UnknownArea, Is.EqualTo(0d).Within(.001d));
            Assert.That(sample.MinimumHeight, Is.EqualTo(6f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.True);
        }

        [Test]
        public void ResidentTerrainAndKnownSeaHaveCompleteSeparateCoverage()
        {
            SetSource(cell => cell == Vector2Int.zero);
            InstallIsland(Vector2Int.zero, 6f);
            var sample = Query(new TerrainFootprint(new Rect(980f, -20f, 40f, 40f)));

            Assert.That(sample.TerrainArea, Is.EqualTo(800d).Within(.001d));
            Assert.That(sample.OpenSeaArea, Is.EqualTo(800d).Within(.001d));
            Assert.That(sample.UnknownArea, Is.EqualTo(0d).Within(.001d));
            Assert.That(sample.CoverageFraction, Is.EqualTo(1f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void WorldCellAndTerrainScanningShareOneWorkLimit()
        {
            SetSource(_ => true);
            InstallIsland(Vector2Int.zero, 6f);
            InstallIsland(Vector2Int.right, 9f);
            var status = world.QueryTerrainFootprint(new TerrainFootprint(new Rect(850f, -100f, 300f, 200f)),
                out var sample, 12);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.WorkLimitExceeded));
            Assert.That(sample.TestedCellCount, Is.LessThanOrEqualTo(12));
            Assert.That(sample.IsComplete, Is.False);
            Assert.That(sample.UnknownArea, Is.GreaterThan(0d));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void QueryLeavesDiscoveryResidencyFocusAndColliderStateUnchanged()
        {
            var source = SetSource(_ => true);
            var island = InstallIsland(Vector2Int.zero, 6f);
            var focus = new GameObject("Existing streaming target");
            focus.transform.SetParent(host.transform, false);
            island.Streaming.Target = focus.transform;
            var logicalPosition = world.LogicalPlayerPosition;
            var knownCount = world.KnownIslandCount;
            var loadedCount = world.LoadedIslandCount;

            Query(new TerrainFootprint(Vector2.zero, new Vector2(100f, 100f)));

            Assert.That(world.FocusedIsland, Is.Null);
            Assert.That(world.LogicalPlayerPosition, Is.EqualTo(logicalPosition));
            Assert.That(world.KnownIslandCount, Is.EqualTo(knownCount));
            Assert.That(world.LoadedIslandCount, Is.EqualTo(loadedCount));
            Assert.That(world.QueuedIslandCount, Is.Zero);
            Assert.That(world.GeneratingIslandCount, Is.Zero);
            Assert.That(island.Streaming.Target, Is.SameAs(focus.transform));
            Assert.That(source.GenerationRequests, Is.Zero);
            Assert.That(host.GetComponentsInChildren<Collider>(true), Is.Empty);
        }

        [Test]
        public void GeneratorPublicQueryUsesItsCpuMapWithoutAnActiveRuntime()
        {
            var island = InstallIsland(Vector2Int.zero, -4f, true);
            Assert.That(island.HasRuntime, Is.False);

            var status = ((ITerrainFootprintQuery)island).QueryTerrainFootprint(
                new TerrainFootprint(new Vector2(-200f, 100f), new Vector2(50f, 30f), 180f), out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Complete));
            Assert.That(sample.MinimumHeight, Is.EqualTo(-4f).Within(.0001f));
            Assert.That(sample.MinimumSeaClearance, Is.EqualTo(-4f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void SouthCoastQueryReversesBothAxesOfAnAsymmetricHeightField()
        {
            SetSource(_ => true);
            var island = InstallIsland(Vector2Int.zero, 0f, true, (x, z) => 20f + x * .02f + z * .01f);
            Assert.That(island.transform.forward, Is.EqualTo(Vector3.back));
            var sample = Query(new TerrainFootprint(new Vector2(-200f, -100f), new Vector2(50f, 30f)));

            Assert.That(sample.MinimumHeight, Is.EqualTo(23.75f).Within(.0001f));
            Assert.That(sample.MaximumHeight, Is.EqualTo(26.25f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.True);
        }

        [TestCase(float.NaN)]
        [TestCase(float.PositiveInfinity)]
        public void NonFiniteWorldCentersReturnInvalidInput(float x)
        {
            SetSource(_ => false);
            var status = world.QueryTerrainFootprint(new TerrainFootprint(new Vector2(x, 0f), Vector2.one), out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.InvalidInput));
            Assert.That(sample.IsDryLand, Is.False);
            Assert.That(sample.TestedCellCount, Is.Zero);
        }

        [Test, Timeout(1000)]
        public void HugeFiniteFootprintCannotOverflowOrStartUnboundedSectionScanning()
        {
            var source = SetSource(_ => false);
            world.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, new Vector2(1e30f, 1e30f)),
                out var sample, 16);

            Assert.That(sample.IsComplete, Is.False);
            Assert.That(sample.IsDryLand, Is.False);
            Assert.That(sample.TestedCellCount, Is.LessThanOrEqualTo(16));
            Assert.That(source.GenerationRequests, Is.Zero);
        }

        [Test]
        public void UnrepresentableFarGridBoundariesCannotMisclassifyAnUnloadedIslandAsSea()
        {
            var source = SetSource(cell => cell == new Vector2Int(10000000, 0));
            var footprint = new TerrainFootprint(new Vector2(20000000000f - 2048f, 0f), new Vector2(4096f, 10f));

            var status = world.QueryTerrainFootprint(footprint, out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.InvalidInput));
            Assert.That(sample.IsComplete, Is.False);
            Assert.That(sample.IsDryLand, Is.False);
            Assert.That(sample.HasTerrain, Is.False);
            Assert.That(sample.TerrainArea, Is.Zero);
            Assert.That(sample.OpenSeaArea, Is.Zero);
            Assert.That(sample.TestedCellCount, Is.Zero);
            Assert.That(source.GenerationRequests, Is.Zero);
            Assert.That(world.KnownIslandCount, Is.Zero);
        }

        private IslandSource SetSource(Func<Vector2Int, bool> hasIsland)
        {
            var source = new IslandSource(hasIsland);
            world.IslandGenerationRequestFactory = source;
            return source;
        }

        private TerrainFootprintSample Query(TerrainFootprint footprint)
        {
            var status = ((ITerrainFootprintQuery)world).QueryTerrainFootprint(footprint, out var sample);
            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Complete));
            Assert.That(sample.Status, Is.EqualTo(status));
            Assert.That(sample.IsComplete, Is.True);
            return sample;
        }

        private IslandGenerator InstallIsland(Vector2Int cell, float height, bool southCoast = false,
            Func<float, float, float> heightField = null)
        {
            var settings = new IslandGenerationSettings
            {
                UseSnapshotCache = false,
                TerrainShape = southCoast ? IslandTerrainShape.CoastalStrip : IslandTerrainShape.Island,
            };
            var profile = new IslandGenerationProfile(settings,
                new IslandRiverSettings(), new IslandForestSettings(), new IslandReedSettings(),
                new IslandFernSettings(), new IslandRenderingSettings(), null);
            var request = IslandGenerationRequest.FromOwnedProfile(42, cell, profile, default,
                coastalInlandSouth: southCoast);
            Assert.That(request.CoastalInlandSouth, Is.EqualTo(southCoast));
            var child = new GameObject("Prepared CPU terrain section " + cell);
            child.transform.SetParent(host.transform, false);
            child.transform.SetPositionAndRotation(new Vector3(cell.x * 2000f, 0f, cell.y * 2000f), request.WorldRotation);
            var generator = child.AddComponent<IslandGenerator>();
            generator.Configure(request);
            var streamer = child.AddComponent<TerrainTileStreamer>();
            streamer.worldSize = 2000f;
            streamer.colliderHeightMap = TerrainFootprintTests.MakeMap(2000f, 2, heightField ?? ((x, z) => height));
            streamers.Add(streamer);
            typeof(IslandGenerator).GetField("terrainStreamer", PrivateInstance).SetValue(generator, streamer);

            var entryType = typeof(IslandWorldManager).GetNestedType("IslandRuntimeEntry", BindingFlags.NonPublic);
            var entry = Activator.CreateInstance(entryType, PrivateInstance, null, new object[] { request }, null);
            entryType.GetProperty("Generator", PrivateInstance).SetValue(entry, generator);
            var entries = (IDictionary)typeof(IslandWorldManager).GetField("managedIslands", PrivateInstance).GetValue(world);
            entries.Add(cell, entry);
            return generator;
        }
    }
}
