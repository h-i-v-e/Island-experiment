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
    public sealed class CylindricalWorldGenerationTests
    {
        private sealed class Fixture : IDisposable
        {
            private readonly GameObject host = new GameObject("Cylindrical world generation");
            internal readonly CoherentIslandFactory Factory;

            internal Fixture()
            {
                Factory = host.AddComponent<CoherentIslandFactory>();
                SetField(Factory, "riverSettings", new IslandRiverSettings());
                SetField(Factory, "renderingSettings", new IslandRenderingSettings());
                SetField(Factory, "reedSettings", new IslandReedSettings());
                SetField(Factory, "fernSettings", new IslandFernSettings());
            }

            public void Dispose() => Object.DestroyImmediate(host);
        }

        [Test]
        public void DefaultWorldHasEightRepeatingSectionsAndStartsJustInsideTheSouthernCoast()
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            Assert.That(factory.WorldSectionCount, Is.EqualTo(8));
            Assert.That(factory.TerrainGeneration.TerrainShape, Is.EqualTo(IslandTerrainShape.CoastalStrip));
            Assert.That(factory.TopCoastRow, Is.EqualTo(16));
            Assert.That(factory.BottomCoastRow, Is.EqualTo(-16));
            Assert.That(factory.StartingCell, Is.EqualTo(new Vector2Int(0, -15)));
            Assert.That(factory.StartingPosition(12f),
                Is.EqualTo(IslandWorldManager.CellCentre(factory.StartingCell, 12f)));

            factory.TerrainGeneration.CoastalSectionCount = 9;
            SetField(factory, "archipelegoRadius", 3);
            Assert.That(factory.WorldSectionCount, Is.EqualTo(9));
            Assert.That(factory.TopCoastRow, Is.EqualTo(3));
            Assert.That(factory.BottomCoastRow, Is.EqualTo(-3));
            Assert.That(factory.StartingCell, Is.EqualTo(new Vector2Int(0, -2)));
            Assert.That(factory.StartingPosition().z, Is.EqualTo(-4000f));
        }

        [TestCase(1)]
        [TestCase(8)]
        [TestCase(9)]
        public void EverySeedAndThresholdKeepTheStartingCellAndItsPeriodicAliasesAtSea(int width)
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            factory.TerrainGeneration.CoastalSectionCount = width;
            foreach (var seed in new[] { int.MinValue, 0, 42, 1840, int.MaxValue })
            foreach (var threshold in new[] { 0f, .25f, .5f, 1f })
            {
                factory.Seed = seed;
                SetField(factory, "islandThreshold", threshold);
                foreach (var repetition in new[] { -100000, -1, 0, 1, 100000 })
                {
                    var cell = new Vector2Int(repetition * width, factory.StartingCell.y);
                    Assert.That(factory.HasIsland(cell), Is.False, $"{seed}/{threshold}: {cell}");
                    Assert.That(factory.CreateIslandGenerationRequest(cell), Is.Null);
                }
            }
        }

        [TestCase(1)]
        [TestCase(8)]
        [TestCase(9)]
        public void DiscoveryRepeatsAcrossTheClosingSeamAndAtExtremeWorldCoordinates(int width)
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            factory.TerrainGeneration.CoastalSectionCount = width;
            foreach (var seed in new[] { -31, 42, 1840 })
            foreach (var threshold in new[] { .25f, .5f, .75f })
            {
                factory.Seed = seed;
                SetField(factory, "islandThreshold", threshold);
                for (var y = factory.BottomCoastRow; y <= factory.TopCoastRow; y++)
                foreach (var x in new[] { int.MinValue, -17, -8, -1, 0, 1, 7, 8, 17, int.MaxValue })
                {
                    var canonical = x % width;
                    if (canonical < 0) canonical += width;
                    var expected = factory.HasIsland(new Vector2Int(canonical, y));
                    Assert.That(factory.HasIsland(new Vector2Int(x, y)), Is.EqualTo(expected),
                        $"{seed}/{threshold}: ({x}, {y})");
                }
            }
        }

        [Test]
        public void InteriorDiscoveryUsesTheNoiseThresholdWithoutRadialOrDistanceWeighting()
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            foreach (var threshold in new[] { 0f, 1f })
            {
                SetField(factory, "islandThreshold", threshold);
                for (var y = factory.BottomCoastRow + 1; y < factory.TopCoastRow; y++)
                foreach (var x in new[] { -1000001, -4, 0, 3, 1000001 })
                {
                    var isStartingCell = x == 0 && y == factory.StartingCell.y;
                    Assert.That(factory.HasIsland(new Vector2Int(x, y)),
                        Is.EqualTo(threshold == 0f && !isStartingCell), $"{threshold}: ({x}, {y})");
                }
            }
            SetField(factory, "islandThreshold", 0f);
            Assert.That(factory.HasIsland(Vector2Int.zero), Is.True,
                "Only the southern starting cell is reserved; the origin is part of the world.");
        }

        [Test]
        public void LayeredNoiseProducesASeededMixtureOfInteriorIslandsAndOpenSea()
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            SetField(factory, "islandThreshold", .5f);
            var masks = new string[2];
            for (var sample = 0; sample < masks.Length; sample++)
            {
                factory.Seed = sample == 0 ? 42 : 1840;
                var mask = new char[factory.WorldSectionCount * (factory.TopCoastRow - factory.BottomCoastRow - 1)];
                var index = 0;
                for (var y = factory.BottomCoastRow + 1; y < factory.TopCoastRow; y++)
                for (var x = 0; x < factory.WorldSectionCount; x++)
                    mask[index++] = factory.HasIsland(new Vector2Int(x, y)) ? 'L' : 'S';
                masks[sample] = new string(mask);
                Assert.That(masks[sample], Does.Contain("L").And.Contain("S"));
            }
            Assert.That(masks[1], Is.Not.EqualTo(masks[0]));
            factory.Seed = 42;
            var origin = factory.HasIsland(Vector2Int.zero);
            factory.Seed = 1840;
            factory.Seed = 42;
            Assert.That(factory.HasIsland(Vector2Int.zero), Is.EqualTo(origin));
        }

        [Test]
        public void BothCoastsAlwaysExistAndNothingGeneratesBeyondTheirNorthSouthBounds()
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            foreach (var threshold in new[] { 0f, 1f })
            {
                SetField(factory, "islandThreshold", threshold);
                foreach (var x in new[] { int.MinValue, -1, 0, 1, int.MaxValue })
                {
                    Assert.That(factory.HasIsland(new Vector2Int(x, factory.TopCoastRow)), Is.True);
                    Assert.That(factory.HasIsland(new Vector2Int(x, factory.BottomCoastRow)), Is.True);
                    foreach (var y in new[] { int.MinValue, factory.BottomCoastRow - 1,
                        factory.TopCoastRow + 1, int.MaxValue })
                    {
                        var outside = new Vector2Int(x, y);
                        Assert.That(factory.HasIsland(outside), Is.False);
                        Assert.That(factory.CreateIslandGenerationRequest(outside), Is.Null);
                    }
                }
            }
        }

        [Test]
        public void RepeatedInteriorCellsRetainTheirProfilesWhileKeepingPhysicalPlacementAndIdentity()
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            factory.Seed = 1840;
            SetField(factory, "islandThreshold", 0f);
            var first = factory.CreateIslandGenerationRequest(new Vector2Int(2, 0));
            Assert.That(first.RandomSeed, Is.EqualTo(677934609),
                "Centred canonical X preserves the existing seed for cell (2, 0).");
            foreach (var x in new[] { -6, 10, 800002 })
            {
                var cell = new Vector2Int(x, 0);
                var repeated = factory.CreateIslandGenerationRequest(cell);
                Assert.That(repeated.RandomSeed, Is.EqualTo(first.RandomSeed));
                Assert.That(repeated.Options, Is.EqualTo(first.Options));
                Assert.That(repeated.ForestOptions, Is.EqualTo(first.ForestOptions));
                Assert.That(repeated.MaterialColours, Is.EqualTo(first.MaterialColours));
                Assert.That(repeated.SnapshotPath, Is.EqualTo(first.SnapshotPath));
                Assert.That(repeated.IslandGridPosition, Is.EqualTo(cell));
                Assert.That(repeated.Descriptor.LogicalXMetres,
                    Is.EqualTo(x * (double)IslandWorldManager.IslandSizeMetres));
                Assert.That(repeated.IslandId, Is.Not.EqualTo(first.IslandId));
                Assert.That(repeated.Generation.TerrainShape, Is.EqualTo(IslandTerrainShape.Island));
            }
        }

        [Test]
        public void SouthernCoastFacesSouthAndReversesItsSectionOrderUsingOneDistinctSharedField()
        {
            using var fixture = new Fixture();
            var factory = fixture.Factory;
            var top = factory.CreateIslandGenerationRequest(new Vector2Int(1, factory.TopCoastRow));
            var bottom = factory.CreateIslandGenerationRequest(new Vector2Int(1, factory.BottomCoastRow));
            var bottomNext = factory.CreateIslandGenerationRequest(new Vector2Int(2, factory.BottomCoastRow));
            var bottomAlias = factory.CreateIslandGenerationRequest(new Vector2Int(9, factory.BottomCoastRow));
            Assert.That(top.RandomSeed, Is.EqualTo(factory.Seed));
            Assert.That(bottom.RandomSeed, Is.Not.EqualTo(top.RandomSeed));
            Assert.That(bottomNext.RandomSeed, Is.EqualTo(bottom.RandomSeed));
            Assert.That(bottomNext.Options, Is.EqualTo(bottom.Options));
            Assert.That(bottomNext.ForestOptions, Is.EqualTo(bottom.ForestOptions));
            Assert.That(bottomNext.MaterialColours, Is.EqualTo(bottom.MaterialColours));
            Assert.That(top.CoastalInlandSouth, Is.False);
            Assert.That(bottom.CoastalInlandSouth, Is.True);
            Assert.That(Vector3.Distance(top.WorldRotation * Vector3.forward, Vector3.forward), Is.LessThan(.00001f));
            Assert.That(Vector3.Distance(bottom.WorldRotation * Vector3.forward, Vector3.back), Is.LessThan(.00001f));
            Assert.That(top.CoastalStripOptions.Value.sectionIndex, Is.EqualTo(1));
            Assert.That(bottom.CoastalStripOptions.Value.sectionIndex, Is.EqualTo(7));
            Assert.That(bottomNext.CoastalStripOptions.Value.sectionIndex, Is.EqualTo(6));
            Assert.That(bottomAlias.CoastalStripOptions, Is.EqualTo(bottom.CoastalStripOptions));
            Assert.That(bottomAlias.SnapshotPath, Is.EqualTo(bottom.SnapshotPath));
            Assert.That(bottomAlias.IslandGridPosition, Is.EqualTo(new Vector2Int(9, factory.BottomCoastRow)));
            Assert.That(bottomAlias.IslandId, Is.Not.EqualTo(bottom.IslandId));
        }

        private static void SetField(object target, string name, object value) =>
            target.GetType().GetField(name, BindingFlags.Instance | BindingFlags.NonPublic).SetValue(target, value);
    }
}
