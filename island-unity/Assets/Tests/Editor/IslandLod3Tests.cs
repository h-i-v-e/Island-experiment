using Motu.Islands;
using Motu.Interop;
using Motu.Settings;
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
            runtime.SetViewPosition(centre + Vector3.right * 2000);
            Assert.IsFalse(runtime.IsLod3Visible, "Exactly 2 km retains the detailed island.");
            Assert.IsTrue(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            runtime.SetViewPosition(centre + Vector3.right * 2000.1f);
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
                new Vector2(4500, 1), new Vector2(4000, 1), new Vector2(3500, .84375f),
                new Vector2(3000, .5f), new Vector2(2500, .15625f), new Vector2(3000, .5f)
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
            runtime.SetViewPosition(centre + Vector3.right * 1999);
            Assert.IsFalse(runtime.IsLod3Visible);
            Assert.IsTrue(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            Assert.IsTrue(runtime.Caves.gameObject.activeInHierarchy);
            Assert.That(silhouette.position, Is.EqualTo(centre), "The rise must finish before tiled LODs return.");
            runtime.SetViewPosition(centre + Vector3.up * 2001);
            Assert.IsTrue(runtime.IsLod3Visible, "Distance is measured to the island centre in world space.");
            runtime.transform.position += new Vector3(8000, 25, -7000);
            runtime.SetViewPosition(runtime.transform.position + Vector3.forward * 3000);
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
                foreach (var lightening in new[] { .08f, 0f, .2f, 1f })
                {
                    settings.SkyHorizonLightening = lightening;
                    environment.SetFirstPersonViewActive(true);
                    Assert.That(RenderSettings.fogColor, Is.EqualTo(originalHaze));
                    Assert.That(Shader.GetGlobalColor("_MotuIslandHorizonColour"), Is.EqualTo(originalHaze));
                    Assert.That(environment.SkyMaterial.GetColor("_ZenithColor"), Is.EqualTo(originalZenith));
                    var skyHorizon = environment.SkyMaterial.GetColor("_HorizonColor")
                        * environment.SkyMaterial.GetFloat("_SkyExposure");
                    var expected = originalHaze * (1f + lightening);
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
