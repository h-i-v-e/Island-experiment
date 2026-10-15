using System;
using System.Collections;
using System.IO;
using System.Linq;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.World;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class IslandDiskStreamingTests
    {
        private string directory;
        private string snapshot;

        [SetUp]
        public void SetUp()
        {
            directory = Path.Combine(Path.GetTempPath(), "motu-horizon-test-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(directory);
            snapshot = Path.Combine(directory, new string('a', 64) + ".motusnapshot");
        }

        [TearDown]
        public void TearDown()
        {
            if (Directory.Exists(directory)) Directory.Delete(directory, true);
        }

        private static IslandPreparedMesh Triangle() => new IslandPreparedMesh(
            new[] { Vector3.zero, Vector3.right, Vector3.up }, Array.Empty<Vector3>(),
            new[] { 0, 1, 2 }, Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());

        [Test]
        public void Lod3DistanceIsPerWorldAndCanBeSetBeforeInitialization()
        {
            var first = new GameObject("Lower capacity world");
            var second = new GameObject("Default capacity world");
            first.SetActive(false);
            second.SetActive(false);
            try
            {
                var lower = first.AddComponent<IslandWorldManager>();
                var normal = second.AddComponent<IslandWorldManager>();
                Assert.That(lower.Lod3DistanceMetres, Is.EqualTo(1000f));
                lower.Lod3DistanceMetres = 500f;
                Assert.That(lower.Lod3DistanceMetres, Is.EqualTo(500f));
                Assert.That(normal.Lod3DistanceMetres, Is.EqualTo(1000f));
                foreach (var invalid in new[] { 0f, -1f, float.NaN, float.PositiveInfinity })
                    Assert.Throws<ArgumentOutOfRangeException>(() => lower.Lod3DistanceMetres = invalid);
                Assert.That(lower.Lod3DistanceMetres, Is.EqualTo(500f));
                // Exercise the serialized Inspector field as well as the script-facing property.
                JsonUtility.FromJsonOverwrite("{\"lod3DistanceMetres\":5000}", normal);
                Assert.That(normal.Lod3DistanceMetres, Is.EqualTo(5000f));
            }
            finally { Object.DestroyImmediate(first); Object.DestroyImmediate(second); }
        }

        [TestCase(500f)]
        [TestCase(5000f)]
        public void CachedAndResidentSilhouettesShareTheConfiguredSubmersionRange(float switchDistance)
        {
            var root = new GameObject("Configured LOD3 submersion");
            root.transform.position = new Vector3(14000f, 32f, -6000f);
            try
            {
                var runtime = IslandRuntime.Create(IslandDescriptor.Request(17, Vector2Int.zero, null), root.transform);
                runtime.Lod3DistanceMetres = switchDistance;
                runtime.InstallLod3(Triangle(), 80f);
                var proxy = new GameObject("Disk-only silhouette");
                proxy.transform.SetParent(root.transform, false);
                var horizon = proxy.AddComponent<IslandHorizonRenderer>();
                horizon.Initialize(Triangle(), 80f);
                var residentRenderer = runtime.GetComponentInChildren<MeshRenderer>(true);
                var cachedRenderer = horizon.GetComponentInChildren<MeshRenderer>();
                foreach (var offset in new[] { 0f, 1500f, 3000f })
                {
                    var viewer = root.transform.position + Vector3.right * (switchDistance + offset);
                    runtime.SetViewPosition(viewer);
                    horizon.SetViewPosition(viewer, true, switchDistance);
                    Assert.That(residentRenderer.transform.localPosition.y, Is.EqualTo(-80f * offset / 3000f).Within(.001f));
                    Assert.That(cachedRenderer.transform.localPosition.y, Is.EqualTo(residentRenderer.transform.localPosition.y).Within(.001f));
                }
                runtime.Dispose();
            }
            finally { Object.DestroyImmediate(root); }
        }

        [Test]
        public void HorizonCacheRoundTripsAndRejectsCorruption()
        {
            var source = Triangle();
            Assert.IsTrue(IslandHorizonCache.TrySave(snapshot, source, 1024 * 1024));
            Assert.IsTrue(IslandHorizonCache.TryLoad(snapshot, out var loaded));
            CollectionAssert.AreEqual(source.vertices, loaded.vertices);
            CollectionAssert.AreEqual(source.triangles, loaded.triangles);
            Assert.IsEmpty(loaded.normals);
            var path = IslandHorizonCache.PathFor(snapshot);
            var bytes = File.ReadAllBytes(path);
            bytes[20] ^= 1;
            File.WriteAllBytes(path, bytes);
            Assert.IsFalse(IslandHorizonCache.TryLoad(snapshot, out _));
            File.WriteAllBytes(path, new byte[3]);
            Assert.IsFalse(IslandHorizonCache.TryLoad(snapshot, out _));
            Assert.IsTrue(IslandHorizonCache.TrySave(snapshot, source, 1024 * 1024));
            Assert.IsTrue(IslandHorizonCache.TryLoad(snapshot, out _));
        }

        [Test]
        public void SubmergedIslandCachesAnEmptySilhouetteAndBudgetProtectsItsSnapshot()
        {
            File.WriteAllBytes(snapshot, new byte[1024]);
            var oldPath = Path.Combine(directory, new string('b', 64) + IslandHorizonCache.Extension);
            File.WriteAllBytes(oldPath, new byte[1024]);
            Assert.IsTrue(IslandHorizonCache.TrySave(snapshot, null, 1));
            Assert.IsTrue(File.Exists(snapshot), "Writing the sidecar must not evict its own full snapshot.");
            Assert.IsFalse(File.Exists(oldPath));
            Assert.IsTrue(IslandHorizonCache.TryLoad(snapshot, out var loaded));
            Assert.IsNull(loaded);
        }

        [UnityTest]
        public IEnumerator WarmHorizonNeverOpensTheFullSnapshot()
        {
            var request = RequestIn(directory);
            // A broken full snapshot would force native generation if this path read it.
            File.WriteAllBytes(request.SnapshotPath, new byte[] { 1, 2, 3 });
            Assert.IsTrue(IslandHorizonCache.TrySave(request.SnapshotPath, Triangle(), 1024 * 1024));
            var handles = NativeIslandHandle.ActiveCount;
            var pending = IslandGenerationWorker.GenerateHorizonAsync(request, CancellationToken.None);
            while (!pending.IsCompleted) yield return null;
            CollectionAssert.AreEqual(Triangle().vertices, pending.GetAwaiter().GetResult().vertices);
            Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(handles));
            CollectionAssert.AreEqual(new byte[] { 1, 2, 3 }, File.ReadAllBytes(request.SnapshotPath));
            using var cancellation = new CancellationTokenSource();
            cancellation.Cancel();
            var cancelled = IslandGenerationWorker.GenerateHorizonAsync(request, cancellation.Token);
            while (!cancelled.IsCompleted) yield return null;
            Assert.IsTrue(cancelled.IsCanceled);
        }

        private static IslandGenerationRequest RequestIn(string path)
        {
            var generation = new Motu.Settings.IslandGenerationSettings
            { Seed = 17, SnapshotCacheDirectory = path, UseSnapshotCache = true };
            return new IslandGenerationRequest(17, Vector2Int.zero, generation,
                new Motu.Settings.IslandRiverSettings(), new Motu.Settings.IslandForestSettings(),
                new Motu.Settings.IslandReedSettings(), new Motu.Settings.IslandFernSettings(),
                new Motu.Settings.IslandRenderingSettings(), default);
        }

        [UnityTest, Timeout(30000)]
        public IEnumerator LargerVisualDistanceExpandsDiscoveryAndGenerationCoverage()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var cache = Path.Combine(Path.GetTempPath(), "motu-large-distance-test-" + Guid.NewGuid().ToString("N"));
            var root = new GameObject("Large visual distance world");
            root.SetActive(false);
            var target = new GameObject("Far viewer");
            target.transform.position = Vector3.right * 21000f;
            var factory = root.AddComponent<GridIslandGenerationRequestFactory>();
            factory.Configure(17, false, 0);
            factory.SetFixedIslands(new GridIslandGenerationRequestFactory.FixedIsland(Vector2Int.zero));
            factory.GenerationSettings.SnapshotCacheDirectory = cache;
            factory.GenerationSettings.UseSnapshotCache = true;
            var request = factory.CreateIslandGenerationRequest(Vector2Int.zero);
            Assert.IsTrue(IslandHorizonCache.TrySave(request.SnapshotPath, Triangle(), 1024 * 1024));
            var manager = root.AddComponent<IslandWorldManager>();
            manager.Lod3DistanceMetres = 20000f;
            manager.ConfigureIslandGenerationRequestFactory(factory);
            manager.SetStreamingTarget(target.transform);
            var handles = NativeIslandHandle.ActiveCount;
            root.SetActive(true);
            var environment = manager.EnvironmentController;
            try
            {
                var deadline = Time.realtimeSinceStartup + 10f;
                while (manager.HorizonIslandCount != 1 || manager.GeneratingIslandCount != 0)
                {
                    Assert.That(Time.realtimeSinceStartup, Is.LessThan(deadline), "The configured distance must expand discovery and generation coverage.");
                    yield return null;
                }
                Assert.That(manager.LoadedIslandCount, Is.Zero);
                Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(handles));
                manager.Lod3DistanceMetres = 1000f;
                Assert.That(manager.HorizonIslandCount, Is.Zero, "Reducing the setting must release silhouettes outside the new retention range.");
            }
            finally
            {
                Object.Destroy(root); Object.Destroy(target);
                if (environment != null) Object.Destroy(environment.gameObject);
            }
            yield return null;
            if (Directory.Exists(cache)) Directory.Delete(cache, true);
            yield return new ExitPlayMode();
        }

        [UnityTest, Timeout(30000)]
        public IEnumerator PredictedHorizonIsRetainedWithoutMutatingTheFactory()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var cache = Path.Combine(Path.GetTempPath(), "motu-prediction-test-" + Guid.NewGuid().ToString("N"));
            var root = new GameObject("Predicted horizon world");
            root.SetActive(false);
            var target = new GameObject("Predicted viewer");
            target.transform.position = Vector3.right * 15000;
            var factory = root.AddComponent<GridIslandGenerationRequestFactory>();
            factory.Configure(17, false, 0);
            factory.SetFixedIslands(new GridIslandGenerationRequestFactory.FixedIsland(Vector2Int.zero));
            factory.GenerationSettings.SnapshotCacheDirectory = cache;
            factory.GenerationSettings.UseSnapshotCache = false;
            var request = factory.CreateIslandGenerationRequest(Vector2Int.zero);
            var snapshotPath = IslandSnapshotCache.PathFor(request);
            Assert.IsTrue(IslandHorizonCache.TrySave(snapshotPath, Triangle(), 1024 * 1024));
            var manager = root.AddComponent<IslandWorldManager>();
            manager.ConfigureIslandGenerationRequestFactory(factory);
            manager.SetStreamingTarget(target.transform);
            root.SetActive(true);
            var environment = manager.EnvironmentController;
            try
            {
                yield return null;
                // Simulate fast incoming travel: its projected position is inside the corridor
                // while the actual viewer remains beyond the silhouette retention radius.
                manager.smoothedVelocity = Vector3.left * 350;
                var deadline = Time.realtimeSinceStartup + 10f;
                while (manager.HorizonIslandCount == 0)
                {
                    Assert.That(Time.realtimeSinceStartup, Is.LessThan(deadline));
                    yield return null;
                }
                var horizon = root.GetComponentInChildren<IslandHorizonRenderer>();
                for (var frame = 0; frame < 5; frame++)
                {
                    yield return null;
                    Assert.AreSame(horizon, root.GetComponentInChildren<IslandHorizonRenderer>());
                }
                Assert.IsFalse(factory.GenerationSettings.UseSnapshotCache);
                Assert.IsNull(request.SnapshotPath);
                Assert.That(manager.LoadedIslandCount, Is.Zero);
                manager.SetStreamingTarget(target.transform); // Remove travel prediction.
                yield return null;
                Assert.That(manager.HorizonIslandCount, Is.Zero);
            }
            finally
            {
                Object.Destroy(root); Object.Destroy(target);
                if (environment != null) Object.Destroy(environment.gameObject);
            }
            yield return null;
            if (Directory.Exists(cache)) Directory.Delete(cache, true);
            yield return new ExitPlayMode();
        }

        [UnityTest, Timeout(600000), Category("NativeIntegration")]
        public IEnumerator DistantIslandReleasesDetailsAndRestoresThemFromDisk()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            // SetUp fields are not preserved by the play-mode domain reload.
            var cache = Path.Combine(Path.GetTempPath(), "motu-residency-test-" + Guid.NewGuid().ToString("N"));
            var root = new GameObject("Disk residency world");
            root.SetActive(false);
            var target = new GameObject("Residency viewer");
            target.transform.position = Vector3.right * 3500;
            var factory = root.AddComponent<GridIslandGenerationRequestFactory>();
            factory.Configure(17, false, 0);
            factory.SetFixedIslands(new GridIslandGenerationRequestFactory.FixedIsland(Vector2Int.zero, "disk-residency"));
            factory.GenerationSettings.SnapshotCacheDirectory = cache;
            factory.GenerationSettings.UseSnapshotCache = true;
            JsonUtility.FromJsonOverwrite("{\"forestPrototypeCount\":1}", factory.ForestSettings);
            JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", factory.RenderingSettings);
            var request = factory.CreateIslandGenerationRequest(Vector2Int.zero);
            var manager = root.AddComponent<IslandWorldManager>();
            manager.ConfigureIslandGenerationRequestFactory(factory);
            manager.SetStreamingTarget(target.transform);
            var baseline = NativeIslandHandle.ActiveCount;
            root.SetActive(true);
            var environment = manager.EnvironmentController;
            var deadline = Time.realtimeSinceStartup + 450f;
            try
            {
                while (manager.HorizonIslandCount != 1 || manager.GeneratingIslandCount != 0)
                {
                    Assert.That(Time.realtimeSinceStartup, Is.LessThan(deadline), "Horizon installation timed out.");
                    yield return null;
                }
                Assert.That(manager.LoadedIslandCount, Is.Zero);
                Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(baseline));
                Assert.IsEmpty(root.GetComponentsInChildren<Collider>(true));
                Assert.IsEmpty(root.GetComponentsInChildren<Motu.Streaming.TerrainTileStreamer>(true));
                Assert.IsTrue(File.Exists(request.SnapshotPath));
                Assert.IsTrue(File.Exists(IslandHorizonCache.PathFor(request.SnapshotPath)));
                var horizon = root.GetComponentInChildren<IslandHorizonRenderer>();
                Assert.IsTrue(horizon.GetComponentInChildren<MeshRenderer>().enabled);
                var initialSidecar = File.ReadAllBytes(IslandHorizonCache.PathFor(request.SnapshotPath));

                foreach (var switchDistance in new[] { 1000f, 500f, 5000f })
                {
                    manager.Lod3DistanceMetres = switchDistance;
                    target.transform.position = Vector3.right * (switchDistance + 400f);
                    manager.SetStreamingTarget(target.transform);
                    while (manager.LoadedIslandCount != 1 || manager.GeneratingIslandCount != 0)
                    {
                        Assert.That(Time.realtimeSinceStartup, Is.LessThan(deadline), "Detailed loading timed out.");
                        yield return null;
                    }
                    var generator = root.GetComponentInChildren<IslandGenerator>();
                    Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(baseline + 1));
                    Assert.IsTrue(generator.Runtime.IsLod3Visible, "Preloading must retain LOD3 beyond the configured switch distance.");
                    Assert.IsFalse(horizon.GetComponentInChildren<MeshRenderer>().enabled);
                    if (switchDistance == 1000f)
                    {
                        manager.Lod3DistanceMetres = 2000f;
                        Assert.IsFalse(generator.Runtime.IsLod3Visible, "A runtime quality change must update an already loaded island immediately.");
                        manager.Lod3DistanceMetres = switchDistance;
                        Assert.IsTrue(generator.Runtime.IsLod3Visible);
                    }
                    target.transform.position = Vector3.right * (switchDistance - 100f);
                    manager.SetStreamingTarget(target.transform);
                    Assert.IsFalse(generator.Runtime.IsLod3Visible);
                    if (switchDistance <= 1000f) Assert.IsNotNull(manager.FocusedIsland);
                    StringAssert.StartsWith("Disk cache", generator.Status);
                    var detailMesh = generator.Runtime.TerrainStreamer.GetComponentInChildren<MeshFilter>(true).sharedMesh;
                    var detailMaterial = generator.Runtime.TerrainStreamer.GetComponentsInChildren<MeshRenderer>(true)
                        .Select(renderer => renderer.sharedMaterial)
                        .First(material => material.HasProperty("_WorldNormal") && material.GetTexture("_WorldNormal") != null);
                    var detailNormalTexture = detailMaterial.GetTexture("_WorldNormal");
                    Assert.IsNotNull(detailNormalTexture);
                    target.transform.position = Vector3.right * (switchDistance + 600f);
                    manager.SetStreamingTarget(target.transform);
                    Assert.That(manager.LoadedIslandCount, Is.EqualTo(1), "The unload band must prevent reload churn.");
                    target.transform.position = Vector3.right * (switchDistance + 900f);
                    manager.SetStreamingTarget(target.transform);
                    yield return null;
                    yield return null;
                    Assert.That(manager.LoadedIslandCount, Is.Zero);
                    Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(baseline));
                    Assert.IsTrue(detailMesh == null, "Detailed GPU meshes must be destroyed on downgrade.");
                    Assert.IsTrue(detailMaterial == null, "Generated terrain materials must be destroyed.");
                    Assert.IsTrue(detailNormalTexture == null, "Generated terrain textures must be destroyed.");
                    Assert.IsEmpty(root.GetComponentsInChildren<Collider>(true));
                    Assert.IsTrue(horizon.GetComponentInChildren<MeshRenderer>().enabled);
                    CollectionAssert.AreEqual(initialSidecar, File.ReadAllBytes(IslandHorizonCache.PathFor(request.SnapshotPath)));
                }

                // Leave during another upgrade, then destroy the world while workers drain.
                target.transform.position = Vector3.right * 900;
                manager.SetStreamingTarget(target.transform);
                yield return null;
                Object.Destroy(root);
                yield return null;
                var drained = IslandGenerationWorker.GenerateHorizonAsync(request, CancellationToken.None);
                while (!drained.IsCompleted)
                {
                    Assert.That(Time.realtimeSinceStartup, Is.LessThan(deadline));
                    yield return null;
                }
                drained.GetAwaiter().GetResult();
                yield return null;
                yield return null;
                Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(baseline));
                Assert.IsTrue(horizon == null);
            }
            finally
            {
                Object.Destroy(root);
                Object.Destroy(target);
                if (environment != null) Object.Destroy(environment.gameObject);
            }
            yield return null;
            if (Directory.Exists(cache)) Directory.Delete(cache, true);
            yield return new ExitPlayMode();
        }
    }
}
