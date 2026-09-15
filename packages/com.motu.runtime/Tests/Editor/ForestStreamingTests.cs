using System;
using System.Collections;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ForestStreamingTests
    {
        [Test]
        public void TileBoundsAssignEveryBoundaryToExactlyOneOwner()
        {
            for (var x = 0; x < 64; x++)
            {
                var area = ForestGridWorker.TileArea(x, x);
                var boundary = (x + 1) / 64f;
                Assert.AreEqual(x / 64f, area.min.x);
                Assert.AreEqual(x / 64f, area.min.y);
                Assert.AreEqual(area.max.x, area.max.y);
                if (x == 63) Assert.AreEqual(1f, area.max.x);
                else
                {
                    Assert.Less(area.max.x, boundary);
                    Assert.AreEqual(boundary, BitConverter.Int32BitsToSingle(
                        BitConverter.SingleToInt32Bits(area.max.x) + 1));
                    Assert.AreEqual(boundary, ForestGridWorker.TileArea(x + 1, x + 1).min.x);
                }
            }
        }

        [Test]
        public void PendingExportCanBeAbandonedWithoutInstallingOrKeepingDetails()
        {
            var root = new GameObject("Forest cancellation test");
            var material = new Material(Shader.Find("Motu/Tree Wood"));
            var streamer = new ForestTileStreamer();
            var completion = new TaskCompletionSource<ForestMeshRegion>();
            CancellationToken requested = default;
            IEnumerator routine = null;
            try
            {
                var prepared = new IslandPreparedForestData(new IslandPreparedMesh[64],
                    new IslandPreparedMesh[64], new IslandPreparedTreeCollider[4096][]);
                streamer.Initialize(root.transform, material, material, material, material, material, prepared, true,
                    (lod, key, token) => { requested = token; return completion.Task; });
                routine = Flatten(streamer.UpdateLod1NeighborhoodIncremental(Vector2Int.zero, () => true));
                Assert.IsTrue(routine.MoveNext());
                Assert.AreEqual(0, streamer.Lod1GroupCount);
                streamer.ClearPlayerFocus();
                Assert.IsTrue(requested.IsCancellationRequested);
                completion.SetResult(new ForestMeshRegion(64));
                while (routine.MoveNext()) { }
                Assert.AreEqual(0, streamer.Lod1GroupCount);
                Assert.IsEmpty(root.GetComponentsInChildren<MeshFilter>(true));
                streamer.Dispose();
                Assert.AreEqual(1, root.GetComponentsInChildren<Transform>(true).Length);
            }
            finally
            {
                (routine as IDisposable)?.Dispose();
                streamer.Dispose();
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void TeleportAndUploadCancellationKeepOnlyTheWantedGroups()
        {
            var root = new GameObject("Forest upload cancellation test");
            var material = new Material(Shader.Find("Motu/Tree Wood"));
            var streamer = new ForestTileStreamer();
            var mesh = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.right, Vector3.forward },
                new[] { Vector3.up, Vector3.up, Vector3.up }, new[] { 0, 2, 1 },
                Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
            var overview = new IslandPreparedMesh[64]; overview[0] = mesh;
            ForestMeshRegion exported = null;
            try
            {
                streamer.Initialize(root.transform, material, material, material, material, material,
                    new IslandPreparedForestData(overview, new IslandPreparedMesh[64], new IslandPreparedTreeCollider[4096][]),
                    true, (lod, key, cancellation) =>
                    {
                        exported = new ForestMeshRegion(lod == 1 ? 64 : 1);
                        for (var i = 0; i < exported.foliage.Length; i++) exported.foliage[i] = mesh;
                        return Task.FromResult(exported);
                    }, .1);
                using (var partial = (IDisposable)Flatten(streamer.UpdateLod1NeighborhoodIncremental(Vector2Int.zero, () => true)))
                {
                    Assert.IsTrue(((IEnumerator)partial).MoveNext());
                    Assert.AreEqual(0, streamer.Lod1GroupCount, "The upload must still be pending.");
                    var beforeEdges = root.GetComponentsInChildren<MeshFilter>(true).Length;
                    streamer.SetMeshEdgesVisible(true);
                    Assert.Greater(root.GetComponentsInChildren<MeshFilter>(true).Length, beforeEdges + 1,
                        "Wireframe toggles must also reach already uploaded pending tiles.");
                    streamer.ClearPlayerFocus();
                }
                Assert.AreEqual(0, streamer.Lod1GroupCount);
                // Only overview, its shared shadow and its wireframe survive a partial upload.
                Assert.AreEqual(3, root.GetComponentsInChildren<MeshFilter>(true).Length);
                for (var cycle = 0; cycle < 3; cycle++)
                {
                    Drain(streamer.UpdateLod1NeighborhoodIncremental(new Vector2Int(3, 3), () => true));
                    Assert.AreEqual(9, streamer.Lod1GroupCount);
                    Assert.That(exported.foliage, Is.All.Null);
                    Drain(streamer.UpdateLod1NeighborhoodIncremental(new Vector2Int(7, 7), () => true));
                    Assert.AreEqual(4, streamer.Lod1GroupCount);
                    streamer.ClearPlayerFocus();
                    Assert.AreEqual(0, streamer.Lod1GroupCount);
                    Assert.AreEqual(3, root.GetComponentsInChildren<MeshFilter>(true).Length);
                }
            }
            finally
            {
                streamer.Dispose();
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(material);
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
