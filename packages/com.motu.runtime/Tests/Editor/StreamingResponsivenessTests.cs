using System;
using System.Collections;
using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;
using System.Collections.Generic;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class StreamingResponsivenessTests
    {
        private static IEnumerator CheckForestExports(NativeIslandHandle owner)
        {
            // Select a populated region using only the cheap overview. Reference
            // exports stay region-sized too; regression tests must not recreate
            // the multi-gigabyte whole-island detail allocation being removed.
            var overviewTask = Task.Run(() => VegetationPreparation.PrepareForestMeshGrid(owner.Value, 10000f, 2, 8, true));
            while (!overviewTask.IsCompleted) yield return null;
            var overview = overviewTask.GetAwaiter().GetResult();
            var largest = 0;
            for (var index = 1; index < overview.Length; index++)
                if ((overview[index]?.vertices.Length ?? 0) > (overview[largest]?.vertices.Length ?? 0)) largest = index;
            Assert.IsNotNull(overview[largest], "Fixture must contain wood.");
            var samples = new HashSet<Vector2Int> { Vector2Int.zero, new Vector2Int(7, 7),
                new Vector2Int(largest % 8, largest / 8) };
            var comparedMeshes = 0;
            for (var lod = 0; lod <= 1; lod++)
                foreach (var key in samples)
                {
                    var level = lod;
                    var area = new MotuNative.ExportArea(key.x / 8f, key.y / 8f, (key.x + 1) / 8f, (key.y + 1) / 8f);
                    var baseline = Task.Run(() => new[]
                    {
                        VegetationPreparation.PrepareForestMeshGrid(owner.Value, 10000f, level, 8, false, area),
                        VegetationPreparation.PrepareForestMeshGrid(owner.Value, 10000f, level, 8, true, area)
                    });
                    while (!baseline.IsCompleted) yield return null;
                    var expected = baseline.GetAwaiter().GetResult();
                    ForestMeshRegion actual = null;
                    if (lod == 1)
                    {
                        var export = ForestGridWorker.PrepareAsync(owner, 10000f, lod, key, CancellationToken.None);
                        while (!export.IsCompleted) yield return null;
                        actual = export.GetAwaiter().GetResult();
                    }
                    for (var index = 0; index < 64; index++)
                    {
                        // The legacy area's maximum edge is inclusive; compare
                        // interior cells where its ownership matches the full grid.
                        if ((index % 8 == 7 && key.x != 7) || (index / 8 == 7 && key.y != 7)) continue;
                        if (lod == 0)
                        {
                            var tile = key * 8 + new Vector2Int(index % 8, index / 8);
                            var export = ForestGridWorker.PrepareAsync(owner, 10000f, lod, tile, CancellationToken.None);
                            while (!export.IsCompleted) yield return null;
                            actual = export.GetAwaiter().GetResult();
                        }
                        var channels = new[] { actual.foliage, actual.wood };
                        for (var channel = 0; channel < channels.Length; channel++)
                        {
                            var before = expected[channel][index];
                            var after = channels[channel][lod == 1 ? index : 0];
                            if (before == null) { Assert.IsNull(after); continue; }
                            Assert.IsNotNull(after);
                            comparedMeshes++;
                            CollectionAssert.AreEqual(before.vertices, after.vertices);
                            CollectionAssert.AreEqual(before.triangles, after.triangles);
                            CollectionAssert.AreEqual(before.normals, after.normals);
                            CollectionAssert.AreEqual(before.uv, after.uv);
                            CollectionAssert.AreEqual(before.material, after.material);
                            CollectionAssert.AreEqual(before.environment, after.environment);
                        }
                    }
                }
            Assert.Greater(comparedMeshes, 0);
            UnityEngine.Debug.Log($"MOTU FOREST EXPORT EQUIVALENCE: {comparedMeshes} nonempty meshes compared exactly.");
            using var cancelled = new CancellationTokenSource();
            cancelled.Cancel();
            var abandoned = ForestGridWorker.PrepareAsync(owner, 10000f, 0, Vector2Int.zero, cancelled.Token);
            while (!abandoned.IsCompleted) yield return null;
            Assert.IsTrue(abandoned.IsCanceled);
        }

        [UnityTest, Timeout(600000)]
        public IEnumerator TravelUsesBackgroundExportsAndUnloadRetainsWorkerLease()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var baselineHandles = NativeIslandHandle.ActiveCount;
            var host = new GameObject("Streaming responsiveness island");
            var island = host.AddComponent<SingleIsland>();
            island.GenerateOnStart = false;
            island.GenerationSettings.Seed = 17;
            island.GenerationSettings.MaximumHeightMetres = 400f;
            island.GenerationSettings.WaterRatio = .85f;
            island.GenerationSettings.UseSnapshotCache = false;
            island.NavigationSettings.Enabled = false;
            JsonUtility.FromJsonOverwrite("{\"forestPrototypeCount\":1}", island.ForestSettings);
            JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", island.RenderingSettings);
            NativeIslandHandle.Lease heldLease = null;
            try
            {
                var generate = island.GenerateAsync(CancellationToken.None);
                while (!generate.IsCompleted) yield return null;
                Assert.IsTrue(generate.GetAwaiter().GetResult(), island.Generator.Status);
                var runtime = island.Generator.Runtime;
                var owner = runtime.NativeHandle;
                var streamer = runtime.TerrainStreamer;
                yield return CheckForestExports(owner);
                // This is the synchronous preparation previously called by the
                // movement handler, without its additional Unity upload cost.
                var area = new MotuNative.ExportArea(.5f, .5f, .625f, .625f);
                var timer = Stopwatch.StartNew();
                var expected = TerrainGridWorker.Prepare(owner.Value, area, 1, 8, 15, 10000f, CancellationToken.None);
                var blockedMilliseconds = timer.Elapsed.TotalMilliseconds;
                var task = TerrainGridWorker.PrepareAsync(owner, area, 1, 8, 15, 10000f, CancellationToken.None);
                var frames = 0;
                while (!task.IsCompleted) { frames++; yield return null; }
                var actual = task.GetAwaiter().GetResult();
                Assert.Greater(frames, 0, "Export must permit frame advancement.");
                Assert.AreEqual(expected.Length, actual.Length);
                for (var index = 0; index < expected.Length; index++)
                {
                    if (expected[index] == null) { Assert.IsNull(actual[index]); continue; }
                    CollectionAssert.AreEqual(expected[index].vertices, actual[index].vertices);
                    CollectionAssert.AreEqual(expected[index].triangles, actual[index].triangles);
                    CollectionAssert.AreEqual(expected[index].normals, actual[index].normals);
                    CollectionAssert.AreEqual(expected[index].uv, actual[index].uv);
                    CollectionAssert.AreEqual(expected[index].material, actual[index].material);
                    CollectionAssert.AreEqual(expected[index].environment, actual[index].environment);
                }
                expected = null; actual = null;
                timer.Restart();
                streamer.SetPlayerPosition(Vector3.zero);
                var focusMilliseconds = timer.Elapsed.TotalMilliseconds;
                Assert.IsEmpty(streamer.lod1Groups, "Initial approach must not synchronously build terrain groups.");
                Assert.IsEmpty(streamer.lod0Groups);
                Assert.IsNotEmpty(streamer.colliderTiles, "Critical collision must be present immediately.");
                var transitionFrames = 0;
                var maxFrameMilliseconds = 0.0;
                var deadline = Time.realtimeSinceStartup + 180f;
                while (streamer.HasPendingTransition)
                {
                    Assert.Less(Time.realtimeSinceStartup, deadline, "Streaming failed to finish.");
                    timer.Restart();
                    yield return null;
                    maxFrameMilliseconds = Math.Max(maxFrameMilliseconds, timer.Elapsed.TotalMilliseconds);
                    transitionFrames++;
                }
                Assert.AreEqual(9, streamer.lod1Groups.Count);
                Assert.AreEqual(9, streamer.lod0Groups.Count);
                // Interrupt another transition, then unload while a lease is held.
                streamer.SetPlayerPosition(new Vector3(2000f, 0f, 2000f));
                yield return null;
                streamer.SetPlayerPosition(new Vector3(-2000f, 0f, -2000f));
                heldLease = owner.Acquire();
                island.Clear();
                Assert.IsFalse(owner.IsValid);
                Assert.Throws<ObjectDisposedException>(() => owner.Acquire());
                Assert.AreEqual(baselineHandles + 1, NativeIslandHandle.ActiveCount);
                heldLease.Dispose(); heldLease.Dispose(); heldLease = null;
                deadline = Time.realtimeSinceStartup + 30f;
                while (NativeIslandHandle.ActiveCount != baselineHandles && Time.realtimeSinceStartup < deadline)
                    yield return null;
                Assert.AreEqual(baselineHandles, NativeIslandHandle.ActiveCount);
                UnityEngine.Debug.Log($"MOTU STREAMING RESPONSIVENESS: syncGridMs={blockedMilliseconds:F3}, exportFrames={frames}, initialFocusMs={focusMilliseconds:F3}, transitionFrames={transitionFrames}, maxObservedFrameMs={maxFrameMilliseconds:F3}");
            }
            finally
            {
                heldLease?.Dispose();
                island.Clear();
                Object.Destroy(host);
            }
            yield return new ExitPlayMode();
        }
    }
}
