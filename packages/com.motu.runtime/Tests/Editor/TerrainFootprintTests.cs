using System;
using Motu.Islands;
using Motu.Streaming;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class TerrainFootprintTests
    {
        private GameObject root;
        private TerrainTileStreamer streamer;

        [SetUp]
        public void CreateUnfocusedTerrain()
        {
            root = new GameObject("CPU terrain footprint query");
            root.SetActive(false);
            streamer = root.AddComponent<TerrainTileStreamer>();
        }

        [TearDown]
        public void DestroyTerrain()
        {
            streamer.Dispose();
            Object.DestroyImmediate(root);
        }

        [Test]
        public void InteriorSpikeAndDepressionAreIncludedWhenEveryFootprintCornerIsFlat()
        {
            InstallMap((x, z) => x == 0f && z == 0f ? 17f : x == 1f && z == 1f ? -3f : 5f);

            var sample = Query(new TerrainFootprint(Vector2.zero, new Vector2(8f, 8f)));

            Assert.That(sample.MinimumHeight, Is.EqualTo(-3f).Within(.0001f));
            Assert.That(sample.MaximumHeight, Is.EqualTo(17f).Within(.0001f));
            Assert.That(sample.HeightRange, Is.EqualTo(20f).Within(.0001f));
            Assert.That(sample.MaximumSlopeDegrees, Is.GreaterThan(80f));
            Assert.That(sample.MinimumSeaClearance, Is.EqualTo(-3f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void RotatedRectangleExcludesHighCellsThatOnlyIntersectItsAxisAlignedBounds()
        {
            InstallMap((x, z) => x == 5f && z == 5f ? 100f : 4f);

            var sample = Query(new TerrainFootprint(Vector2.zero, new Vector2(16f, 2f), 45f));

            Assert.That(sample.MaximumHeight, Is.EqualTo(4f).Within(.0001f));
            Assert.That(sample.MinimumHeight, Is.EqualTo(4f).Within(.0001f));
            Assert.That(sample.MaximumSlopeDegrees, Is.EqualTo(0f).Within(.0001f));
            Assert.That(sample.TerrainArea, Is.EqualTo(32d).Within(.0001d));
        }

        [Test]
        public void IntersectingCellUsesAllItsNodesForConservativeHeightAndSlope()
        {
            InstallMap((x, z) => x == 1f && z == 1f ? 20f : 2f);

            var sample = Query(new TerrainFootprint(new Vector2(.1f, .1f), new Vector2(.1f, .1f)));

            Assert.That(sample.MaximumHeight, Is.EqualTo(20f).Within(.0001f));
            Assert.That(sample.MinimumHeight, Is.EqualTo(2f).Within(.0001f));
            Assert.That(sample.MaximumSlopeDegrees, Is.GreaterThan(85f));
        }

        [TestCase(0f)]
        [TestCase(37f)]
        [TestCase(180f)]
        public void TranslatedRotatedTerrainPreservesWorldHeightAndSeaClearance(float yaw)
        {
            InstallMap((x, z) => 7f + x * .2f + z * .1f);
            root.transform.SetPositionAndRotation(new Vector3(-317f, 23f, -811f), Quaternion.Euler(0f, yaw, 0f));
            var center = root.transform.TransformPoint(new Vector3(-6f, 0f, 3f));
            var sample = Query(new TerrainFootprint(new Vector2(center.x, center.z), new Vector2(4f, 2f), yaw));

            // The conservative scan can include the adjacent node at a rounded boundary.
            Assert.That(sample.MinimumHeight, Is.InRange(28f, 29f));
            Assert.That(sample.MaximumHeight, Is.InRange(29.5f, 30.5f));
            Assert.That(sample.MaximumSlopeDegrees,
                Is.EqualTo(Mathf.Atan(Mathf.Sqrt(.05f)) * Mathf.Rad2Deg).Within(.002f));
            Assert.That(sample.MinimumSeaClearance, Is.EqualTo(sample.MinimumHeight - 23f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.True);
        }

        [Test]
        public void EntirelySubmergedTerrainStillHasAHeightRangeAndCoverage()
        {
            InstallMap((x, z) => -6f);
            var sample = Query(new TerrainFootprint(new Rect(-2f, -3f, 4f, 6f)));

            Assert.That(sample.HasTerrain, Is.True);
            Assert.That(sample.MinimumHeight, Is.EqualTo(-6f).Within(.0001f));
            Assert.That(sample.MaximumHeight, Is.EqualTo(-6f).Within(.0001f));
            Assert.That(sample.CoverageFraction, Is.EqualTo(1f));
            Assert.That(sample.MinimumSeaClearance, Is.EqualTo(-6f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void TerrainAtTheSeaReferencePlaneIsNotSubmerged()
        {
            InstallMap((x, z) => 0f);
            var sample = Query(new TerrainFootprint(Vector2.zero, new Vector2(2f, 2f)));

            Assert.That(sample.MinimumSeaClearance, Is.EqualTo(0f));
            Assert.That(sample.IsDryLand, Is.True);
        }

        [Test]
        public void ExactDomainCoverageDoesNotDoubleCountBoundaryCells()
        {
            InstallMap((x, z) => 5f);
            var sample = Query(new TerrainFootprint(new Rect(-32f, -32f, 64f, 64f)));

            Assert.That(sample.RequestedArea, Is.EqualTo(4096d));
            Assert.That(sample.TerrainArea, Is.EqualTo(4096d).Within(.0001d));
            Assert.That(sample.OpenSeaArea, Is.Zero);
            Assert.That(sample.UnknownArea, Is.EqualTo(0d).Within(.0001d));
            Assert.That(sample.CoverageFraction, Is.EqualTo(1f));
            Assert.That(sample.TestedCellCount, Is.LessThanOrEqualTo(4096));
        }

        [Test]
        public void CrossingTheStandaloneMapEdgeLeavesUnknownCoverage()
        {
            InstallMap((x, z) => 5f);
            var status = streamer.QueryTerrainFootprint(new TerrainFootprint(new Rect(30f, -2f, 4f, 4f)),
                out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Incomplete));
            Assert.That(sample.TerrainArea, Is.EqualTo(8d).Within(.0001d));
            Assert.That(sample.UnknownArea, Is.EqualTo(8d).Within(.0001d));
            Assert.That(sample.OpenSeaArea, Is.Zero);
            Assert.That(sample.CoverageFraction, Is.EqualTo(.5f).Within(.0001f));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void TinyOutOfMapSliverCannotBeRoundedIntoCompleteDryLand()
        {
            InstallMap((x, z) => 5f, 2000f);
            var status = streamer.QueryTerrainFootprint(
                new TerrainFootprint(new Vector2(950.00006f, 0f), new Vector2(100f, 10f)), out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Incomplete));
            Assert.That(sample.UnknownArea, Is.GreaterThan(0d));
            Assert.That(sample.IsComplete, Is.False);
            Assert.That(sample.IsDryLand, Is.False);
        }

        [TestCase(0)]
        [TestCase(-1)]
        public void NonPositiveWorkLimitsAreInvalid(int maximumCellTests)
        {
            InstallMap((x, z) => 5f);
            var status = streamer.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, Vector2.one),
                out var sample, maximumCellTests);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.InvalidInput));
            Assert.That(sample.TestedCellCount, Is.Zero);
            Assert.That(sample.IsDryLand, Is.False);
        }

        [Test]
        public void HeightExtremaCrossThePreparedMapSampleBlockBoundary()
        {
            const int tileSamples = 9;
            const int dimension = 513;
            var boundary = IslandPreparedColliderHeightMap.SamplesPerBlock;
            var x = boundary % dimension - 256f;
            var z = boundary / dimension - 256f;
            InstallMap((px, pz) => px == x && pz == z ? 41f : 3f, 512f, tileSamples);

            var sample = Query(new TerrainFootprint(new Vector2(x, z), new Vector2(2f, 2f)));

            Assert.That(sample.MaximumHeight, Is.EqualTo(41f).Within(.0001f));
            Assert.That(sample.MinimumHeight, Is.EqualTo(3f).Within(.0001f));
            Assert.That(sample.MaximumSlopeDegrees, Is.GreaterThan(85f));
        }

        [Test]
        public void QueriesDoNotCreateCollidersOrChangeStreamingFocus()
        {
            InstallMap((x, z) => 5f);
            var requested = streamer.requestedLod1;
            Query(new TerrainFootprint(Vector2.zero, new Vector2(6f, 6f)));

            Assert.That(streamer.requestedLod1, Is.EqualTo(requested));
            Assert.That(streamer.TerrainColliderCount, Is.Zero);
            Assert.That(streamer.Lod0GroupCount, Is.Zero);
            Assert.That(streamer.Lod1GroupCount, Is.Zero);
            Assert.That(streamer.HasPendingTransition, Is.False);
            Assert.That(root.GetComponentsInChildren<Collider>(true), Is.Empty);
        }

        [Test]
        public void WorkLimitProducesAnExplicitIncompleteResult()
        {
            InstallMap((x, z) => 5f);
            var status = streamer.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, new Vector2(20f, 20f)),
                out var sample, 7);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.WorkLimitExceeded));
            Assert.That(sample.Status, Is.EqualTo(status));
            Assert.That(sample.IsComplete, Is.False);
            Assert.That(sample.TestedCellCount, Is.LessThanOrEqualTo(7));
            Assert.That(sample.UnknownArea, Is.GreaterThan(0d));
            Assert.That(sample.IsDryLand, Is.False);
        }

        [TestCase(0f, 2f, 0f)]
        [TestCase(-1f, 2f, 0f)]
        [TestCase(float.NaN, 2f, 0f)]
        [TestCase(float.PositiveInfinity, 2f, 0f)]
        [TestCase(2f, 2f, float.NaN)]
        [TestCase(2f, 2f, float.NegativeInfinity)]
        public void InvalidFootprintsNeverReportDryLand(float width, float depth, float yaw)
        {
            InstallMap((x, z) => 5f);
            var status = streamer.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, new Vector2(width, depth), yaw),
                out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.InvalidInput));
            Assert.That(sample.IsComplete, Is.False);
            Assert.That(sample.IsDryLand, Is.False);
            Assert.That(sample.TestedCellCount, Is.Zero);
        }

        [TestCase(2f, 1f, 1f, 0f)]
        [TestCase(-1f, 1f, 1f, 0f)]
        [TestCase(1f, 1f, 1f, 12f)]
        public void ScaledMirroredOrTiltedTerrainReturnsUnsupportedTransform(float x, float y, float z, float pitch)
        {
            InstallMap((px, pz) => 5f);
            root.transform.localScale = new Vector3(x, y, z);
            root.transform.rotation = Quaternion.Euler(pitch, 0f, 0f);
            var status = streamer.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, Vector2.one), out var sample);

            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.UnsupportedTransform));
            Assert.That(sample.IsDryLand, Is.False);
            Assert.That(sample.IsComplete, Is.False);
        }

        [Test]
        public void ShearedHierarchyWithUnitLengthAxesReturnsUnsupportedTransform()
        {
            InstallMap((x, z) => 5f);
            var parent = new GameObject("Sheared terrain parent");
            try
            {
                root.transform.SetParent(parent.transform, false);
                parent.transform.localScale = new Vector3(2f, 1f, .5f);
                root.transform.localRotation = Quaternion.Euler(0f, 45f, 0f);
                var inverseLength = 1f / Mathf.Sqrt(2.125f);
                root.transform.localScale = new Vector3(inverseLength, 1f, inverseLength);
                var matrix = root.transform.localToWorldMatrix;
                Assert.That(((Vector3)matrix.GetColumn(0)).magnitude, Is.EqualTo(1f).Within(.0001f));
                Assert.That(((Vector3)matrix.GetColumn(2)).magnitude, Is.EqualTo(1f).Within(.0001f));
                Assert.That(Vector3.Dot(matrix.GetColumn(0), matrix.GetColumn(2)), Is.GreaterThan(.5f));

                var status = streamer.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, Vector2.one), out var sample);

                Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.UnsupportedTransform));
                Assert.That(sample.IsDryLand, Is.False);
            }
            finally
            {
                root.transform.SetParent(null, false);
                Object.DestroyImmediate(parent);
            }
        }

        private TerrainFootprintSample Query(TerrainFootprint footprint)
        {
            var status = streamer.QueryTerrainFootprint(footprint, out var sample);
            Assert.That(status, Is.EqualTo(TerrainFootprintQueryStatus.Complete));
            Assert.That(sample.Status, Is.EqualTo(status));
            Assert.That(sample.IsComplete, Is.True);
            return sample;
        }

        private void InstallMap(Func<float, float, float> height, float worldSize = 64f, int tileSamples = 2)
        {
            streamer.worldSize = worldSize;
            streamer.colliderHeightMap = MakeMap(worldSize, tileSamples, height);
        }

        internal static IslandPreparedColliderHeightMap MakeMap(float worldSize, int tileSamples,
            Func<float, float, float> height)
        {
            var dimension = TerrainTileStreamer.Lod1Resolution * (tileSamples - 1) + 1;
            var samples = IslandPreparedColliderHeightMap.AllocateSamples(dimension * dimension);
            for (var z = 0; z < dimension; z++)
            for (var x = 0; x < dimension; x++)
            {
                var index = z * dimension + x;
                var localX = -worldSize * .5f + x * worldSize / (dimension - 1);
                var localZ = -worldSize * .5f + z * worldSize / (dimension - 1);
                samples[index / IslandPreparedColliderHeightMap.SamplesPerBlock]
                    [index % IslandPreparedColliderHeightMap.SamplesPerBlock] = height(localX, localZ) / worldSize;
            }
            return new IslandPreparedColliderHeightMap(dimension, tileSamples, samples, worldSize);
        }
    }
}
