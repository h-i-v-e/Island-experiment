using System;
using System.Collections;
using System.Runtime.CompilerServices;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

namespace Motu.Tests
{
    public sealed class ColliderHeightMapTests
    {
        private const int TileSamples = 17;
        private const int Dimension = 1025;
        private const float WorldSize = 2000f;
        private IslandPreparedColliderHeightMap heightMap;

        [OneTimeSetUp]
        public void PrepareAsymmetricFieldAcrossSeveralBlocks()
        {
            var blocks = IslandPreparedColliderHeightMap.AllocateSamples(Dimension * Dimension);
            var offset = 0;
            foreach (var block in blocks)
            {
                for (var index = 0; index < block.Length; index++, offset++)
                    block[index] = NativeHeight(offset % Dimension, offset / Dimension);
            }
            heightMap = new IslandPreparedColliderHeightMap(Dimension, TileSamples, blocks, WorldSize);
        }

        [OneTimeTearDown]
        public void ReleaseField() => heightMap = null;

        [Test]
        public void WorldHeightPreservesSamplesOnBothSidesOfEveryBlockBoundary()
        {
            for (var boundary = IslandPreparedColliderHeightMap.SamplesPerBlock;
                boundary < Dimension * Dimension;
                boundary += IslandPreparedColliderHeightMap.SamplesPerBlock)
            {
                for (var delta = -1; delta <= 1; delta++)
                {
                    var sample = boundary + delta;
                    var x = sample % Dimension;
                    var y = sample / Dimension;
                    Assert.That(heightMap.WorldHeightAt(x, y),
                        Is.EqualTo(NativeHeight(x, y) * WorldSize).Within(.00003f));
                }
            }
        }

        [TestCase(48, 15, false)]
        [TestCase(48, 15, true)]
        [TestCase(63, 63, false)]
        [TestCase(63, 63, true)]
        [TestCase(0, 0, false)]
        public void TileSamplesPreserveNormalizationAndAxisReversal(int tileX, int tileY, bool reverse)
        {
            var tile = heightMap.CopyTileHeights(new Vector2Int(tileX, tileY), reverse);
            for (var y = 0; y < TileSamples; y++)
            for (var x = 0; x < TileSamples; x++)
            {
                var sourceX = tileX * (TileSamples - 1) + (reverse ? TileSamples - 1 - x : x);
                var sourceY = tileY * (TileSamples - 1) + (reverse ? TileSamples - 1 - y : y);
                var expected = (NativeHeight(sourceX, sourceY) * WorldSize - heightMap.verticalOrigin)
                    / heightMap.verticalSize;
                Assert.That(tile[y, x], Is.EqualTo(expected).Within(1e-7f), $"Sample ({sourceX}, {sourceY})");
            }
        }

        [Test]
        public void InvalidOrNonFiniteBlocksAreRejected()
        {
            Assert.Throws<InvalidOperationException>(() =>
                new IslandPreparedColliderHeightMap(Dimension, TileSamples, null, WorldSize));
            Assert.Throws<InvalidOperationException>(() =>
                new IslandPreparedColliderHeightMap(Dimension, TileSamples,
                    new[] { new float[1] }, WorldSize));
            var blocks = IslandPreparedColliderHeightMap.AllocateSamples(Dimension * Dimension);
            blocks[blocks.Length - 1][0] = float.NaN;
            Assert.Throws<InvalidOperationException>(() =>
                new IslandPreparedColliderHeightMap(Dimension, TileSamples, blocks, WorldSize));
        }

        [UnityTest]
        public IEnumerator FullResolutionMapReleasesEverySampleBlockAfterUnload()
        {
            var samples = UnownedFullResolutionSamples();
            yield return null;
            GC.Collect();
            GC.WaitForPendingFinalizers();
            GC.Collect();
            foreach (var sample in samples)
                Assert.IsFalse(sample.IsAlive, "Unloaded full-resolution collision samples must be collectible.");
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private static WeakReference[] UnownedFullResolutionSamples()
        {
            var dimension = TerrainTileStreamer.Lod1Resolution
                * (TerrainTileStreamer.ColliderSamplesPerTile - 1) + 1;
            var blocks = IslandPreparedColliderHeightMap.AllocateSamples(checked(dimension * dimension));
            var references = new WeakReference[blocks.Length];
            for (var index = 0; index < blocks.Length; index++)
            {
                Assert.That(blocks[index].Length * sizeof(float), Is.LessThanOrEqualTo(1024 * 1024));
                references[index] = new WeakReference(blocks[index]);
            }
            var map = new IslandPreparedColliderHeightMap(dimension,
                TerrainTileStreamer.ColliderSamplesPerTile, blocks, WorldSize);
            GC.KeepAlive(map);
            return references;
        }

        private static float NativeHeight(int x, int y) => ((x * 17 + y * 31) % 10007 - 5000) / 100000f;
    }
}
