using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using Unity.Profiling;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ForestStreamingTests
    {
        [Test]
        public void IslandsShareInstancedModelsWithoutCreatingAnotherVisualTreeSystem()
        {
            var data = ForestData();
            using var first = new ForestFixture(data);
            using var second = new ForestFixture(data);
            var firstRenderer = first.Renderer;
            Assert.AreSame(firstRenderer.Models, second.Renderer.Models);
            Assert.AreEqual(3, firstRenderer.InstanceCount);
            Assert.AreEqual(3, firstRenderer.PrototypeCount);
            Assert.AreEqual(1, first.Host.GetComponentsInChildren<InstancedForestRenderer>(true).Length);
            Assert.IsEmpty(first.Host.GetComponentsInChildren<MeshRenderer>(true));
            Assert.IsEmpty(first.Host.GetComponentsInChildren<MeshFilter>(true));
            Assert.AreEqual(0, first.Streamer.ColliderTileCount);
            var sharedMesh = second.Renderer.Models.meshes[0][0];
            first.Streamer.Dispose();
            Assert.IsTrue(sharedMesh != null, "The other island retains its shared GPU catalog.");
            second.Streamer.Dispose();
            Assert.IsTrue(sharedMesh == null, "The final island releases its prototype meshes.");
            Assert.DoesNotThrow(first.Streamer.Dispose);
            Assert.DoesNotThrow(second.Streamer.Dispose);
        }

        [Test]
        public void OnlyTheFinestNeighborhoodOwnsCollidersAndClearingKeepsVisualInstances()
        {
            using var scene = new ForestFixture(ForestData(), CollidersEverywhere());
            foreach (var center in new[] { new Vector2Int(24, 24), new Vector2Int(25, 24), Vector2Int.zero,
                new Vector2Int(63, 63), new Vector2Int(24, 24) })
            {
                Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(center, () => true));
                var count = center.x == 0 || center.x == 63 ? 4 : 9;
                Assert.IsTrue(scene.Streamer.Lod0NeighborhoodMatches(center));
                Assert.AreEqual(count, scene.Streamer.ColliderTileCount);
                Assert.AreEqual(count, scene.Streamer.ActiveTrunkColliderCount);
                Assert.AreEqual(count, scene.Streamer.ActiveLogColliderCount);
                Assert.AreEqual(count, scene.Streamer.Root.GetComponentsInChildren<CapsuleCollider>(true).Length);
                Assert.AreEqual(count, scene.Streamer.Root.GetComponentsInChildren<BoxCollider>(true).Length);
            }
            var models = scene.Renderer.Models;
            scene.Streamer.ClearPlayerFocus();
            Assert.AreEqual(0, scene.Streamer.ColliderTileCount);
            Assert.AreEqual(0, scene.Streamer.ActiveTrunkColliderCount);
            Assert.AreEqual(0, scene.Streamer.ActiveLogColliderCount);
            Assert.IsEmpty(scene.Streamer.Root.GetComponentsInChildren<Collider>(true));
            Assert.AreSame(models, scene.Renderer.Models);
            Assert.AreEqual(3, scene.Renderer.InstanceCount);
            Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(24, 24), () => true));
            Assert.AreEqual(9, scene.Streamer.ColliderTileCount);
            Assert.IsTrue(scene.Host.GetComponentsInChildren<Transform>(true).All(item => item.gameObject.layer == 27));
        }

        [Test]
        public void ReadinessMatchesTheExactClippedNeighborhoodIncludingOutsideCentres()
        {
            using var scene = new ForestFixture(ForestData());
            foreach (var resident in NeighborhoodCenters)
            {
                Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(resident, () => true));
                var expected = NeighborhoodKeys(resident);
                Assert.AreEqual(expected.Count, scene.Streamer.ColliderTileCount, $"Resident centre {resident}");
                foreach (var requested in NeighborhoodCenters)
                    Assert.AreEqual(expected.SetEquals(NeighborhoodKeys(requested)), scene.Streamer.Lod0NeighborhoodMatches(requested),
                        $"Resident centre {resident}, requested centre {requested}");
            }
            scene.Streamer.Dispose();
            Assert.IsFalse(scene.Streamer.Lod0NeighborhoodMatches(Vector2Int.zero));
            Assert.IsFalse(scene.Streamer.Lod0NeighborhoodMatches(new Vector2Int(-2, 8)));
            using var uninitialized = new ForestTileStreamer();
            Assert.IsFalse(uninitialized.Lod0NeighborhoodMatches(new Vector2Int(-2, 8)));
        }

        [Test]
        public void RepeatedReadinessChecksAllocateNoManagedObjects()
        {
            using var scene = new ForestFixture(ForestData());
            var center = new Vector2Int(24, 24);
            var adjacent = center + Vector2Int.right;
            var outside = new Vector2Int(-2, 8);
            Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(center, () => true));
            for (var i = 0; i < 64; ++i)
            {
                scene.Streamer.Lod0NeighborhoodMatches(center);
                scene.Streamer.Lod0NeighborhoodMatches(adjacent);
                scene.Streamer.Lod0NeighborhoodMatches(outside);
            }
            using (var calibration = AllocationRecorder())
            {
                var retained = new byte[64 * 1024]; retained[0] = 1;
                GC.KeepAlive(retained);
                calibration.Stop();
                Assert.That(AllocationBlocks(calibration), Is.GreaterThan(0), "The counter must observe a real retained 64KB allocation.");
            }
            var correct = true;
            using (var measured = AllocationRecorder())
            {
                for (var i = 0; i < 1024; ++i)
                {
                    correct &= scene.Streamer.Lod0NeighborhoodMatches(center);
                    correct &= !scene.Streamer.Lod0NeighborhoodMatches(adjacent);
                    correct &= !scene.Streamer.Lod0NeighborhoodMatches(outside);
                }
                measured.Stop();
                Assert.AreEqual(0, AllocationBlocks(measured), "Matching, wrong-key and outside readiness checks must not allocate.");
            }
            Assert.IsTrue(correct);
        }

        private static readonly Vector2Int[] NeighborhoodCenters =
        {
            new Vector2Int(24, 24), new Vector2Int(25, 24), new Vector2Int(0, 24), new Vector2Int(63, 24),
            new Vector2Int(-1, 24), new Vector2Int(64, 24), Vector2Int.zero, new Vector2Int(63, 63),
            new Vector2Int(-1, -1), new Vector2Int(-1, 64), new Vector2Int(64, -1), new Vector2Int(64, 64),
            new Vector2Int(-2, 24), new Vector2Int(65, 24), new Vector2Int(24, -2), new Vector2Int(24, 65),
            new Vector2Int(int.MinValue, 24), new Vector2Int(int.MaxValue - 1, 24),
            new Vector2Int(24, int.MinValue + 1), new Vector2Int(24, int.MaxValue)
        };

        private static readonly Vector2Int[] NeighborhoodOffsets =
        {
            new Vector2Int(-1, -1), new Vector2Int(0, -1), new Vector2Int(1, -1),
            new Vector2Int(-1, 0), Vector2Int.zero, new Vector2Int(1, 0),
            new Vector2Int(-1, 1), new Vector2Int(0, 1), new Vector2Int(1, 1)
        };

        private static HashSet<Vector2Int> NeighborhoodKeys(Vector2Int center)
        {
            var result = new HashSet<Vector2Int>();
            foreach (var offset in NeighborhoodOffsets)
            {
                var x = (long)center.x + offset.x;
                var y = (long)center.y + offset.y;
                if (x >= 0 && y >= 0 && x < 64 && y < 64) result.Add(new Vector2Int((int)x, (int)y));
            }
            return result;
        }

        private static ProfilerRecorder AllocationRecorder() => ProfilerRecorder.StartNew(ProfilerCategory.Internal, "GC.Alloc", 1,
            ProfilerRecorderOptions.SumAllSamplesInFrame | ProfilerRecorderOptions.CollectOnlyOnCurrentThread);

        private static long AllocationBlocks(ProfilerRecorder recorder) => recorder.Count == 0 ? 0 : recorder.GetSample(0).Count;

        [Test]
        public void AuthoredFallenLogsKeepGraphicsWithTheirCollidersAndReleaseOwnedMeshes()
        {
            var logTiles = new IslandPreparedMesh[64];
            logTiles[63] = new IslandPreparedMesh(new[] { LogBottom, LogTop, LogBottom + Vector3.up },
                new[] { Vector3.forward, Vector3.forward, Vector3.forward }, new[] { 0, 1, 2 },
                Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
            using var scene = new ForestFixture(ForestData(), CollidersEverywhere(), logTiles);
            var visual = scene.Streamer.Root.GetComponentsInChildren<MeshRenderer>(true).Single();
            var mesh = visual.GetComponent<MeshFilter>().sharedMesh;
            Assert.AreEqual("Motu/Tree Wood", visual.sharedMaterial.shader.name);
            Assert.AreEqual(UnityEngine.Rendering.ShadowCastingMode.On, visual.shadowCastingMode);
            Assert.IsTrue(visual.receiveShadows);
            Assert.AreEqual(3, mesh.vertexCount);
            Assert.AreEqual(27, visual.gameObject.layer);
            Assert.AreEqual(1, scene.Streamer.Root.GetComponentsInChildren<InstancedForestRenderer>(true).Length);
            Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(24, 24), () => true));
            Assert.AreEqual(9, scene.Streamer.ActiveLogColliderCount);
            scene.Streamer.SetVisible(false);
            Assert.IsFalse(visual.gameObject.activeInHierarchy);
            Assert.IsTrue(scene.Streamer.Root.GetComponentsInChildren<BoxCollider>(true).All(item => !item.gameObject.activeInHierarchy));
            scene.Streamer.SetVisible(true);
            Assert.IsTrue(visual.gameObject.activeInHierarchy);
            Assert.AreEqual(9, scene.Streamer.Root.GetComponentsInChildren<BoxCollider>().Length);
            scene.Streamer.ClearPlayerFocus();
            Assert.AreEqual(0, scene.Streamer.ActiveLogColliderCount);
            Assert.IsTrue(mesh != null, "Authored log graphics remain resident while nearby collision ownership changes.");
            Assert.IsTrue(visual.gameObject.activeInHierarchy);
            scene.Streamer.Dispose();
            Assert.IsTrue(mesh == null, "The streamer releases the uploaded fallen-log mesh.");
            Assert.IsTrue(visual == null);
        }

        [Test]
        public void TrunkCapsulesAndLogBoxesPreserveAuthoredEndpointsAndDimensions()
        {
            using var scene = new ForestFixture(ForestData(), CollidersEverywhere());
            Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(Vector2Int.zero, () => true));
            var capsule = scene.Streamer.Root.GetComponentInChildren<CapsuleCollider>();
            Assert.AreEqual(Vector3.zero, capsule.center);
            Assert.AreEqual(1, capsule.direction);
            Assert.AreEqual(.3f, capsule.radius);
            Assert.AreEqual(TrunkAxis.magnitude, capsule.height, .00001f);
            Assert.That(Vector3.Distance(capsule.transform.localPosition, (TrunkBottom + TrunkTop) * .5f), Is.LessThan(.00001f));
            Assert.That(Vector3.Dot(capsule.transform.localRotation * Vector3.up, TrunkAxis.normalized), Is.GreaterThan(.99999f));
            var box = scene.Streamer.Root.GetComponentInChildren<BoxCollider>();
            Assert.AreEqual(new Vector3(.4f, LogAxis.magnitude, .4f), box.size);
            Assert.That(Vector3.Distance(box.transform.localPosition, (LogBottom + LogTop) * .5f), Is.LessThan(.00001f));
            Assert.That(Vector3.Dot(box.transform.localRotation * Vector3.up, LogAxis.normalized), Is.GreaterThan(.99999f));
        }

        [Test]
        public void DisposingAPartialColliderUploadDestroysItsInactivePendingTile()
        {
            using var scene = new ForestFixture(ForestData(), HeavyFirstTile());
            var routine = Flatten(scene.Streamer.UpdateLod0NeighborhoodIncremental(Vector2Int.zero, () => true));
            try
            {
                Assert.IsTrue(routine.MoveNext());
                Assert.AreEqual(0, scene.Streamer.ColliderTileCount);
                Assert.AreEqual(0, scene.Streamer.ActiveTrunkColliderCount);
                var pending = scene.Streamer.Root.GetComponentsInChildren<CapsuleCollider>(true);
                Assert.AreEqual(32, pending.Length, "Even a large time budget permits only 32 colliders before yielding.");
                Assert.IsTrue(pending.All(collider => !collider.gameObject.activeInHierarchy));
            }
            finally { (routine as IDisposable)?.Dispose(); }
            Assert.IsEmpty(scene.Streamer.Root.GetComponentsInChildren<Collider>(true));
            Assert.AreEqual(0, scene.Streamer.Root.transform.childCount);
            Assert.AreEqual(0, scene.Streamer.ColliderTileCount);
        }

        [TestCase(false)]
        [TestCase(true)]
        public void ClearingOrDisposingInvalidatesASuspendedUpload(bool disposeStreamer)
        {
            using var scene = new ForestFixture(ForestData(), HeavyFirstTile());
            var routine = Flatten(scene.Streamer.UpdateLod0NeighborhoodIncremental(Vector2Int.zero, () => true));
            try
            {
                Assert.IsTrue(routine.MoveNext());
                if (disposeStreamer) scene.Streamer.Dispose(); else scene.Streamer.ClearPlayerFocus();
                Assert.IsFalse(routine.MoveNext(), "A stale iterator cannot install colliders after ownership is cleared.");
                Assert.AreEqual(0, scene.Streamer.ColliderTileCount);
                Assert.AreEqual(0, scene.Streamer.ActiveTrunkColliderCount);
                Assert.IsEmpty(scene.Host.GetComponentsInChildren<Collider>(true));
                if (disposeStreamer)
                {
                    Assert.IsNull(scene.Streamer.Root);
                    Assert.AreEqual(0, scene.Host.transform.childCount);
                }
                else
                {
                    Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(8, 8), () => true));
                    Assert.IsTrue(scene.Streamer.Lod0NeighborhoodMatches(new Vector2Int(8, 8)));
                    Assert.AreEqual(3, scene.Renderer.InstanceCount);
                }
            }
            finally { (routine as IDisposable)?.Dispose(); }
        }

        [Test]
        public void ANewTargetRetiresPendingWorkAndAnInterruptedMoveCanReturnToItsOldTile()
        {
            using var scene = new ForestFixture(ForestData(), HeavyFirstTile());
            var old = Flatten(scene.Streamer.UpdateLod0NeighborhoodIncremental(Vector2Int.zero, () => true));
            try
            {
                Assert.IsTrue(old.MoveNext());
                Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(8, 8), () => true));
                Assert.IsFalse(old.MoveNext());
                Assert.IsEmpty(scene.Streamer.Root.GetComponentsInChildren<Collider>(true));
                Assert.IsTrue(scene.Streamer.Lod0NeighborhoodMatches(new Vector2Int(8, 8)));
            }
            finally { (old as IDisposable)?.Dispose(); }
            var wanted = true;
            var partial = Flatten(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(9, 8), () => wanted));
            try
            {
                Assert.IsTrue(partial.MoveNext());
                Assert.AreEqual(8, scene.Streamer.ColliderTileCount);
                wanted = false;
                Assert.IsFalse(partial.MoveNext());
                Assert.IsFalse(scene.Streamer.Lod0NeighborhoodMatches(new Vector2Int(8, 8)));
            }
            finally { (partial as IDisposable)?.Dispose(); }
            Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(8, 8), () => true));
            Assert.IsTrue(scene.Streamer.Lod0NeighborhoodMatches(new Vector2Int(8, 8)));
        }

        [Test]
        public void VisibilityDisablesTheInstancedRendererAndItsResidentCollidersTogether()
        {
            using var scene = new ForestFixture(ForestData(), CollidersEverywhere());
            Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(new Vector2Int(24, 24), () => true));
            scene.Streamer.SetVisible(false);
            Assert.IsFalse(scene.Renderer.isActiveAndEnabled);
            Assert.AreEqual(9, scene.Streamer.ColliderTileCount);
            Assert.IsTrue(scene.Streamer.Root.GetComponentsInChildren<Collider>(true).All(collider => !collider.gameObject.activeInHierarchy));
            scene.Streamer.SetVisible(true);
            Assert.IsTrue(scene.Renderer.isActiveAndEnabled);
            Assert.AreEqual(9, scene.Streamer.Root.GetComponentsInChildren<CapsuleCollider>().Length);
            Assert.IsEmpty(scene.Streamer.Root.GetComponentsInChildren<MeshRenderer>(true));
        }

        [Test]
        public void InvalidColliderDataRollsBackTheWholePendingTile()
        {
            var trunks = new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][];
            trunks[0] = new[] { Trunk(), new IslandPreparedTreeCollider(Vector3.zero, Vector3.zero, .3f) };
            using var scene = new ForestFixture(ForestData(), trunks);
            Assert.Throws<InvalidOperationException>(() => Drain(scene.Streamer.UpdateLod0NeighborhoodIncremental(Vector2Int.zero, () => true)));
            Assert.AreEqual(0, scene.Streamer.ColliderTileCount);
            Assert.AreEqual(0, scene.Streamer.ActiveTrunkColliderCount);
            Assert.IsEmpty(scene.Streamer.Root.GetComponentsInChildren<Collider>(true));
        }

        private static readonly Vector3 TrunkBottom = new Vector3(2, .2f, 3);
        private static readonly Vector3 TrunkTop = new Vector3(2.25f, 4.2f, 3.5f);
        private static readonly Vector3 TrunkAxis = TrunkTop - TrunkBottom;
        private static readonly Vector3 LogBottom = new Vector3(1, .3f, 5);
        private static readonly Vector3 LogTop = new Vector3(4, .6f, 5.5f);
        private static readonly Vector3 LogAxis = LogTop - LogBottom;
        private static IslandPreparedTreeCollider Trunk() => new IslandPreparedTreeCollider(TrunkBottom, TrunkTop, .3f);

        private static IslandPreparedTreeCollider[][] CollidersEverywhere()
        {
            var result = new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][];
            var source = new[] { Trunk() };
            for (var i = 0; i < result.Length; ++i) result[i] = source;
            return result;
        }

        private static IslandPreparedTreeCollider[][] HeavyFirstTile()
        {
            var result = new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][];
            result[0] = Enumerable.Repeat(Trunk(), 64).ToArray();
            return result;
        }

        private static HybridForestData ForestData()
        {
            var triangle = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.up * 4, Vector3.right },
                new[] { Vector3.forward, Vector3.forward, Vector3.forward }, new[] { 0, 1, 2 },
                Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
            var prototypes = new IslandPreparedMesh[3][];
            var instances = new HybridForestNative.Instance[3];
            for (var i = 0; i < 3; ++i)
            {
                prototypes[i] = Enumerable.Repeat(triangle, 8).ToArray();
                instances[i] = new HybridForestNative.Instance { anchor = new Vector3(.5f + i * .01f, .5f, 0), scale = 1, prototype = (uint)i };
            }
            return new HybridForestData(prototypes, instances, 1, 1000);
        }

        private sealed class ForestFixture : IDisposable
        {
            internal readonly GameObject Host = new GameObject("Instanced forest streaming test") { layer = 27 };
            internal readonly ForestTileStreamer Streamer = new ForestTileStreamer();
            internal InstancedForestRenderer Renderer => Streamer.Root.GetComponent<InstancedForestRenderer>();
            private readonly Material fallenLogMaterial;

            internal ForestFixture(HybridForestData data, IslandPreparedTreeCollider[][] trunks = null,
                IslandPreparedMesh[] fallenLogTiles = null)
            {
                var logs = new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][];
                if (trunks != null)
                    for (var i = 0; i < trunks.Length; ++i)
                        if (trunks[i]?.Length == 1) logs[i] = new[] { new IslandPreparedTreeCollider(LogBottom, LogTop, .2f) };
                if (fallenLogTiles != null) fallenLogMaterial = new Material(Shader.Find("Motu/Tree Wood"));
                Streamer.Initialize(Host.transform, new IslandPreparedForestData(data,
                    trunks ?? new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][], logs, fallenLogTiles),
                    true, double.MaxValue, null, fallenLogMaterial);
            }

            public void Dispose()
            {
                Streamer.Dispose();
                Object.DestroyImmediate(Host);
                if (fallenLogMaterial != null) Object.DestroyImmediate(fallenLogMaterial);
            }
        }

        private static void Drain(IEnumerator routine)
        {
            var flattened = Flatten(routine);
            try { while (flattened.MoveNext()) { } }
            finally { (flattened as IDisposable)?.Dispose(); }
        }

        private static IEnumerator Flatten(IEnumerator routine)
        {
            try
            {
                while (routine.MoveNext())
                {
                    if (routine.Current is IEnumerator nested)
                    {
                        var child = Flatten(nested);
                        try { while (child.MoveNext()) yield return child.Current; }
                        finally { (child as IDisposable)?.Dispose(); }
                    }
                    else yield return routine.Current;
                }
            }
            finally { (routine as IDisposable)?.Dispose(); }
        }
    }
}
