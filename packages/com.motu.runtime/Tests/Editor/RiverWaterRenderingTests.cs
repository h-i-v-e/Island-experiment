using System;
using System.IO;
using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class RiverWaterRenderingTests
    {
        [TestCase(-1)]
        [TestCase(1)]
        public void OverlappingWaterfallSheetsKeepTheNearestSurfaceInEitherTriangleOrder(int side)
        {
            using var fixture = new Fixture();
            fixture.LookFrom(side, false);
            var first = fixture.Capture($"overlap-{side}-first");
            fixture.ReverseSheets();
            var reversed = fixture.Capture($"overlap-{side}-reversed");
            Assert.That(Difference(first, reversed), Is.LessThan(.001f),
                "The nearest refractive sheet must win from both sides, independently of index order.");
            fixture.Water.SetActive(false);
            Assert.That(Difference(first, fixture.Capture()), Is.GreaterThan(.01f),
                "Both sides of the waterfall must actually render.");
            Assert.That(ShaderUtil.ShaderHasError(fixture.Material.shader), Is.False);
        }

        [TestCase(-1, false, false)]
        [TestCase(1, false, true)]
        [TestCase(1, true, false)]
        public void RearFoamVeilOnlyChangesTheBackOfSteepFalls(int side, bool horizontal, bool changes)
        {
            using var fixture = new Fixture();
            fixture.LookFrom(side, horizontal);
            fixture.Material.SetFloat("_WaterfallBackFoam", 0);
            var clear = fixture.Capture($"veil-{side}-{horizontal}-off");
            fixture.Material.SetFloat("_WaterfallBackFoam", .6f);
            var veiled = fixture.Capture($"veil-{side}-{horizontal}-on");
            var difference = Difference(clear, veiled);
            if (changes)
                Assert.That(difference, Is.GreaterThan(.025f), "Rear foam must visibly disguise the thin sheet.");
            else
                Assert.That(difference, Is.LessThan(.001f), "The front and calm river keep their existing optics.");
        }

        [Test]
        public void DepthWritingWaterStillRefractsTheOpaqueBackground()
        {
            using var fixture = new Fixture();
            fixture.LookFrom(1, true);
            fixture.Material.SetFloat("_WhitewaterStrength", 0);
            fixture.Material.SetFloat("_AbsorptionStrength", 0);
            fixture.Material.SetFloat("_RefractionStrength", 0);
            fixture.Material.SetFloat("_SunGlintStrength", 0);
            fixture.SetBackground(new Color(.2f, .04f, .01f));
            var red = fixture.Capture();
            fixture.SetBackground(new Color(.01f, .04f, .2f));
            Assert.That(Difference(red, fixture.Capture()), Is.GreaterThan(.03f),
                "Depth writes must preserve the opaque scene seen through calm water.");
        }

        private static float Difference(Color[] first, Color[] second)
        {
            float sum = 0;
            for (var index = 0; index < first.Length; index++)
                sum += Mathf.Abs(first[index].r - second[index].r)
                    + Mathf.Abs(first[index].g - second[index].g)
                    + Mathf.Abs(first[index].b - second[index].b);
            return sum / (first.Length * 3);
        }

        private sealed class Fixture : IDisposable
        {
            private const int Size = 512;
            private readonly RenderPipelineAsset oldPipeline = GraphicsSettings.defaultRenderPipeline;
            private readonly RenderPipelineAsset oldQuality = QualitySettings.renderPipeline;
            private readonly AmbientMode oldAmbientMode = RenderSettings.ambientMode;
            private readonly Color oldAmbient = RenderSettings.ambientLight;
            private readonly SphericalHarmonicsL2 oldProbe = RenderSettings.ambientProbe;
            private readonly Light oldSun = RenderSettings.sun;
            private readonly bool oldFog = RenderSettings.fog;
            private readonly float oldCloud = Shader.GetGlobalFloat("_MotuCloudEnabled");
            private readonly float oldReflection = Shader.GetGlobalFloat("_PlanarReflectionAvailable");
            private readonly UniversalRendererData renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
            private readonly UniversalRenderPipelineAsset pipeline;
            private readonly GameObject root = new GameObject("Waterfall rear render fixture");
            private readonly RenderTexture target = new RenderTexture(Size, Size, 24);
            private readonly Texture2D image = new Texture2D(Size, Size, TextureFormat.RGBA32, false);
            private readonly Texture2D noise = ProceduralNoiseTextures.CreateRiverNoiseTexture();
            private readonly Material backdropMaterial = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
            private readonly Mesh mesh;
            private readonly Camera camera;
            private readonly GameObject backdrop;
            internal readonly GameObject Water;
            internal readonly Material Material = new Material(Shader.Find("Motu/River Water"));

            internal Fixture()
            {
                renderer.copyDepthMode = CopyDepthMode.AfterOpaques;
                pipeline = UniversalRenderPipelineAsset.Create(renderer);
                pipeline.supportsCameraDepthTexture = true;
                pipeline.supportsCameraOpaqueTexture = true;
                pipeline.msaaSampleCount = 1;
                GraphicsSettings.defaultRenderPipeline = pipeline;
                QualitySettings.renderPipeline = pipeline;
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_PlanarReflectionAvailable", 0);
                RenderSettings.fog = false;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = Color.gray * .5f;
                var probe = new SphericalHarmonicsL2();
                probe.AddAmbientLight(Color.gray * .5f);
                RenderSettings.ambientProbe = probe;
                ConfigureSun();
                camera = Child("Camera").AddComponent<Camera>();
                ConfigureCamera();
                backdrop = GameObject.CreatePrimitive(PrimitiveType.Cube);
                backdrop.transform.SetParent(root.transform);
                backdrop.layer = 31;
                backdropMaterial.SetColor("_BaseColor", new Color(.06f, .11f, .22f));
                backdrop.GetComponent<Renderer>().sharedMaterial = backdropMaterial;
                Water = Child("Two overlapping waterfall sheets");
                mesh = Sheets();
                Water.AddComponent<MeshFilter>().sharedMesh = mesh;
                Water.AddComponent<MeshRenderer>().sharedMaterial = Material;
                Water.transform.position = new Vector3(0, 3, 0);
                ConfigureWater();
            }

            private GameObject Child(string name)
            {
                var item = new GameObject(name) { layer = 31 };
                item.transform.SetParent(root.transform);
                return item;
            }

            private void ConfigureSun()
            {
                var sun = Child("Sun").AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 1.2f;
                sun.transform.rotation = Quaternion.Euler(35, 25, 0);
                RenderSettings.sun = sun;
            }

            private void ConfigureCamera()
            {
                camera.enabled = false;
                camera.orthographic = true;
                camera.orthographicSize = 2.5f;
                camera.nearClipPlane = .1f;
                camera.farClipPlane = 30;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = new Color(.1f, .15f, .2f);
                camera.allowHDR = false;
                camera.allowMSAA = false;
                var data = camera.GetUniversalAdditionalCameraData();
                data.requiresColorTexture = true;
                data.requiresDepthTexture = true;
                data.renderPostProcessing = false;
            }

            private void ConfigureWater()
            {
                Material.SetTexture("_NoiseTex", noise);
                Material.SetFloat("_WorldSize", 1);
                Material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                Material.SetFloat("_CoarseNoiseWorldSize", 1.2f);
                Material.SetFloat("_FineNoiseWorldSize", .65f);
                // Freeze animation so consecutive renders isolate depth order
                // and the rear veil rather than advancing the foam pattern.
                Material.SetFloat("_CoarseFlowSpeed", 0);
                Material.SetFloat("_FineFlowSpeed", 0);
                Material.SetFloat("_WaterfallFlowSpeed", 0);
                Material.SetFloat("_FoamWorldSize", .7f);
                Material.SetFloat("_FoamStretch", 3);
                Material.SetFloat("_SurfaceRoughness", .3f);
                Material.SetFloat("_AbsorptionStrength", .15f);
                Material.SetVector("_AbsorptionCoefficients", new Vector4(.3f, .1f, .04f, 0));
            }

            internal void LookFrom(int side, bool horizontal)
            {
                var centre = Water.transform.position;
                var direction = horizontal ? Vector3.up : Vector3.forward * side;
                Water.transform.rotation = horizontal ? Quaternion.Euler(90, 0, 0) : Quaternion.identity;
                camera.transform.position = centre + direction * 8;
                camera.transform.LookAt(centre, horizontal ? Vector3.forward : Vector3.up);
                backdrop.transform.position = centre - direction * 3;
                backdrop.transform.localScale = horizontal ? new Vector3(20, .1f, 20) : new Vector3(20, 20, .1f);
            }

            private static Mesh Sheets()
            {
                var positions = new Vector3[8];
                var uv = new Vector2[8];
                for (var sheet = 0; sheet < 2; sheet++)
                {
                    var offset = sheet * 4;
                    var depth = sheet == 0 ? -.2f : .2f;
                    positions[offset] = new Vector3(-2, -2, depth);
                    positions[offset + 1] = new Vector3(2, -2, depth);
                    positions[offset + 2] = new Vector3(2, 2, depth);
                    positions[offset + 3] = new Vector3(-2, 2, depth);
                    uv[offset] = new Vector2(0, sheet * 7.3f);
                    uv[offset + 1] = uv[offset] + new Vector2(4, 0);
                    uv[offset + 2] = uv[offset] + new Vector2(4, 4);
                    uv[offset + 3] = uv[offset] + new Vector2(0, 4);
                }
                var result = new Mesh { vertices = positions, uv = uv,
                    triangles = new[] { 0, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6 } };
                result.RecalculateNormals();
                result.RecalculateBounds();
                return result;
            }

            internal void ReverseSheets() => mesh.triangles = new[] { 4, 6, 5, 4, 7, 6, 0, 2, 1, 0, 3, 2 };

            internal void SetBackground(Color colour) => backdropMaterial.SetColor("_BaseColor", colour);

            internal Color[] Capture(string name = null)
            {
                var previous = RenderTexture.active;
                try
                {
                    RenderPipeline.SubmitRenderRequest(camera,
                        new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, Size, Size), 0, 0);
                    image.Apply();
                    var directory = Environment.GetEnvironmentVariable("MOTU_WATERFALL_IMAGES");
                    if (name != null && !string.IsNullOrEmpty(directory))
                    {
                        Directory.CreateDirectory(directory);
                        File.WriteAllBytes(Path.Combine(directory, name + ".png"), image.EncodeToPNG());
                    }
                    return image.GetPixels();
                }
                finally { RenderTexture.active = previous; }
            }

            public void Dispose()
            {
                GraphicsSettings.defaultRenderPipeline = oldPipeline;
                QualitySettings.renderPipeline = oldQuality;
                RenderSettings.ambientMode = oldAmbientMode;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.ambientProbe = oldProbe;
                RenderSettings.sun = oldSun;
                RenderSettings.fog = oldFog;
                Shader.SetGlobalFloat("_MotuCloudEnabled", oldCloud);
                Shader.SetGlobalFloat("_PlanarReflectionAvailable", oldReflection);
                foreach (var item in new Object[] { root, target, image, mesh, Material,
                    noise, backdropMaterial, pipeline, renderer }) Object.DestroyImmediate(item);
            }
        }
    }
}
