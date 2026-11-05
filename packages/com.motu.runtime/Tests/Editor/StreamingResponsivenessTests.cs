using System;
using System.Collections;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
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
        private static int NeighborhoodColliderCount(IslandPreparedTreeCollider[][] tiles, Vector2Int center)
        {
            var count = 0;
            for (var y = Mathf.Max(0, center.y - 1); y <= Mathf.Min(63, center.y + 1); y++)
                for (var x = Mathf.Max(0, center.x - 1); x <= Mathf.Min(63, center.x + 1); x++)
                    count += tiles[y * 64 + x]?.Length ?? 0;
            return count;
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
            JsonUtility.FromJsonOverwrite("{\"experimentalInstancedTrees\":false,\"forestPrototypeCount\":1}", island.ForestSettings);
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
                var forests = host.GetComponentsInChildren<InstancedForestRenderer>(true);
                Assert.AreEqual(1, forests.Length, "Every generated island must use the shared forest renderer.");
                var forest = forests[0];
                Assert.AreEqual(12, forest.PrototypeCount, "Four shared variants for each of broadleaf, pine and palm.");
                Assert.Greater(forest.InstanceCount, 0, "Fixture must contain compact tree placements.");
                Assert.IsTrue(forest.GetComponentsInChildren<MeshRenderer>(true)
                    .All(renderer => renderer.name.StartsWith("Fallen logs tile ", StringComparison.Ordinal)),
                    "Standing-tree visuals must not allocate tile or per-tree renderers; only authored fallen-log tiles use meshes.");
                var compactExport = Task.Run(() =>
                {
                    using var lease = owner.Acquire();
                    return (catalog: HybridForestNative.Prepare(owner.Value, 10000f),
                        trunks: VegetationPreparation.PrepareForestTrunkColliders(owner.Value, 10000f),
                        logs: VegetationPreparation.PrepareForestTrunkColliders(owner.Value, 10000f, fallenLogs: true));
                });
                while (!compactExport.IsCompleted) yield return null;
                var compact = compactExport.GetAwaiter().GetResult();
                Assert.IsNotNull(compact.catalog);
                Assert.AreEqual(forest.InstanceCount, compact.catalog.instances.Length);
                Assert.AreEqual(4, compact.catalog.broadleafCount);
                Assert.AreEqual(12, compact.catalog.prototypes.Length);
                Assert.AreEqual(4096, compact.trunks.Length);
                Assert.AreEqual(4096, compact.logs.Length);
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
                Assert.AreEqual(9, streamer.ForestColliderTileCount,
                    "Forest collision ownership must remain bounded to the active 3x3 LOD0 owner grid.");
                var expectedTrunks = NeighborhoodColliderCount(compact.trunks, streamer.requestedLod1);
                var expectedLogs = NeighborhoodColliderCount(compact.logs, streamer.requestedLod1);
                Assert.AreEqual(expectedTrunks, streamer.ForestTrunkColliderCount);
                Assert.AreEqual(expectedLogs, streamer.ForestLogColliderCount);
                Assert.AreEqual(expectedTrunks, forest.GetComponentsInChildren<CapsuleCollider>().Length,
                    "Only nearby trunk capsules may be installed.");
                Assert.AreEqual(expectedLogs, forest.GetComponentsInChildren<BoxCollider>().Length,
                    "Only nearby fallen-log boxes may be installed.");
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
