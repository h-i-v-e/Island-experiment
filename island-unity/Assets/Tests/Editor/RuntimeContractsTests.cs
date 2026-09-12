using System;
using System.Collections;
using System.Linq;
using System.Reflection;
using System.Threading;
using NUnit.Framework;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class RuntimeContractsTests
    {
        [Test] public void WeatherAndWaves() => VegetationWindValidation.BatchValidateSharedWind();
        [Test] public void ShoreBreaking() => OceanShoreDistanceValidation.BatchValidateShoreDistance();
        [Test] public void WaveTransitions() => OceanWaveTransitionValidation.BatchValidateWaveTransitions();
        [Test] public void MinimapTeleport() => MinimapTeleportValidation.BatchValidateMinimapTeleport();
        [Test] public void SoilPolicyAndRequestIsolation() => IslandRequestValidation.ValidateSoilAndSnapshots();
        [Test] public void CancellationAndRestart() => IslandRequestValidation.ValidateCancellation();
        [Test]
        public void RuntimeOwnership()
        {
            IslandOwnershipValidation.ValidateOwnershipContract();
            OceanRenderingValidation.ValidateCoastalMaskIsolation();
        }
        [Test] public void MaterialCacheRoundTrip() => MaterialCacheValidation.ValidateRoundTrip();
        [Test] public void SupportedScenes() => SceneMigrationValidation.ValidateScenes();

        [Test]
        public void ProfileCopiesPreserveValuesAndAuthoredAssets()
        {
            var material = new Material(Shader.Find("Motu/Terrain Unified"));
            try
            {
                var rendering = new IslandRenderingSettings();
                rendering.AssignMaterialTemplates(material, material, material, material, material, material);
                var profile = new IslandGenerationProfile(new IslandGenerationSettings(), new IslandRiverSettings(),
                    new IslandForestSettings(), new IslandReedSettings(), new IslandFernSettings(), rendering, new IslandDebugSettings());
                var copy = profile.Clone();
                foreach (var property in typeof(IslandGenerationProfile).GetProperties())
                {
                    var original = property.GetValue(profile);
                    var snapshot = property.GetValue(copy);
                    Assert.AreNotSame(original, snapshot);
                    Assert.AreEqual(JsonUtility.ToJson(original), JsonUtility.ToJson(snapshot), property.Name);
                    foreach (var field in original.GetType().GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic))
                    {
                        // Adding a mutable nested reference requires extending the copy contract.
                        Assert.IsTrue(field.FieldType.IsValueType || typeof(Object).IsAssignableFrom(field.FieldType), field.Name);
                    }
                }
                Assert.AreSame(material, copy.Rendering.TerrainMaterial);
                profile.Generation.MaximumHeightMetres = 123f;
                Assert.AreNotEqual(profile.Generation.MaximumHeightMetres, copy.Generation.MaximumHeightMetres);
            }
            finally { Object.DestroyImmediate(material); }
        }

        [Test]
        public void VegetationTilesRetainOverlapAndRecoverFromInterruptedTravel()
        {
            var host = new GameObject("Vegetation ownership fixture");
            var material = new Material(Shader.Find("Motu/Forest Ferns"));
            using var reeds = new ReedTileStreamer();
            using var ferns = new FernTileStreamer();
            try
            {
                var triangle = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.right, Vector3.forward },
                    new[] { Vector3.up, Vector3.up, Vector3.up }, new[] { 0, 1, 2 },
                    Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
                var prepared = Enumerable.Repeat(triangle, 64 * 64).ToArray();
                reeds.Initialize(host.transform, material, prepared, true);
                ferns.Initialize(host.transform, material, prepared, true);
                reeds.UpdateLod0Neighborhood(new Vector2Int(31, 32));
                ferns.UpdateLod0Neighborhood(new Vector2Int(31, 32));
                Assert.AreEqual(9, reeds.ActiveTileCount);
                Assert.AreEqual(9, ferns.ActiveTileCount);
                var before = host.GetComponentsInChildren<MeshFilter>().Select(value => value.sharedMesh).ToArray();
                reeds.UpdateLod0Neighborhood(new Vector2Int(32, 32));
                ferns.UpdateLod0Neighborhood(new Vector2Int(32, 32));
                var after = host.GetComponentsInChildren<MeshFilter>().Select(value => value.sharedMesh).ToArray();
                Assert.AreEqual(12, before.Intersect(after).Count(), "Six tiles per species should be retained.");
                var interrupted = reeds.UpdateLod0NeighborhoodIncremental(new Vector2Int(50, 50), () => false);
                Assert.IsTrue(interrupted.MoveNext());
                Assert.IsFalse(interrupted.MoveNext());
                reeds.UpdateLod0Neighborhood(Vector2Int.zero);
                ferns.UpdateLod0Neighborhood(Vector2Int.zero);
                Assert.AreEqual(4, reeds.ActiveTileCount);
                Assert.AreEqual(4, ferns.ActiveTileCount);
                reeds.Dispose();
                ferns.Dispose();
                Assert.IsEmpty(host.GetComponentsInChildren<MeshFilter>());
                Assert.IsTrue(after.All(mesh => mesh == null), "Disposal leaked uploaded meshes.");
                Assert.IsTrue(material != null, "A tile disposed its borrowed material.");
            }
            finally { Object.DestroyImmediate(host); Object.DestroyImmediate(material); }
        }

        [UnityTest, Timeout(600000), Category("NativeIntegration")]
        public IEnumerator GenerationCancellationInstallationAndUnload()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var environmentHost = new GameObject("Lifecycle world");
            var islandHost = new GameObject("Lifecycle island");
            var environment = environmentHost.AddComponent<WorldEnvironmentController>();
            environment.Initialize(new WorldEnvironmentSettings(), new IslandCloudSettings(), 64f, 128f, null);
            var sky = environment.SkyMaterial;
            var weatherNoise = environment.WeatherNoiseTexture;
            var generator = islandHost.AddComponent<IslandGenerator>();
            generator.ConfigureWorldManagement(environment);
            var generation = new IslandGenerationSettings { Seed = 17, WaterRatio = .85f, MaximumHeightMetres = 80f, UseSnapshotCache = false };
            var forest = new IslandForestSettings();
            JsonUtility.FromJsonOverwrite("{\"forestPrototypeCount\":1}", forest);
            var rendering = new IslandRenderingSettings();
            JsonUtility.FromJsonOverwrite("{\"materialTextureResolution\":16}", rendering);
            var request = new IslandGenerationRequest(17, Vector2Int.zero, generation, new IslandRiverSettings(), forest,
                new IslandReedSettings(), new IslandFernSettings(), rendering, default);
            var handles = NativeIslandHandle.ActiveCount;
            try
            {
                using (var cancellation = new CancellationTokenSource())
                {
                    var pending = generator.GenerateAsync(request, cancellation.Token, .5f);
                    cancellation.Cancel();
                    while (!pending.IsCompleted) yield return null;
                    Assert.IsFalse(pending.GetAwaiter().GetResult());
                    Assert.AreEqual(handles, NativeIslandHandle.ActiveCount);
                }
                using (var cancellation = new CancellationTokenSource())
                {
                    var pending = generator.GenerateAsync(request, cancellation.Token, .5f);
                    while (generator.Runtime == null && !pending.IsCompleted) yield return null;
                    Assert.IsNotNull(generator.Runtime, "Installation did not reach ownership transfer.");
                    cancellation.Cancel();
                    while (!pending.IsCompleted) yield return null;
                    Assert.IsFalse(pending.GetAwaiter().GetResult());
                    Assert.IsFalse(generator.HasRuntime);
                    Assert.AreEqual(handles, NativeIslandHandle.ActiveCount);
                }
                var installed = generator.GenerateAsync(request, CancellationToken.None, .5f);
                while (!installed.IsCompleted) yield return null;
                Assert.IsTrue(installed.GetAwaiter().GetResult(), generator.Status);
                Assert.IsTrue(generator.HasActiveRuntime);
                IslandLod3Tests.ValidateInstalledLod3(generator.Runtime, environment);
                Assert.IsFalse(islandHost.GetComponentsInChildren<Transform>(true)
                    .Any(value => value.name == "Island Coastal Water Overlay"));
                var meshes = islandHost.GetComponentsInChildren<MeshFilter>(true).Select(value => value.sharedMesh)
                    .Where(mesh => mesh != null && !UnityEditor.EditorUtility.IsPersistent(mesh)).Distinct().ToArray();
                generator.Clear();
                generator.Clear();
                Assert.AreEqual(handles, NativeIslandHandle.ActiveCount);
                Assert.IsTrue(meshes.All(mesh => mesh == null));
                Assert.AreSame(sky, environment.SkyMaterial);
                Assert.AreSame(weatherNoise, environment.WeatherNoiseTexture);
                Assert.IsTrue(environment.IsInstalled);
            }
            finally { Object.DestroyImmediate(islandHost); Object.DestroyImmediate(environmentHost); }
        }
    }
}
