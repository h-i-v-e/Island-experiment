using System;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class BoulderTests
    {
        [Test]
        public void NativeCentresAndRadiiMapToUnityAndExactlyOneOwnerTile()
        {
            var buckets = TerrainPreparation.BucketBoulderColliders(new[]
            {
                new Vector4(.25f, .75f, .05f, .001f),
                new Vector4(1, 1, .02f, .0015f),
            }, 2000);
            Assert.That(buckets.Length, Is.EqualTo(64 * 64));
            var sphere = buckets[48 * 64 + 16][0];
            Assert.That(sphere.centre, Is.EqualTo(new Vector3(-500, 100, 500)));
            Assert.That(sphere.radius, Is.EqualTo(2));
            Assert.That(buckets[63 * 64 + 63].Length, Is.EqualTo(1));
            var count = 0;
            foreach (var bucket in buckets) count += bucket.Length;
            Assert.That(count, Is.EqualTo(2));
            Assert.Throws<InvalidOperationException>(() => TerrainPreparation.BucketBoulderColliders(
                new[] { new Vector4(.5f, .5f, 1, -1) }, 2000));
            Assert.Throws<InvalidOperationException>(() => TerrainPreparation.BucketBoulderColliders(
                new[] { new Vector4(float.NaN, .5f, 1, 1) }, 2000));
        }

        [Test]
        public void SpheresExistOnlyAtLod0AndRetireWithTheTerrainGroup()
        {
            var host = new GameObject("Boulder streaming test");
            var streamer = host.AddComponent<TerrainTileStreamer>();
            var coarse = new TerrainTileStreamer.TileGroup(new GameObject("LOD 1"), Array.Empty<TerrainTileStreamer.Tile>(), 0);
            var fine = new TerrainTileStreamer.TileGroup(new GameObject("LOD 0"), Array.Empty<TerrainTileStreamer.Tile>(), 0);
            try
            {
                coarse.root.transform.SetParent(host.transform, false);
                fine.root.transform.SetParent(host.transform, false);
                streamer.preparedBoulderColliders = TerrainPreparation.BucketBoulderColliders(
                    new[] { new Vector4(.25f, .75f, .05f, .001f) }, 2000);
                var owner = new Vector2Int(16, 48);
                streamer.SetRocksVisible(true);
                streamer.ConfigureBoulderColliders(coarse, 1, owner);
                Assert.That(coarse.root.GetComponentsInChildren<SphereCollider>(true), Is.Empty);
                streamer.ConfigureBoulderColliders(fine, 0, owner);
                streamer.lod0Groups.Add(owner, fine);
                var spheres = fine.root.GetComponentsInChildren<SphereCollider>();
                Assert.That(spheres.Length, Is.EqualTo(1));
                var sphere = spheres[0];
                Assert.That(sphere.radius, Is.EqualTo(2));
                Assert.That(sphere.transform.localPosition, Is.EqualTo(new Vector3(-500, 100, 500)));
                Assert.IsNull(sphere.GetComponent<Rigidbody>(), "Settling happens during generation, not at runtime.");
                streamer.SetRocksVisible(false);
                Assert.IsFalse(sphere.gameObject.activeInHierarchy);
                streamer.SetRocksVisible(true);
                Assert.IsTrue(sphere.gameObject.activeInHierarchy);
                fine.root.SetActive(false);
                Assert.IsFalse(sphere.gameObject.activeInHierarchy, "Pending/inactive groups cannot collide.");
                fine.root.SetActive(true);
                streamer.ClearPlayerFocus();
                Assert.That(host.GetComponentsInChildren<SphereCollider>(true), Is.Empty);
                Assert.That(streamer.lod0Groups.Count, Is.Zero);
            }
            finally
            {
                streamer.Dispose();
                TerrainTileStreamer.DestroyGroup(coarse);
                Object.DestroyImmediate(host);
            }
        }

        [Test]
        public void RockRenderBatchSurvivesLod0RetirementUntilLod1Leaves()
        {
            var host = new GameObject("Rock feature rendering test");
            var material = new Material(Shader.Find("Motu/Rock Decoration"));
            TerrainTileStreamer.TileGroup group = null;
            try
            {
                var prepared = new IslandPreparedMesh[64 * 64];
                prepared[0] = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.right, Vector3.forward },
                    new[] { Vector3.up, Vector3.up, Vector3.up }, new[] { 0, 1, 2 },
                    Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
                group = TerrainTileStreamer.CreatePreparedFeatureGroup(Vector2Int.zero, prepared, host, material, "Boulder");
                var renderers = group.root.GetComponentsInChildren<MeshRenderer>();
                Assert.That(renderers.Length, Is.EqualTo(1));
                Assert.That(renderers[0].sharedMaterial, Is.SameAs(material));
                Assert.That(group.root.GetComponentsInChildren<Collider>(), Is.Empty);
                group.root.SetActive(false); // LOD2
                Assert.IsFalse(renderers[0].gameObject.activeInHierarchy);
                group.root.SetActive(true); // LOD1 and LOD0 share this feature group.
                Assert.IsTrue(renderers[0].gameObject.activeInHierarchy);
            }
            finally { TerrainTileStreamer.DestroyGroup(group); Object.DestroyImmediate(material); Object.DestroyImmediate(host); }
        }

        [Test]
        public void BoulderNativeExportAcceptsNullIslandAndReleasesSafely()
        {
            MotuNative.CreateBoulderColliders(IntPtr.Zero, out var export);
            Assert.That(export.handle, Is.EqualTo(IntPtr.Zero));
            Assert.That(export.data, Is.EqualTo(IntPtr.Zero));
            Assert.That(export.length, Is.Zero);
            MotuNative.ReleaseBoulderColliders(ref export);
        }
    }
}
