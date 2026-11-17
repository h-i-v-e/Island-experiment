using System;
using System.Collections;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using Motu.Towns;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using Motu.Editor;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalTownNativeIntegrationTests
    {
        private static readonly Lazy<TownPreparedData> Fixture = new Lazy<TownPreparedData>(() => TownNative.Generate(Recipe(), Input(true)));
        private static readonly Lazy<RealTownScene> RealFixture = new Lazy<RealTownScene>(GenerateRealCoast);

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void EditorBakerPersistsSupportlessPrefabsAndTheirSharedResources()
        {
            var destination = AssetDatabase.GenerateUniqueAssetPath("Assets/TownPrefabBakeTest");
            try
            {
                var paths = TownPrefabBaker.Bake(Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue"), 42, destination);
                Assert.That(paths.Length, Is.EqualTo(3));
                AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
                foreach (var path in paths)
                {
                    var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(path);
                    Assert.That(prefab, Is.Not.Null, path);
                    Assert.That(prefab.GetComponent<LODGroup>().GetLODs().Length, Is.EqualTo(3));
                    Assert.That(prefab.GetComponentsInChildren<MeshFilter>(true).All(f => EditorUtility.IsPersistent(f.sharedMesh)), Is.True);
                    var materials = prefab.GetComponentsInChildren<MeshRenderer>(true).SelectMany(r => r.sharedMaterials).Distinct().ToArray();
                    Assert.That(materials.All(EditorUtility.IsPersistent), Is.True);
                    Assert.That(materials.Where(m => m.mainTexture != null).All(m => EditorUtility.IsPersistent(m.mainTexture)), Is.True);
                    Assert.That(materials.Any(m => m.mainTexture != null), Is.True);
                    foreach (var material in materials)
                        foreach (var property in material.GetTexturePropertyNames())
                        {
                            var texture = material.GetTexture(property);
                            if (texture == null) continue;
                            var importer = (TextureImporter)AssetImporter.GetAtPath(AssetDatabase.GetAssetPath(texture));
                            Assert.That(importer.sRGBTexture, Is.EqualTo(property == "_BaseMap"), property);
                        }
                    Assert.That(prefab.GetComponentsInChildren<MeshCollider>(true), Is.Empty,
                        "Fitted town foundations belong to the runtime support mesh, not reusable shell prefabs.");
                }
            }
            finally { if (AssetDatabase.IsValidFolder(destination)) AssetDatabase.DeleteAsset(destination); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void RealCoastalTerrainExportsTownInputAndAcceptsNativeClearance()
        {
            var result = RealFixture.Value.town;
            Assert.That(result.schema_version, Is.EqualTo(1));
            Assert.That(result.clearance_json, Is.Not.Null.And.Not.Empty);
            Assert.That(result.instances.Length, Is.GreaterThan(0), "The fixed seed-42 coast must exercise actual building/support export and clearance.");
            TestContext.Progress.WriteLine($"Real coast town roundtrip: {result.instances.Length} buildings, {result.supports.indices.Length / 3} support triangles.");
        }

        private sealed class RealTownScene
        {
            internal TownPreparedData town;
            internal IslandPreparedMesh terrain;
        }

        private static RealTownScene GenerateRealCoast()
        {
            var generation = new IslandGenerationSettings {
                TerrainShape = IslandTerrainShape.CoastalStrip, UseSnapshotCache = false, CoastalSectionCount = 8
            };
            var profile = new IslandGenerationProfile(generation, new IslandRiverSettings(), new IslandForestSettings(),
                new IslandReedSettings(), new IslandFernSettings(), new IslandRenderingSettings(), new IslandDebugSettings(),
                new IslandCaveSettings { Enabled = false }, towns: new CoastalTownSettings { enabled = false });
            var request = new IslandGenerationRequest(42, Vector2Int.zero, profile, default);
            var snapshot = Environment.GetEnvironmentVariable("MOTU_TOWN_RENDER_SNAPSHOT");
            var handle = !string.IsNullOrEmpty(snapshot) && File.Exists(snapshot)
                ? MotuNative.LoadMotuSnapshot(snapshot, out _) : IntPtr.Zero;
            if (handle == IntPtr.Zero) handle = IslandPreparationPipeline.LoadOrGenerateNative(request.RandomSeed, request.Options,
                request.ForestOptions, request.ReedOptions, request.FernOptions, null, 0,
                request.CaveOptions, request.CaveNetworkOptions, request.CaveWalkOptions,
                out _, request.HybridForestOptions, request.CoastalStripOptions);
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero));
            try
            {
                var catalogue = Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue");
                var recipe = catalogue.CaptureJson(out var dependencies);
                var result = TownNative.Prepare(handle, recipe, request.TownSeed, dependencies);
                Assert.That(result.schema_version, Is.EqualTo(1));
                Assert.That(result.seed, Is.EqualTo(request.TownSeed.ToString()));
                Assert.That(result.clearance_json, Is.Not.Null.And.Not.Empty);
                Assert.That(result.instances.Length, Is.GreaterThan(0), "The fixed seed-42 coast must exercise actual building/support export and clearance.");
                return new RealTownScene { town = result, terrain = TerrainCrop(handle, result) };
            }
            finally { MotuNative.ReleaseMotu(handle); }
        }

        private static IslandPreparedMesh TerrainCrop(IntPtr handle, TownPreparedData town)
        {
            var x = town.instances.Average(instance => instance.position[0]);
            var y = town.instances.Average(instance => instance.position[1]);
            var area = new MotuNative.ExportArea(Mathf.Clamp01((x - 160f) / 2000f), Mathf.Clamp01((y - 160f) / 2000f),
                Mathf.Clamp01((x + 160f) / 2000f), Mathf.Clamp01((y + 160f) / 2000f));
            var memory = Marshal.AllocHGlobal(Marshal.SizeOf<MotuNative.ExportArea>());
            try
            {
                Marshal.StructureToPtr(area, memory, false);
                MotuNative.CreateMesh(handle, memory, 0, 0, out var export);
                try { return IslandMeshInterop.CopyTerrainMeshData(export, 0, 2000f); }
                finally { MotuNative.ReleaseMesh(ref export); }
            }
            finally { Marshal.FreeHGlobal(memory); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void NativeBuildingPackageImportsGeometryTexturesDoorsAndCombinedSupports()
        {
            var data = Fixture.Value;
            Assert.That(data.instances.Length, Is.EqualTo(2));
            Assert.That(data.prototypes.Length, Is.EqualTo(1));
            Assert.That(data.prototypes[0].lods.Length, Is.EqualTo(3));
            Assert.That(data.supports.indices.Length, Is.GreaterThan(0));
            Assert.That(data.supports.grub_positions.Length, Is.EqualTo(data.supports.positions.Length));
            Assert.That(data.support_material.grub_uniform.Length, Is.EqualTo(20));
            var owner = new GameObject("Native town import fixture");
            try
            {
                var town = TownRenderer.Install(owner.transform, data, 2000f);
                Assert.That(town.BuildingCount, Is.EqualTo(2));
                Assert.That(town.GetComponentsInChildren<TownDoor>().Length, Is.GreaterThanOrEqualTo(2));
                Assert.That(town.GetComponentsInChildren<MeshCollider>().Length, Is.EqualTo(1));
                var supports = town.GetComponentInChildren<MeshCollider>();
                Assert.That(supports.GetComponent<MeshRenderer>().sharedMaterials.Length, Is.EqualTo(1));
                Physics.SyncTransforms();
                var mesh = supports.sharedMesh;
                var vertices = mesh.vertices;
                var triangles = mesh.triangles;
                var foundSupport = false;
                for (var triangle = 0; triangle < triangles.Length && !foundSupport; triangle += 3)
                {
                    if (mesh.normals[triangles[triangle]].y < 0.9f) continue;
                    var local = (vertices[triangles[triangle]] + vertices[triangles[triangle + 1]] + vertices[triangles[triangle + 2]]) / 3f;
                    var surface = supports.transform.TransformPoint(local);
                    foundSupport = supports.Raycast(new Ray(surface + Vector3.up * 0.1f, Vector3.down), out var hit, 0.2f)
                        && Vector3.Distance(hit.point, surface) < 0.002f;
                }
                Assert.That(foundSupport, Is.True, "The imported native support mesh must provide a physical upward-facing walking surface.");
                Assert.That(town.GetComponentsInChildren<MeshRenderer>().SelectMany(r => r.sharedMaterials)
                    .Any(m => m.mainTexture != null), Is.True);
            }
            finally { Object.DestroyImmediate(owner); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void NativeRiverlessInputReturnsEmptyPackage()
        {
            var result = TownNative.Generate(Recipe(), Input(false));
            Assert.That(result.instances, Is.Empty);
            Assert.That(result.prototypes, Is.Empty);
            Assert.That(result.mouth, Is.Empty);
        }

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator NativeTownMaterialsRenderOnMetal()
            => RenderTown(Fixture.Value, null, "native-town-fixture");

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator RealCoastalTownAndFittedSupportsRenderOnNativeTerrain()
            => RenderTown(RealFixture.Value.town, RealFixture.Value.terrain, "real-coast-town");

        private static IEnumerator RenderTown(TownPreparedData data, IslandPreparedMesh terrain, string fileName)
        {
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
                Assert.Ignore("Town camera validation requires a graphics device.");
            var oldPipeline = GraphicsSettings.defaultRenderPipeline;
            var oldQualityPipeline = QualitySettings.renderPipeline;
            var oldAmbient = RenderSettings.ambientLight;
            var oldMode = RenderSettings.ambientMode;
            var oldProbe = RenderSettings.ambientProbe;
            var oldTarget = RenderTexture.active;
            var root = new GameObject("Native town camera fixture");
            var renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
            var pipeline = UniversalRenderPipelineAsset.Create(renderer);
            var target = new RenderTexture(1024, 768, 24);
            var image = new Texture2D(1024, 768, TextureFormat.RGB24, false);
            Material groundMaterial = null;
            Mesh groundMesh = null;
            try
            {
                GraphicsSettings.defaultRenderPipeline = pipeline;
                QualitySettings.renderPipeline = pipeline;
                var town = TownRenderer.Install(root.transform, data, 2000f);
                var bounds = town.GetComponentsInChildren<MeshRenderer>().Select(r => r.bounds)
                    .Aggregate((a, b) => { a.Encapsulate(b); return a; });
                var ground = new GameObject("Native terrain beneath town");
                ground.transform.SetParent(root.transform, false);
                groundMaterial = new Material(Shader.Find("Universal Render Pipeline/Lit"));
                groundMaterial.SetColor("_BaseColor", new Color(0.16f, 0.23f, 0.09f));
                if (terrain != null) groundMesh = IslandMeshInterop.CreateTerrainMesh(terrain, 0);
                else
                {
                    groundMesh = new Mesh {
                        vertices = new[] { new Vector3(bounds.min.x - 10, 2, bounds.min.z - 10), new Vector3(bounds.max.x + 10, 2, bounds.min.z - 10),
                            new Vector3(bounds.min.x - 10, 2, bounds.max.z + 10), new Vector3(bounds.max.x + 10, 2, bounds.max.z + 10) },
                        triangles = new[] { 0, 2, 1, 1, 2, 3 }, uv = new[] { Vector2.zero, Vector2.right, Vector2.up, Vector2.one }
                    };
                    groundMesh.RecalculateNormals();
                }
                ground.AddComponent<MeshFilter>().sharedMesh = groundMesh;
                ground.AddComponent<MeshRenderer>().sharedMaterial = groundMaterial;
                var cameraRoot = new GameObject("Town review camera");
                cameraRoot.transform.SetParent(root.transform);
                var camera = cameraRoot.AddComponent<Camera>();
                camera.enabled = false;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = new Color(0.08f, 0.14f, 0.22f);
                camera.orthographic = true;
                camera.orthographicSize = bounds.extents.magnitude * 1.1f;
                camera.nearClipPlane = 0.1f;
                camera.farClipPlane = 2000f;
                var focus = bounds.center;
                camera.transform.position = focus + new Vector3(18f, 15f, -22f).normalized * bounds.extents.magnitude * 3f;
                camera.transform.LookAt(focus);
                var sunRoot = new GameObject("Town review sun");
                sunRoot.transform.SetParent(root.transform);
                var sun = sunRoot.AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 0.55f;
                sun.transform.rotation = Quaternion.Euler(45f, -30f, 0f);
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = new Color(0.6f, 0.6f, 0.6f);
                var probe = new SphericalHarmonicsL2();
                probe.AddAmbientLight(new Color(0.2f, 0.2f, 0.2f));
                RenderSettings.ambientProbe = probe;
                yield return null;
                CaptureFrame(camera, target, image, fileName + ".png");
                camera.orthographicSize = 13f;
                focus = town.InspectionLocalPosition + Vector3.up * 2f;
                camera.transform.position = focus + new Vector3(18f, 15f, -22f);
                camera.transform.LookAt(focus);
                CaptureFrame(camera, target, image, fileName + "-supports.png");
            }
            finally
            {
                GraphicsSettings.defaultRenderPipeline = oldPipeline;
                QualitySettings.renderPipeline = oldQualityPipeline;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.ambientMode = oldMode;
                RenderSettings.ambientProbe = oldProbe;
                RenderTexture.active = oldTarget;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(pipeline);
                Object.DestroyImmediate(renderer);
                if (groundMaterial != null) Object.DestroyImmediate(groundMaterial);
                if (groundMesh != null) Object.DestroyImmediate(groundMesh);
            }
        }

        private static void CaptureFrame(Camera camera, RenderTexture target, Texture2D image, string fileName)
        {
            RenderPipeline.SubmitRenderRequest(camera, new UniversalRenderPipeline.SingleCameraRequest { destination = target });
            RenderTexture.active = target;
            image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0);
            image.Apply();
            var path = Path.Combine(Path.DirectorySeparatorChar == '/' ? "/tmp" : Path.GetTempPath(), "coastal-towns-20261117", fileName);
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllBytes(path, image.EncodeToPNG());
            TestContext.Progress.WriteLine("Native town camera render: " + path);
            var pixels = image.GetPixels32();
            var magenta = pixels.Count(c => c.r > 230 && c.g < 25 && c.b > 230);
            var visible = pixels.Count(c => Mathf.Abs(c.r / 255f - camera.backgroundColor.r) > 0.1f);
            Assert.That(magenta, Is.LessThan(pixels.Length / 100));
            Assert.That(visible, Is.GreaterThan(1000));
            Assert.That(ShaderUtil.ShaderHasError(Resources.Load<Shader>("Towns/TownBuilding")), Is.False);
        }
        [Serializable] private sealed class FixtureRecipe { public int schema_version = 1; public FixtureEntry[] catalogue; }
        [Serializable] private sealed class FixtureEntry
        {
            public string id = "fixture-cottage", placement = "centre", building_json;
            public int min_count = 2, max_count = 2;
        }
        private static string Recipe()
        {
            var building = Resources.Load<TextAsset>("Towns/Recipes/cottage").text.Replace("\"atlas_size\": 1024", "\"atlas_size\": 128");
            return JsonUtility.ToJson(new FixtureRecipe { catalogue = new[] { new FixtureEntry { building_json = building } } });
        }
        private static string Input(bool rivers)
        {
            var heights = string.Join(",", Enumerable.Range(0, 441).Select(i => i < 21 ? "-2" : "2"));
            return "{\"tile_seed\":42,\"sea_level\":0,\"terrain\":{\"origin\":[0,0],\"spacing\":100,\"width\":21,\"height\":21,\"heights\":[" + heights
                + "]},\"rivers\":" + (rivers ? "[{\"points\":[[1000,1600],[1000,0]],\"widths\":[3,8]}]" : "[]") + "}";
        }
    }
}
