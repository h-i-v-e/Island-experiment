using System;
using System.Collections;
using System.IO;
using System.Linq;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class HostIntegrationTests
    {
        [Test]
        public void NativeBinaryMatchesManagedLayouts() => NativeAbi.Validate();

        [Test]
        public void RuntimeDoesNotReferenceSamplesOrNavigation()
        {
            var references = typeof(IslandGenerator).Assembly.GetReferencedAssemblies().Select(a => a.Name);
            Assert.That(references, Does.Not.Contain("Motu.Samples"));
            Assert.That(references, Does.Not.Contain("Motu.Navigation"));
            Assert.That(references, Does.Not.Contain("UnityEditor"));
            Assert.That(references, Does.Not.Contain("UnityEngine.AIModule"));
        }

        [Test]
        public void ShaderLibraryContainsEveryRuntimeShader()
        {
            var library = Resources.Load<MotuShaderLibrary>("Motu/ShaderLibrary");
            Assert.IsNotNull(library);
            Assert.Greater(library.Shaders.Length, 15);
            foreach (var shader in library.Shaders)
            {
                Assert.IsNotNull(shader);
                Assert.IsTrue(shader.isSupported, shader.name);
                Assert.IsFalse(ShaderUtil.ShaderHasError(shader), shader.name);
            }
        }

        [Test]
        public void SharedNoiseLivesUntilTheLastIslandReleasesIt()
        {
            for (var cycle = 0; cycle < 20; cycle++)
            {
                var first = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.Weather);
                var second = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.Weather);
                var texture = first.Texture;
                Assert.AreSame(texture, second.Texture);
                first.Dispose(); first.Dispose();
                Assert.IsTrue(texture != null);
                second.Dispose();
                Assert.IsTrue(texture == null);
            }
        }

        [Test]
        public void CacheLocationIsSnapshottedWithoutCreatingDirectories()
        {
            var directory = Path.Combine(Path.GetTempPath(), "motu-cache-contract-" + Guid.NewGuid().ToString("N"));
            var host = new GameObject("Factory");
            try
            {
                var factory = host.AddComponent<SingleIsland>();
                factory.GenerationSettings.SnapshotCacheDirectory = directory;
                var request = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.AreEqual(directory, Path.GetDirectoryName(request.SnapshotPath));
                Assert.IsFalse(Directory.Exists(directory));
                factory.GenerationSettings.SnapshotCacheDirectory = null;
                Assert.AreEqual(directory, request.Generation.SnapshotCacheDirectory);
                Assert.Throws<ArgumentException>(() => factory.GenerationSettings.SnapshotCacheDirectory = "relative/cache");
            }
            finally { Object.DestroyImmediate(host); }
        }

        [Test]
        public void CacheEvictionPreservesHostFilesAndIncompleteWrites()
        {
            var directory = Path.Combine(Path.GetTempPath(), "motu-eviction-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(directory);
            try
            {
                var old = Path.Combine(directory, new string('a', 64) + ".motusnapshot");
                var current = Path.Combine(directory, new string('b', 64) + ".motumaterials");
                var temporary = current + ".tmp-writing";
                var unrelated = Path.Combine(directory, "notes.motumaterials");
                foreach (var path in new[] { old, current, temporary, unrelated }) File.WriteAllBytes(path, new byte[32]);
                File.SetLastWriteTimeUtc(old, DateTime.UtcNow.AddDays(-1));
                IslandSnapshotCache.TrimToBudget(directory, 32, current);
                Assert.IsFalse(File.Exists(old));
                foreach (var path in new[] { current, temporary, unrelated }) Assert.IsTrue(File.Exists(path), path);
            }
            finally { Directory.Delete(directory, true); }
        }

        [UnityTest, Timeout(1800000)]
        public IEnumerator IslandGeneratesAndUnloadsWithoutMotuEnvironmentOrSamples()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var camera = new GameObject("Host camera").AddComponent<Camera>();
            camera.depthTextureMode = DepthTextureMode.None;
            var light = new GameObject("Host sun").AddComponent<Light>();
            light.type = LightType.Directional;
            RenderSettings.sun = light;
            var fog = RenderSettings.fog;
            var ambient = RenderSettings.ambientLight;
            var host = new GameObject("Independent island");
            var island = host.AddComponent<SingleIsland>();
            island.GenerateOnStart = false;
            island.GenerationSettings.Seed = 17;
            island.GenerationSettings.WaterRatio = .85f;
            island.GenerationSettings.MaximumHeightMetres = 80f;
            island.GenerationSettings.UseSnapshotCache = false;
            island.NavigationSettings.Enabled = false;
            JsonUtility.FromJsonOverwrite("{\"forestPrototypeCount\":1}", island.ForestSettings);
            JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", island.RenderingSettings);
            var handles = NativeIslandHandle.ActiveCount;
            try
            {
                var cycles = int.TryParse(Environment.GetEnvironmentVariable("MOTU_LIFECYCLE_CYCLES"), out var count)
                    ? Mathf.Clamp(count, 1, 20) : 1;
                for (var cycle = 0; cycle < cycles; cycle++)
                {
                    yield return null; // An unconfigured generator must remain inert.
                    var pending = island.GenerateAsync(CancellationToken.None);
                    while (!pending.IsCompleted) yield return null;
                    Assert.IsTrue(pending.GetAwaiter().GetResult(), island.Generator.Status);
                    Assert.IsTrue(island.Generator.HasActiveRuntime);
                    Assert.IsNull(island.Generator.Runtime.Navigation);
                    Assert.Greater(host.GetComponentsInChildren<MeshFilter>(true).Length, 0);
                    Assert.AreEqual(DepthTextureMode.None, camera.depthTextureMode);
                    Assert.AreSame(light, RenderSettings.sun);
                    Assert.AreEqual(fog, RenderSettings.fog);
                    Assert.AreEqual(ambient, RenderSettings.ambientLight);
                    Assert.AreEqual(1, Object.FindObjectsByType<Camera>(FindObjectsSortMode.None).Length);
                    var textures = host.GetComponentsInChildren<Renderer>(true).SelectMany(r => r.sharedMaterials)
                        .Where(m => m != null).SelectMany(m => m.GetTexturePropertyNames().Select(m.GetTexture))
                        .Where(t => t != null && !EditorUtility.IsPersistent(t)).Distinct().ToArray();
                    var meshes = host.GetComponentsInChildren<MeshFilter>(true).Select(m => m.sharedMesh)
                        .Where(m => m != null && !EditorUtility.IsPersistent(m)).Distinct().ToArray();
                    island.Clear();
                    island.Clear();
                    Assert.IsTrue(meshes.All(m => m == null), "An owned mesh leaked.");
                    Assert.AreEqual(handles, NativeIslandHandle.ActiveCount);
                    Assert.IsFalse(island.Generator.HasRuntime);
                    Assert.IsTrue(textures.Where(t => t != Texture2D.whiteTexture && t != Texture2D.blackTexture
                        && t != Texture2D.grayTexture && t != Texture2D.normalTexture).All(t => t == null), "An owned runtime texture leaked.");
                    Debug.Log($"MOTU LIFECYCLE cycle={cycle + 1}: generated and released all captured owned meshes/textures; handles={NativeIslandHandle.ActiveCount}");
                }
            }
            finally
            {
                Object.DestroyImmediate(host);
                Object.DestroyImmediate(camera.gameObject);
                Object.DestroyImmediate(light.gameObject);
            }
        }
    }
}
