using System;
using System.Reflection;
using System.Runtime.InteropServices;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalStripGenerationTests
    {
        [Test]
        public void ShapeDefaultsToIslandAndCoastalScalarsStayInsideNativeLimits()
        {
            var settings = new IslandGenerationSettings();
            Assert.That(settings.TerrainShape, Is.EqualTo(IslandTerrainShape.Island));
            Assert.That(settings.ToNativeCoastalStripOptions(Vector2Int.zero), Is.Null);
            Assert.Throws<ArgumentOutOfRangeException>(() => settings.TerrainShape = (IslandTerrainShape)123);
            settings.TerrainShape = IslandTerrainShape.CoastalStrip;
            settings.CoastalSectionCount = int.MaxValue;
            settings.CoastalSeamSegments = int.MinValue;
            settings.CoastalSeamBlendWidthMetres = float.NaN;
            settings.CoastalSeamRidgeHeightMetres = float.PositiveInfinity;
            var native = settings.ToNativeCoastalStripOptions(Vector2Int.zero).Value;
            Assert.That(native.sectionCount, Is.EqualTo(4096));
            Assert.That(native.seamSegments, Is.EqualTo(8));
            Assert.That(native.seamBlendWidth, Is.EqualTo(.08f).Within(.000001f));
            Assert.That(native.seamRidgeHeight, Is.EqualTo(.02f).Within(.000001f));
            settings.MaximumHeightMetres = 5f;
            Assert.That(settings.ToNativeCoastalStripOptions(Vector2Int.zero).Value.seamRidgeHeight,
                Is.LessThanOrEqualTo(settings.MaximumHeightNormalized));
        }

        [TestCase(-9, 7)]
        [TestCase(-8, 0)]
        [TestCase(-1, 7)]
        [TestCase(0, 0)]
        [TestCase(7, 7)]
        [TestCase(8, 0)]
        [TestCase(17, 1)]
        public void SectionsUsePositiveModuloInBothWorldDirections(int x, int expected)
        {
            var settings = new IslandGenerationSettings { TerrainShape = IslandTerrainShape.CoastalStrip };
            Assert.That(settings.ToNativeCoastalStripOptions(new Vector2Int(x, 0)).Value.sectionIndex,
                Is.EqualTo((uint)expected));
        }

        [Test]
        public void CoastDiscoverySharesOneFieldAndPlacesRepeatedSectionsInTheirOwnWorldCells()
        {
            var host = new GameObject("Coastal strip factory tests");
            try
            {
                var factory = CreateFactory(host);
                var row = factory.TopCoastRow;
                Assert.That(factory.HasIsland(factory.StartingCell), Is.False);
                Assert.That(factory.HasIsland(new Vector2Int(int.MinValue, row)), Is.True);
                Assert.That(factory.HasIsland(new Vector2Int(int.MaxValue, factory.BottomCoastRow)), Is.True);
                Assert.That(factory.HasIsland(new Vector2Int(0, row + 1)), Is.False);
                var first = factory.CreateIslandGenerationRequest(new Vector2Int(0, row));
                var next = factory.CreateIslandGenerationRequest(new Vector2Int(1, row));
                var repeated = factory.CreateIslandGenerationRequest(new Vector2Int(8, row));
                var west = factory.CreateIslandGenerationRequest(new Vector2Int(-8, row));
                Assert.That(first.RandomSeed, Is.EqualTo(factory.Seed));
                Assert.That(next.RandomSeed, Is.EqualTo(first.RandomSeed));
                Assert.That(next.Options, Is.EqualTo(first.Options), "Neighbouring tiles use one original field.");
                Assert.That(next.MaterialColours, Is.EqualTo(first.MaterialColours));
                Assert.That(next.SnapshotPath, Is.Not.EqualTo(first.SnapshotPath));
                Assert.That(repeated.SnapshotPath, Is.EqualTo(first.SnapshotPath));
                Assert.That(west.SnapshotPath, Is.EqualTo(first.SnapshotPath));
                Assert.That(repeated.IslandId, Is.Not.EqualTo(first.IslandId));
                Assert.That(repeated.Descriptor.LogicalXMetres - first.Descriptor.LogicalXMetres,
                    Is.EqualTo(8d * IslandWorldManager.IslandSizeMetres));
                Assert.That(first.Descriptor.LogicalZMetres, Is.EqualTo(row * (double)IslandWorldManager.IslandSizeMetres));
                Assert.That(first.Descriptor.EstimatedBoundingRadiusMetres,
                    Is.EqualTo(IslandWorldManager.IslandSizeMetres * Mathf.Sqrt(.5f)).Within(.001f));
                factory.CoastalStripGeneration.MaximumHeightMetres = 100f;
                Assert.That(first.Generation.MaximumHeightMetres, Is.EqualTo(400f),
                    "Queued request profiles must be isolated from later factory edits.");
            }
            finally { Object.DestroyImmediate(host); }
        }

        [Test]
        public void EveryCoastalSettingAndShapeParticipatesInTheSnapshotKey()
        {
            var settings = new IslandGenerationSettings { TerrainShape = IslandTerrainShape.CoastalStrip };
            var initial = Request(settings).SnapshotPath;
            Assert.That(Request(settings).SnapshotPath, Is.EqualTo(initial));
            AssertChanged(settings, s => s.CoastalSectionCount = 16, initial);
            AssertChanged(settings, s => s.CoastalSeamSegments = 64, initial);
            AssertChanged(settings, s => s.CoastalSeamBlendWidthMetres = 200f, initial);
            AssertChanged(settings, s => s.CoastalSeamRidgeHeightMetres = 20f, initial);
            AssertChanged(settings, s => s.TerrainShape = IslandTerrainShape.Island, initial);
        }

        [Test]
        public void SeparateCoastalOptionBlockHasTheDocumentedScalarLayout()
        {
            Assert.That(Marshal.SizeOf<CoastalStripNative.Options>(), Is.EqualTo(20));
            Assert.That(Marshal.OffsetOf<CoastalStripNative.Options>("sectionIndex").ToInt32(), Is.Zero);
            Assert.That(Marshal.OffsetOf<CoastalStripNative.Options>("sectionCount").ToInt32(), Is.EqualTo(4));
            Assert.That(Marshal.OffsetOf<CoastalStripNative.Options>("seamSegments").ToInt32(), Is.EqualTo(8));
            Assert.That(Marshal.OffsetOf<CoastalStripNative.Options>("seamBlendWidth").ToInt32(), Is.EqualTo(12));
            Assert.That(Marshal.OffsetOf<CoastalStripNative.Options>("seamRidgeHeight").ToInt32(), Is.EqualTo(16));
        }

        [Test, Category("NativeIntegration")]
        public void NativeCoastalLibraryMatchesTheCurrentTerrainAlgorithm()
        {
            Assert.DoesNotThrow(CoastalStripNative.EnsureCompatible);
        }

        [Test]
        public void CoastalMaterialCoordinatesMeetAcrossSectionsAndSurviveOriginMoves()
        {
            var left = IslandDescriptor.Request(42, new Vector2Int(7, 3), null);
            var right = IslandDescriptor.Request(42, new Vector2Int(8, 3), null);
            var leftOffset = (Vector3)IslandGenerator.TerrainFieldOffset(left, IslandTerrainShape.CoastalStrip);
            var rightOffset = (Vector3)IslandGenerator.TerrainFieldOffset(right, IslandTerrainShape.CoastalStrip);
            var halfSize = IslandWorldManager.IslandSizeMetres * .5f;
            var leftLocal = new Vector3(halfSize, 12f, 105f);
            var rightLocal = new Vector3(-halfSize, 12f, 105f);
            Assert.That(leftLocal + leftOffset, Is.EqualTo(rightLocal + rightOffset));
            Assert.That(leftOffset.y, Is.Zero, "Field offsets must not change the island-local sea elevation.");
            var shiftedRoot = Matrix4x4.Translate(new Vector3(-8000f, -25f, 2400f));
            var shiftedWorld = shiftedRoot.MultiplyPoint3x4(leftLocal);
            Assert.That(shiftedRoot.inverse.MultiplyPoint3x4(shiftedWorld) + leftOffset,
                Is.EqualTo(leftLocal + leftOffset));
            Assert.That(IslandGenerator.TerrainFieldOffset(left, IslandTerrainShape.Island), Is.EqualTo(Vector4.zero));
            var settings = new IslandGenerationSettings { TerrainShape = IslandTerrainShape.CoastalStrip };
            Assert.That(IslandGenerator.TerrainSeamSettings(settings), Is.EqualTo(new Vector4(1000f, 160f, 0f, 0f)));
            settings.TerrainShape = IslandTerrainShape.Island;
            Assert.That(IslandGenerator.TerrainSeamSettings(settings), Is.EqualTo(Vector4.zero));
        }

        private static void AssertChanged(IslandGenerationSettings original,
            Action<IslandGenerationSettings> change, string initial)
        {
            var settings = original.Copy();
            change(settings);
            Assert.That(Request(settings).SnapshotPath, Is.Not.EqualTo(initial));
        }

        private static IslandGenerationRequest Request(IslandGenerationSettings generation) =>
            new IslandGenerationRequest(42, Vector2Int.zero, generation,
                new IslandRiverSettings(), new IslandForestSettings(), new IslandReedSettings(),
                new IslandFernSettings(), new IslandRenderingSettings(), default);

        private static CoherentIslandFactory CreateFactory(GameObject host)
        {
            var factory = host.AddComponent<CoherentIslandFactory>();
            factory.Seed = 42;
            foreach (var value in new (string name, object settings)[]
            {
                ("riverSettings", new IslandRiverSettings()),
                ("renderingSettings", new IslandRenderingSettings()),
                ("reedSettings", new IslandReedSettings()),
                ("fernSettings", new IslandFernSettings()),
            })
                typeof(CoherentIslandFactory).GetField(value.name, BindingFlags.Instance | BindingFlags.NonPublic)
                    .SetValue(factory, value.settings);
            return factory;
        }
    }
}
