using System;
using System.Collections;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.World;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class IslandWorldSchedulingTests
    {
        private const BindingFlags Hidden = BindingFlags.Instance | BindingFlags.NonPublic;

        [Test]
        public void DiscoveryRanksAllCandidatesBeforeStartingTheFirstWorker()
        {
            var gate = (SemaphoreSlim)typeof(IslandGenerationWorker)
                .GetField("PreparationGate", BindingFlags.Static | BindingFlags.NonPublic).GetValue(null);
            Assert.That(gate.Wait(5000), Is.True);
            try
            {
                using var fixture = new Fixture();
                fixture.Add(Vector2Int.right * 3); // Deliberately discover a distant miss first.
                var nearby = fixture.Add(Vector2Int.zero);
                Set(fixture.World, "shutdown", new CancellationTokenSource());
                Invoke(fixture.World, "Update");
                Assert.That(Get(fixture.World, "currentGeneration"), Is.SameAs(nearby),
                    "An approaching island must beat an earlier distant silhouette cache miss.");
                Assert.That(fixture.World.GeneratingIslandCount, Is.EqualTo(1));
            }
            finally { gate.Release(); }
        }

        [Test]
        public void FastTravelStartsDetailsEarlierWithoutExpandingTheResidentLimit()
        {
            using var fixture = new Fixture();
            var entry = fixture.Add(Vector2Int.zero);
            var viewer = Vector3.left * 3000;
            Set(fixture.World, "lastQueryPosition", viewer);
            Set(fixture.World, "projectedGenerationPosition", viewer);
            Assert.That(Invoke(fixture.World, "WantsDetailedIsland", entry), Is.False);
            Set(fixture.World, "smoothedVelocity", Vector3.right * 40);
            Assert.That(Invoke(fixture.World, "WantsDetailedIsland", entry), Is.True,
                "Fast approaches need preparation time in addition to the fixed visual margin.");
            Assert.That(fixture.World.ResidentIslandLimit, Is.EqualTo(3));
            Set(fixture.World, "lastQueryPosition", Vector3.left * 10000);
            Set(fixture.World, "projectedGenerationPosition", Vector3.left * 10000);
            Set(fixture.World, "smoothedVelocity", Vector3.right * 10000);
            Assert.That(Invoke(fixture.World, "WantsDetailedIsland", entry), Is.False,
                "Teleport or fly-mode velocity must not create an unbounded preparation radius.");
        }

        [Test]
        public void ColdNearbyIslandStartsDetailsWithoutASeparateHorizonWorker()
        {
            var gate = (SemaphoreSlim)typeof(IslandGenerationWorker)
                .GetField("PreparationGate", BindingFlags.Static | BindingFlags.NonPublic).GetValue(null);
            Assert.That(gate.Wait(5000), Is.True);
            try
            {
                using var fixture = new Fixture();
                var nearby = fixture.Add(Vector2Int.zero);
                Set(fixture.World, "shutdown", new CancellationTokenSource());
                Invoke(fixture.World, "Update");
                var generator = nearby.GetType().GetProperty("Generator", Hidden).GetValue(nearby) as IslandGenerator;
                Assert.That(generator, Is.Not.Null, "An uncached approaching island must go directly into detail preparation.");
                Assert.That(generator.IsGenerating, Is.True);
                Assert.That(fixture.World.GeneratingIslandCount, Is.EqualTo(1));
            }
            finally { gate.Release(); }
        }

        [Test]
        public void UrgentDetailsPreemptBackgroundWorkButNotAnotherImmediateIsland()
        {
            using var fixture = new Fixture();
            var nearby = fixture.Add(Vector2Int.zero);
            var distant = fixture.Add(Vector2Int.right * 3);
            Assert.That(Invoke(fixture.World, "ShouldPreemptGeneration", distant, nearby), Is.True);
            Assert.That(Invoke(fixture.World, "ShouldPreemptGeneration", nearby, distant), Is.False);
            Assert.That(Invoke(fixture.World, "ShouldPreemptGeneration", nearby, nearby), Is.False);
        }

        [Test]
        public void ANewNearbyRequestCancelsTheDistantWorkerOnlyOnce()
        {
            using var fixture = new Fixture();
            var nearby = fixture.Add(Vector2Int.zero);
            var distant = fixture.Add(Vector2Int.right * 3);
            var cancellation = new CancellationTokenSource();
            var type = distant.GetType();
            type.GetProperty("GenerationCancellation", Hidden).SetValue(distant, cancellation);
            type.GetProperty("Generating", Hidden).SetValue(distant, true);
            nearby.GetType().GetProperty("Queued", Hidden).SetValue(nearby, true);
            Set(fixture.World, "currentGeneration", distant);
            Invoke(fixture.World, "CancelObsoleteGeneration");
            Assert.That(cancellation.IsCancellationRequested, Is.True);
            int token = (int)type.GetProperty("GenerationToken", Hidden).GetValue(distant);
            Invoke(fixture.World, "CancelObsoleteGeneration");
            Assert.That(type.GetProperty("GenerationToken", Hidden).GetValue(distant), Is.EqualTo(token));
            type.GetProperty("GenerationCancellation", Hidden).SetValue(distant, null);
            cancellation.Dispose();
        }

        [Test]
        public void ObservedSlowPreparationExtendsTravelLeadWithinTheSameBounds()
        {
            using var fixture = new Fixture();
            var entry = fixture.Add(Vector2Int.zero);
            Set(fixture.World, "lastQueryPosition", Vector3.left * 4000);
            Set(fixture.World, "projectedGenerationPosition", Vector3.left * 4000);
            Set(fixture.World, "smoothedVelocity", Vector3.right * 10);
            Assert.That(Invoke(fixture.World, "WantsDetailedIsland", entry), Is.False);
            Set(fixture.World, "observedDetailPreparationSeconds", 240f);
            Assert.That(Invoke(fixture.World, "WantsDetailedIsland", entry), Is.True);
            Assert.That(fixture.World.ResidentIslandLimit, Is.EqualTo(3));
            Set(fixture.World, "lastQueryPosition", Vector3.left * 12000);
            Set(fixture.World, "projectedGenerationPosition", Vector3.left * 12000);
            Assert.That(Invoke(fixture.World, "WantsDetailedIsland", entry), Is.False);
        }

        [UnityTest, Timeout(600000), Category("NativeIntegration")]
        public IEnumerator ColdAdjacentIslandHasTerrainAndCollisionBeforeEnteringItsCell()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var handles = NativeIslandHandle.ActiveCount;
            using (var fixture = new Fixture())
            {
                fixture.MoveViewer(Vector3.left * 2500);
                fixture.World.Lod3DistanceMetres = 1500;
                Set(fixture.World, "nextDiscoveryTime", float.MaxValue);
                var entry = fixture.Add(Vector2Int.zero);
                fixture.World.enabled = false;
                fixture.Root.SetActive(true);
                Set(fixture.World, "smoothedVelocity", Vector3.right * 10);
                var timer = Stopwatch.StartNew();
                Invoke(fixture.World, "QueueDesiredGeneration");
                Invoke(fixture.World, "ProcessGenerationQueue");
                var generator = (IslandGenerator)entry.GetType().GetProperty("Generator", Hidden).GetValue(entry);
                Assert.That(generator, Is.Not.Null, "A steady approach must start detail preparation directly.");
                while (true)
                {
                    var position = Vector3.right * (-2500 + (float)timer.Elapsed.TotalSeconds * 10);
                    fixture.MoveViewer(position);
                    Invoke(fixture.World, "Update");
                    bool ready = fixture.World.HasActiveTerrainColliderAt(Vector3.left * 999)
                        && !generator.IsGenerating && generator.HasRuntime
                        && !generator.Runtime.IsLod3Visible
                        && !generator.Runtime.TerrainStreamer.HasPendingTransition
                        && generator.Runtime.TerrainStreamer.lod0Groups.Count > 0;
                    Assert.That(position.x, Is.LessThan(-1000),
                        "At 10 m/s the detailed island and collision must finish before the ship enters its cell.");
                    if (ready) break;
                    yield return null;
                }
                Assert.That(fixture.World.LoadedIslandCount, Is.EqualTo(1));
                Assert.That(fixture.World.HorizonIslandCount, Is.EqualTo(1), "Direct detail generation must also retain an independent fallback.");
                Assert.That(IslandWorldManager.WorldToCell(new Vector3(fixture.World.LogicalPlayerPosition.x, 0,
                    fixture.World.LogicalPlayerPosition.y)), Is.Not.EqualTo(Vector2Int.zero));
                Assert.That(fixture.World.HasActiveTerrainColliderAt(Vector3.left * 999), Is.True,
                    "Collision must be ready at the approaching cell edge before the ship crosses it.");
                fixture.MoveViewer(Vector3.left * 900);
                Invoke(fixture.World, "Update");
                Assert.That(fixture.World.FocusedIsland, Is.Not.Null);
                fixture.MoveViewer(Vector3.left * 1001);
                Invoke(fixture.World, "Update");
                Assert.That(fixture.World.HasActiveTerrainColliderAt(Vector3.left * 999), Is.True,
                    "Changing grid-cell focus must retain an adjacent island's collision while a ship straddles its edge.");
                Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(handles + 1));
                UnityEngine.Debug.Log($"MOTU COLD APPROACH: adjacent terrain/collision ready in {timer.Elapsed.TotalSeconds:F2}s, one native preparation and one resident island.");
            }
            var drainDeadline = Time.realtimeSinceStartup + 30;
            while (NativeIslandHandle.ActiveCount != handles)
            {
                Assert.That(Time.realtimeSinceStartup, Is.LessThan(drainDeadline));
                yield return null;
            }
            yield return new ExitPlayMode();
        }

        [UnityTest, Timeout(600000), Category("NativeIntegration")]
        public IEnumerator NearbySilhouetteUpgradesToTerrainAndCollisionWithoutAnotherWorldUpdate()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var handles = NativeIslandHandle.ActiveCount;
            using (var fixture = new Fixture())
            {
                fixture.CacheHorizon(fixture.Add(Vector2Int.right * 3));
                fixture.CacheHorizon(fixture.Add(Vector2Int.zero));
                fixture.World.enabled = false;
                fixture.Root.SetActive(true);
                var timer = Stopwatch.StartNew();
                // Exactly one discovery update. The queue itself must promote
                // the nearby silhouette before taking more distant work.
                Invoke(fixture.World, "Update");
                var deadline = Time.realtimeSinceStartup + 450;
                while ((bool)Get(fixture.World, "queueRunning"))
                {
                    Assert.That(Time.realtimeSinceStartup, Is.LessThan(deadline));
                    yield return null;
                }
                Assert.That(fixture.World.LoadedIslandCount, Is.EqualTo(1));
                Assert.That(fixture.World.HorizonIslandCount, Is.EqualTo(2));
                Assert.That(fixture.World.IsTerrainCollisionReadyAt(Vector3.zero), Is.True);
                Assert.That(fixture.World.HasActiveTerrainColliderAt(Vector3.zero), Is.True);
                Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(handles + 1));
                UnityEngine.Debug.Log($"MOTU APPROACH LOAD: terrain and collision ready in {timer.Elapsed.TotalSeconds:F2}s; resident islands=1, distant silhouettes=1.");
            }
            var drainDeadline = Time.realtimeSinceStartup + 30;
            while (NativeIslandHandle.ActiveCount != handles)
            {
                Assert.That(Time.realtimeSinceStartup, Is.LessThan(drainDeadline));
                yield return null;
            }
            yield return new ExitPlayMode();
        }

        [Test]
        public void CurrentIslandKeepsPriorityAndItsResidentSlotAheadOfPredictedDestinations()
        {
            using var fixture = new Fixture();
            var nearby = fixture.Add(Vector2Int.zero);
            var ahead = fixture.Add(Vector2Int.right * 3);
            Set(fixture.World, "lastQueryPosition", Vector3.right * 800);
            Set(fixture.World, "projectedGenerationPosition", Vector3.right * 6000);
            Set(fixture.World, "maximumLoadedIslandCount", 1);
            Assert.That((int)Invoke(fixture.World, "CompareGenerationPriority", nearby, ahead), Is.LessThan(0));
            Assert.That(Invoke(fixture.World, "IsPreferredResident", nearby), Is.True);
            Assert.That(Invoke(fixture.World, "IsPreferredResident", ahead), Is.False);
        }

        private static object Get(object target, string name) => target.GetType().GetField(name, Hidden).GetValue(target);
        private static void Set(object target, string name, object value) => target.GetType().GetField(name, Hidden).SetValue(target, value);
        private static object Invoke(object target, string name, params object[] arguments) =>
            target.GetType().GetMethod(name, Hidden).Invoke(target, arguments);

        private sealed class Fixture : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Approaching island scheduling");
            internal readonly IslandWorldManager World;
            private readonly GameObject target = new GameObject("Approaching ship");
            private readonly GridIslandGenerationRequestFactory factory;
            private readonly string cache = Path.Combine(Path.GetTempPath(), "motu-approach-" + Guid.NewGuid().ToString("N"));

            internal Fixture()
            {
                Root.SetActive(false);
                factory = Root.AddComponent<GridIslandGenerationRequestFactory>();
                factory.Configure(17, false, 0);
                factory.SetFixedIslands(new GridIslandGenerationRequestFactory.FixedIsland(Vector2Int.zero),
                    new GridIslandGenerationRequestFactory.FixedIsland(Vector2Int.right * 3));
                factory.GenerationSettings.UseSnapshotCache = true;
                factory.GenerationSettings.SnapshotCacheDirectory = cache;
                JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", factory.RenderingSettings);
                World = Root.AddComponent<IslandWorldManager>();
                World.ConfigureIslandGenerationRequestFactory(factory);
                Set(World, "streamingTarget", target.transform);
                Set(World, "hasQueryPosition", true);
                Set(World, "nextDiscoveryTime", float.MaxValue);
                if (!Application.isPlaying)
                {
                    var environment = new GameObject("Scheduling environment stub").AddComponent<WorldEnvironmentController>();
                    Set(World, "worldEnvironment", environment);
                }
            }

            internal void MoveViewer(Vector3 point) { target.transform.position = point; Set(World, "lastQueryPosition", point); Set(World, "projectedGenerationPosition", point); }

            internal object Add(Vector2Int cell)
            {
                var type = typeof(IslandWorldManager).GetNestedType("IslandRuntimeEntry", BindingFlags.NonPublic);
                var entry = Activator.CreateInstance(type, Hidden, null,
                    new object[] { factory.CreateIslandGenerationRequest(cell) }, null);
                ((IDictionary)Get(World, "managedIslands")).Add(cell, entry);
                return entry;
            }

            internal void CacheHorizon(object entry)
            {
                var request = (IslandGenerationRequest)entry.GetType().GetProperty("GenerationRequest", Hidden).GetValue(entry);
                var mesh = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.right, Vector3.forward },
                    Array.Empty<Vector3>(), new[] { 0, 2, 1 }, Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
                Assert.That(IslandHorizonCache.TrySave(request.SnapshotPath, mesh, 1024 * 1024), Is.True);
            }

            public void Dispose()
            {
                var environment = World.EnvironmentController;
                Invoke(World, "OnDestroy");
                Object.DestroyImmediate(Root); Object.DestroyImmediate(target);
                if (environment != null) Object.DestroyImmediate(environment.gameObject);
                if (Directory.Exists(cache)) Directory.Delete(cache, true);
            }
        }
    }
}
