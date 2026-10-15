using Motu.Islands;
using Motu.Interop;
using Motu.Settings;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class IslandLod3Tests
    {
        [Test]
        public void EmptyHorizonExportInstallsWithoutARenderer()
        {
            var prepared = IslandMeshInterop.CopyOptionalGeneratedMeshData(default, 4000f);
            Assert.IsNull(prepared);
            var parent = new GameObject("Submerged island validation");
            try
            {
                var runtime = IslandRuntime.Create(IslandDescriptor.Request(17, Vector2Int.zero, null), parent.transform);
                runtime.InstallLod3(prepared, 80f);
                runtime.SetViewPosition(Vector3.right * 4000f);
                runtime.SetViewPosition(Vector3.right * 1000f);
                Assert.IsFalse(runtime.IsLod3Visible);
                Assert.That(runtime.Lod3TriangleCount, Is.Zero);
                Assert.IsEmpty(runtime.GetComponentsInChildren<MeshRenderer>(true));
                runtime.Dispose();
                runtime.Dispose();
            }
            finally { Object.DestroyImmediate(parent); }
        }

        [Test]
        public void OptionalHorizonStillRejectsInvalidAndRequiredEmptyMeshes()
        {
            var invalid = new MotuNative.ExportMesh();
            invalid.triangles.length = 3;
            Assert.Throws<System.InvalidOperationException>(() =>
                IslandMeshInterop.CopyOptionalGeneratedMeshData(invalid, 4000f));
            Assert.Throws<System.InvalidOperationException>(() =>
                IslandMeshInterop.CopyGeneratedMeshData(default, 4000f));
        }

        internal static void ValidateInstalledLod3(IslandRuntime runtime, WorldEnvironmentController environment,
            float maximumHeightMetres)
        {
            Debug.Log($"Island LOD3: {runtime.Lod3TriangleCount} triangles; tiled LOD2: {runtime.TerrainStreamer.BaseTriangleCount} triangles.");
            Assert.That(runtime.Lod3TriangleCount, Is.GreaterThan(0));
            Assert.That(runtime.Lod3TriangleCount, Is.LessThan(runtime.TerrainStreamer.BaseTriangleCount));
            var centre = runtime.transform.position;
            runtime.SetViewPosition(centre + Vector3.right * 1000);
            Assert.IsFalse(runtime.IsLod3Visible, "Exactly 1 km retains the detailed island.");
            Assert.IsTrue(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            runtime.SetViewPosition(centre + Vector3.right * 1000.1f);
            Assert.IsTrue(runtime.IsLod3Visible);
            Assert.IsFalse(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            Assert.IsFalse(runtime.Caves.gameObject.activeInHierarchy);
            var renderers = runtime.GetComponentsInChildren<MeshRenderer>();
            Assert.That(renderers.Length, Is.EqualTo(1), "LOD3 must replace every detailed renderer with one unsliced draw.");
            Assert.That(renderers[0].sharedMaterial.shader.name, Is.EqualTo("Motu/Island Horizon"));
            Assert.IsFalse(renderers[0].receiveShadows);
            Assert.IsNull(renderers[0].GetComponent<Collider>());
            var mesh = renderers[0].GetComponent<MeshFilter>().sharedMesh;
            Assert.That(mesh.subMeshCount, Is.EqualTo(1));
            Assert.IsFalse(mesh.HasVertexAttribute(UnityEngine.Rendering.VertexAttribute.Normal));
            var silhouette = renderers[0].transform;
            // Visit several positions without changing LOD, then reverse direction.
            foreach (var sample in new[]
            {
                new Vector2(4500, 1), new Vector2(4000, 1), new Vector2(3250, .84375f),
                new Vector2(2500, .5f), new Vector2(1750, .15625f), new Vector2(2500, .5f)
            })
            {
                runtime.SetViewPosition(centre + Vector3.right * sample.x);
                Assert.IsTrue(runtime.IsLod3Visible);
                Assert.That(silhouette.position.y,
                    Is.EqualTo(centre.y - maximumHeightMetres * sample.y).Within(.001f));
                Assert.That(runtime.transform.position, Is.EqualTo(centre), "The actual island centre must not move.");
            }
            runtime.SetDormant(true);
            Assert.IsTrue(runtime.gameObject.activeInHierarchy);
            Assert.IsTrue(runtime.IsLod3Visible, "Dormant resident islands must retain their silhouettes.");
            runtime.SetViewPosition(centre + Vector3.right * 4000);
            Assert.That(silhouette.position.y, Is.EqualTo(centre.y - maximumHeightMetres).Within(.001f));
            runtime.SetDormant(false);
            Assert.IsTrue(runtime.IsLod3Visible, "Waking a distant island must not briefly reveal LOD2.");
            runtime.SetViewPosition(centre + Vector3.right * 999);
            Assert.IsFalse(runtime.IsLod3Visible);
            Assert.IsTrue(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            Assert.IsTrue(runtime.Caves.gameObject.activeInHierarchy);
            Assert.That(silhouette.position, Is.EqualTo(centre), "The rise must finish before tiled LODs return.");
            runtime.SetViewPosition(centre + Vector3.up * 1001);
            Assert.IsTrue(runtime.IsLod3Visible, "Distance is measured to the island centre in world space.");
            runtime.transform.position += new Vector3(8000, 25, -7000);
            runtime.SetViewPosition(runtime.transform.position + Vector3.forward * 2500);
            Assert.That(silhouette.position.y,
                Is.EqualTo(runtime.transform.position.y - maximumHeightMetres * .5f).Within(.001f));
            runtime.SetViewPosition(runtime.transform.position + Vector3.forward * 100);
            Assert.IsFalse(runtime.IsLod3Visible, "Translated islands must measure from their own centre.");
            runtime.transform.position = centre;
            runtime.SetViewPosition(centre);
            Assert.That(environment.GetComponent<OceanSurfaceController>().CoastalWaveBindingCount, Is.EqualTo(1));
        }

        [TestCase(0f)]
        [TestCase(12f)]
        public void SkyHorizonLighteningKeepsHazeAndDistantIslandsMatched(float solarHour)
        {
            var host = new GameObject("Horizon colour validation");
            var oldFog = RenderSettings.fog;
            var oldFogColour = RenderSettings.fogColor;
            var oldIslandColour = Shader.GetGlobalColor("_MotuIslandHorizonColour");
            try
            {
                var settings = new WorldEnvironmentSettings();
                Assert.That(settings.SkyHorizonLightening, Is.EqualTo(.08f));
                JsonUtility.FromJsonOverwrite($"{{\"startingSolarTimeHours\":{solarHour}}}", settings);
                var environment = host.AddComponent<WorldEnvironmentController>();
                environment.Initialize(settings, new IslandCloudSettings(), 64f, 128f, null);
                environment.SetFirstPersonViewActive(true);
                var originalHaze = RenderSettings.fogColor;
                var originalZenith = environment.SkyMaterial.GetColor("_ZenithColor");
                var originalSkyHorizon = environment.SkyMaterial.GetColor("_HorizonColor");
                foreach (var lightening in new[] { .08f, 0f, .2f, 1f })
                {
                    settings.SkyHorizonLightening = lightening;
                    environment.SetFirstPersonViewActive(true);
                    Assert.That(RenderSettings.fogColor, Is.EqualTo(originalHaze));
                    var shaderHaze = QualitySettings.activeColorSpace == ColorSpace.Linear
                        ? originalHaze.linear : originalHaze;
                    var islandColour = Shader.GetGlobalColor("_MotuIslandHorizonColour");
                    Assert.That(islandColour.r, Is.EqualTo(shaderHaze.r).Within(.00001f));
                    Assert.That(islandColour.g, Is.EqualTo(shaderHaze.g).Within(.00001f));
                    Assert.That(islandColour.b, Is.EqualTo(shaderHaze.b).Within(.00001f));
                    Assert.That(environment.SkyMaterial.GetColor("_ZenithColor"), Is.EqualTo(originalZenith));
                    var skyHorizon = environment.SkyMaterial.GetColor("_HorizonColor");
                    Assert.That(environment.SeaMaterial.GetColor("_ReflectionHorizonColor"), Is.EqualTo(skyHorizon),
                        "The fallback sea reflection must follow the current sky horizon.");
                    var expected = originalSkyHorizon * ((1f + lightening) / 1.08f);
                    Assert.That(skyHorizon.r, Is.EqualTo(expected.r).Within(.00001f));
                    Assert.That(skyHorizon.g, Is.EqualTo(expected.g).Within(.00001f));
                    Assert.That(skyHorizon.b, Is.EqualTo(expected.b).Within(.00001f));
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(environment.SkyMaterial.shader));
            }
            finally
            {
                Object.DestroyImmediate(host);
                RenderSettings.fog = oldFog;
                RenderSettings.fogColor = oldFogColour;
                Shader.SetGlobalColor("_MotuIslandHorizonColour", oldIslandColour);
            }
        }

        [TestCase(0f)]
        [TestCase(12f)]
        public void HorizonCameraMatchesSkyColourSpaceAndExposure(float solarHour)
        {
            var root = new GameObject("Horizon colour-space validation");
            var material = new Material(Resources.Load<Shader>("IslandHorizon"));
            var target = new RenderTexture(64, 64, 24, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var image = new Texture2D(64, 64, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            var oldColour = Shader.GetGlobalColor("_MotuIslandHorizonColour");
            try
            {
                var quad = GameObject.CreatePrimitive(PrimitiveType.Quad);
                quad.transform.SetParent(root.transform);
                quad.transform.position = Vector3.forward * 4;
                quad.transform.localScale = Vector3.one * 4;
                quad.layer = 31;
                quad.GetComponent<Renderer>().sharedMaterial = material;
                var cameraObject = new GameObject("Horizon camera");
                cameraObject.transform.SetParent(root.transform);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.magenta;
                camera.targetTexture = target;
                camera.allowHDR = true;
                camera.allowMSAA = false;
                var settings = new WorldEnvironmentSettings();
                settings.SkyHorizonLightening = 0f;
                JsonUtility.FromJsonOverwrite($"{{\"startingSolarTimeHours\":{solarHour}}}", settings);
                var environment = root.AddComponent<WorldEnvironmentController>();
                environment.Initialize(settings, new IslandCloudSettings(), 64f, 128f, null);
                environment.SetFirstPersonViewActive(true);
                var horizon = environment.SkyMaterial.GetColor("_HorizonColor");
                var colour = (QualitySettings.activeColorSpace == ColorSpace.Linear ? horizon.linear : horizon)
                    * environment.SkyExposure;
                colour.a = 1f;
                foreach (var lightening in new[] { 0f, .2f })
                {
                    settings.SkyHorizonLightening = lightening;
                    environment.SetFirstPersonViewActive(true);
                    UrpTestCamera.Render(camera);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 64, 64), 0, 0); image.Apply();
                    var pixel = image.GetPixel(32, 32);
                    Assert.That(pixel.r, Is.EqualTo(colour.r).Within(.0001));
                    Assert.That(pixel.g, Is.EqualTo(colour.g).Within(.0001));
                    Assert.That(pixel.b, Is.EqualTo(colour.b).Within(.0001));
                    Assert.That(pixel.a, Is.EqualTo(1f));
                    var sky = environment.SkyMaterial.GetColor("_HorizonColor");
                    sky = (QualitySettings.activeColorSpace == ColorSpace.Linear ? sky.linear : sky)
                        * environment.SkyExposure;
                    if (lightening > 0f)
                    {
                        Assert.That(pixel.r, Is.LessThan(sky.r));
                        Assert.That(pixel.g, Is.LessThan(sky.g));
                        Assert.That(pixel.b, Is.LessThan(sky.b));
                    }
                }
            }
            finally
            {
                RenderTexture.active = oldTarget;
                Shader.SetGlobalColor("_MotuIslandHorizonColour", oldColour);
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }

        [TestCase(0f)]
        [TestCase(12f)]
        public void ReflectedHorizonKeepsItsColourThroughDistanceFog(float solarHour)
        {
            var root = new GameObject("Reflected horizon colour validation");
            var material = new Material(Resources.Load<Shader>("IslandHorizon"));
            var target = new RenderTexture(128, 128, 24, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var image = new Texture2D(64, 64, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            var oldFog = RenderSettings.fog;
            var oldDensity = RenderSettings.fogDensity;
            try
            {
                var settings = new WorldEnvironmentSettings();
                settings.SkyHorizonLightening = 0f;
                JsonUtility.FromJsonOverwrite($"{{\"startingSolarTimeHours\":{solarHour}}}", settings);
                var environment = root.AddComponent<WorldEnvironmentController>();
                environment.Initialize(settings, new IslandCloudSettings(), 64f, 128f, null);
                environment.SetFirstPersonViewActive(true);
                var expected = Shader.GetGlobalColor("_MotuIslandHorizonColour");
                var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
                cube.transform.SetParent(root.transform);
                cube.transform.position = new Vector3(0, 2, 0);
                cube.layer = 31;
                cube.GetComponent<Renderer>().sharedMaterial = material;
                var plane = new GameObject("Reflection plane");
                plane.transform.SetParent(root.transform);
                var cameraObject = new GameObject("Horizon reflection camera");
                cameraObject.transform.SetParent(root.transform);
                cameraObject.transform.SetPositionAndRotation(new Vector3(0, 2, -10), Quaternion.Euler(10, 0, 0));
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.targetTexture = target;
                camera.allowHDR = true;
                camera.allowMSAA = false;
                var reflection = cameraObject.AddComponent<PlanarWaterReflection>();
                reflection.Configure(plane.transform);
                var serialized = new SerializedObject(reflection);
                serialized.FindProperty("frameInterval").intValue = 1;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                Color[] Capture()
                {
                    reflection.PrepareReflection();
                    RenderTexture.active = reflection.ReflectionCamera.targetTexture;
                    image.ReadPixels(new Rect(0, 0, 64, 64), 0, 0); image.Apply();
                    return image.GetPixels();
                }
                RenderSettings.fog = false;
                var clear = Capture();
                var index = System.Array.FindIndex(clear, pixel => pixel.r > expected.r * .9f);
                Assert.That(index, Is.GreaterThanOrEqualTo(0), "The reflected horizon must be visible.");
                RenderSettings.fog = true;
                RenderSettings.fogDensity = .5f;
                var hazy = Capture()[index];
                Assert.That(hazy.r, Is.EqualTo(clear[index].r).Within(.0001), "Reflection haze must match the island colour.");
                Assert.That(hazy.g, Is.EqualTo(clear[index].g).Within(.0001));
                Assert.That(hazy.b, Is.EqualTo(clear[index].b).Within(.0001));
            }
            finally
            {
                RenderTexture.active = oldTarget;
                Object.DestroyImmediate(root);
                RenderSettings.fog = oldFog;
                RenderSettings.fogDensity = oldDensity;
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void HorizonShaderRendersOnlyTheCurrentFlatColour()
        {
            var shader = Resources.Load<Shader>("IslandHorizon");
            Assert.IsNotNull(shader);
            var material = new Material(shader);
            var target = new RenderTexture(32, 32, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var image = new Texture2D(32, 32, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            var oldColour = Shader.GetGlobalColor("_MotuIslandHorizonColour");
            var oldFog = RenderSettings.fog;
            var oldAmbient = RenderSettings.ambientLight;
            try
            {
                foreach (var colour in new[] { new Color(.12f, .35f, .6f, 1), new Color(.015f, .02f, .04f, 1) })
                {
                    Shader.SetGlobalColor("_MotuIslandHorizonColour", colour);
                    RenderSettings.fog = true;
                    RenderSettings.ambientLight = Color.red;
                    Graphics.Blit(Texture2D.whiteTexture, target, material);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 32, 32), 0, 0); image.Apply();
                    foreach (var pixel in image.GetPixels())
                    {
                        Assert.That(pixel.r, Is.EqualTo(colour.r).Within(.001));
                        Assert.That(pixel.g, Is.EqualTo(colour.g).Within(.001));
                        Assert.That(pixel.b, Is.EqualTo(colour.b).Within(.001));
                        Assert.That(pixel.a, Is.EqualTo(1));
                    }
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(shader));
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Shader.SetGlobalColor("_MotuIslandHorizonColour", oldColour);
                RenderSettings.fog = oldFog;
                RenderSettings.ambientLight = oldAmbient;
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }
    }
}
