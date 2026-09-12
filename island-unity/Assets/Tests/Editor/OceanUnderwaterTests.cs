using System.IO;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanUnderwaterTests
    {
        private const int Size = 128;

        [TestCase(false, 0f)]
        [TestCase(false, 30f)]
        [TestCase(true, 25f)]
        public void PartialSubmersionOnlyTintsWetPixels(bool orthographic, float roll)
        {
            using var scene = new ViewFixture();
            scene.Camera.orthographic = orthographic;
            scene.Camera.orthographicSize = 2;
            scene.Camera.transform.rotation = Quaternion.Euler(0, 0, roll);
            var dry = scene.Render(false);
            var wet = scene.Render(true);
            scene.Save($"partial-{orthographic}-{roll}.png");
            // Exclude the one-pixel antialiasing band in world units.
            var margin = orthographic ? scene.Camera.orthographicSize * 4 / Size : .03f;
            var dryCount = 0;
            var wetCount = 0;
            for (var y = 8; y < Size - 8; y++)
            for (var x = 8; x < Size - 8; x++)
            {
                var world = scene.Camera.ViewportToWorldPoint(new Vector3((x + .5f) / Size,
                    (y + .5f) / Size, scene.Camera.nearClipPlane));
                var index = y * Size + x;
                if (world.y > margin)
                {
                    Assert.That(Difference(wet[index], dry[index]), Is.LessThan(.01f),
                        $"Above-water pixel ({x},{y}) must remain clear.");
                    dryCount++;
                }
                if (world.y < -margin)
                {
                    Assert.That(Difference(wet[index], dry[index]), Is.GreaterThan(.08f),
                        $"Submerged pixel ({x},{y}) must receive underwater optics.");
                    wetCount++;
                }
            }
            Assert.That(dryCount, Is.GreaterThan(2000));
            Assert.That(wetCount, Is.GreaterThan(2000));
            scene.Save($"partial-{orthographic}-{roll}.png");
        }

        [Test]
        public void FullySubmergedViewStopsAbsorptionAtTheSurfaceAndForeground()
        {
            using var scene = new ViewFixture();
            scene.Camera.transform.position = Vector3.down * 2;
            scene.Camera.transform.rotation = Quaternion.Euler(-75, 0, 0);
            var throughSurface = scene.Render(true)[64 * Size + 64];
            scene.Save("underwater-looking-up.png");
            scene.Camera.transform.rotation = Quaternion.Euler(75, 0, 0);
            var deepWater = scene.Render(true)[64 * Size + 64];
            scene.Save("underwater-looking-down.png");
            Assert.That(throughSurface.grayscale, Is.GreaterThan(deepWater.grayscale + .08f),
                "Sky absorption must stop at the water-to-air exit, not the far clip plane.");
            scene.Camera.transform.rotation = Quaternion.identity;
            var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
            var solid = new Material(Shader.Find("Standard"));
            try
            {
                solid.color = Color.white;
                solid.EnableKeyword("_EMISSION");
                solid.SetColor("_EmissionColor", Color.white);
                cube.GetComponent<Renderer>().sharedMaterial = solid;
                cube.transform.position = new Vector3(0, -2, 1);
                cube.transform.localScale = new Vector3(4, 4, .1f);
                var near = scene.Render(true)[64 * Size + 64];
                cube.transform.position = new Vector3(0, -2, 30);
                cube.transform.localScale = new Vector3(80, 80, .1f);
                var far = scene.Render(true)[64 * Size + 64];
                Assert.That(near.r, Is.GreaterThan(far.r + .15f), "Nearby opaque geometry must shorten the water path.");
            }
            finally { Object.DestroyImmediate(cube); Object.DestroyImmediate(solid); }
        }

        [Test]
        public void SurfaceExitDepthStaysAlignedWithARolledUnderwaterCamera()
        {
            using var scene = new ViewFixture();
            scene.Camera.transform.SetPositionAndRotation(Vector3.down * 2, Quaternion.Euler(0, 0, 30));
            // Constant incoming radiance isolates the interface mask from surface shading.
            scene.Ocean.SurfaceTransform.GetComponent<Renderer>().enabled = false;
            var pixels = scene.Render(true);
            var exiting = 0f;
            var deep = 0f;
            var exitCount = 0;
            var deepCount = 0;
            for (var y = 8; y < Size - 8; y++)
            for (var x = 8; x < Size - 8; x++)
            {
                var ray = scene.Camera.ViewportPointToRay(new Vector3((x + .5f) / Size, (y + .5f) / Size, 0));
                if (ray.direction.y > .2f) { exiting += pixels[y * Size + x].g; exitCount++; }
                if (ray.direction.y < -.2f) { deep += pixels[y * Size + x].g; deepCount++; }
            }
            Assert.That(exitCount, Is.GreaterThan(1000));
            Assert.That(deepCount, Is.GreaterThan(1000));
            Assert.That(exiting / exitCount, Is.GreaterThan(deep / deepCount + .15f),
                "Surface exit depth must line up with upward rays, including camera roll.");
            scene.Save("rolled-exit-depth.png");
        }

        [Test]
        public void WaterlineFollowsAnimatedWavesAndAboveWaterIsUnchanged()
        {
            using var scene = new ViewFixture();
            scene.Camera.backgroundColor = new Color(.8f, .4f, .2f, 1);
            scene.Camera.transform.position = Vector3.up * 3;
            var before = scene.Render(false);
            var above = scene.Render(true);
            Assert.That(MeanDifference(before, above), Is.LessThan(.001f));
            scene.Camera.transform.position = Vector3.zero;
            scene.Ocean.SurfaceMaterial.SetFloat("_GeometricWaves", 1);
            scene.Ocean.SurfaceMaterial.SetVector("_OceanWave0", new Vector4(1, 0, 16, .4f));
            scene.Ocean.SurfaceMaterial.SetVector("_OceanWave1", Vector4.zero);
            scene.Ocean.SurfaceMaterial.SetVector("_OceanWave2", Vector4.zero);
            scene.Ocean.SurfaceMaterial.SetVector("_OceanWave3", Vector4.zero);
            scene.Ocean.SurfaceMaterial.SetFloat("_OnshoreWaveEnabled", 0);
            scene.Ocean.SurfaceMaterial.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 16, Mathf.PI * .5f));
            var crest = scene.Render(true);
            scene.Save("wave-crest.png");
            scene.Ocean.SurfaceMaterial.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 16, -Mathf.PI * .5f));
            var trough = scene.Render(true);
            scene.Save("wave-trough.png");
            Assert.That(MeanDifference(crest, trough), Is.GreaterThan(.12f));
        }

        private static float Difference(Color a, Color b) => Mathf.Max(Mathf.Abs(a.r-b.r), Mathf.Abs(a.g-b.g), Mathf.Abs(a.b-b.b));
        private static float MeanDifference(Color[] a, Color[] b)
        {
            var sum = 0f;
            for (var i = 0; i < a.Length; i++) sum += Difference(a[i], b[i]);
            return sum / a.Length;
        }

        private sealed class ViewFixture : System.IDisposable
        {
            internal readonly Camera Camera;
            internal readonly OceanSurfaceController Ocean;
            private readonly OceanUnderwaterView effect;
            private readonly GameObject oceanHost, cameraHost;
            private readonly RenderTexture target;
            private readonly Texture2D image;
            private readonly Vector4 oldWind;
            private readonly float oldCloud, oldShips;
            private readonly int oldCapsules;
            private readonly bool oldFog;
            private readonly Color oldAmbient;
            private readonly Light oldSun;

            internal ViewFixture()
            {
                EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
                oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
                oldCloud = Shader.GetGlobalFloat("_MotuCloudEnabled");
                oldShips = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
                oldCapsules = Shader.GetGlobalInt("_MotuDeckCapsuleCount");
                oldFog = RenderSettings.fog;
                oldAmbient = RenderSettings.ambientLight;
                oldSun = RenderSettings.sun;
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", 0);
                RenderSettings.fog = false;
                RenderSettings.ambientLight = Color.white;
                RenderSettings.sun = null;
                oceanHost = new GameObject("Underwater test ocean");
                Ocean = oceanHost.AddComponent<OceanSurfaceController>();
                var material = new Material(Shader.Find("Motu/Sea Water"));
                Ocean.Install(material, 256, true);
                material.SetFloat("_GeometricWaves", 0);
                material.SetFloat("_RippleStrength", 0);
                material.SetFloat("_WhitecapStrength", 0);
                material.SetFloat("_PersistentFoamStrength", 0);
                cameraHost = new GameObject("Underwater test camera");
                Camera = cameraHost.AddComponent<Camera>();
                Camera.enabled = false;
                Camera.clearFlags = CameraClearFlags.SolidColor;
                Camera.backgroundColor = Color.white;
                Camera.nearClipPlane = .3f;
                Camera.farClipPlane = 100;
                Camera.fieldOfView = 60;
                Camera.allowMSAA = false;
                Camera.allowHDR = false;
                effect = cameraHost.AddComponent<OceanUnderwaterView>();
                effect.Configure(Ocean);
                target = new RenderTexture(Size, Size, 24, RenderTextureFormat.ARGB32);
                image = new Texture2D(Size, Size, TextureFormat.RGBA32, false);
                Camera.targetTexture = target;
            }
            internal Color[] Render(bool underwater)
            {
                effect.enabled = underwater;
                Camera.Render();
                var previous = RenderTexture.active;
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, Size, Size), 0, 0);
                image.Apply();
                RenderTexture.active = previous;
                Assert.IsFalse(ShaderUtil.ShaderHasError(Resources.Load<Shader>("OceanUnderwater")));
                Assert.IsFalse(ShaderUtil.ShaderHasError(Ocean.SurfaceMaterial.shader));
                return image.GetPixels();
            }
            internal void Save(string name)
            {
                var directory = Path.Combine(Path.GetTempPath(), "motu-underwater-validation");
                Directory.CreateDirectory(directory);
                File.WriteAllBytes(Path.Combine(directory, name), image.EncodeToPNG());
            }
            public void Dispose()
            {
                Object.DestroyImmediate(cameraHost);
                Object.DestroyImmediate(oceanHost);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(image);
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalFloat("_MotuCloudEnabled", oldCloud);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", oldShips);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", oldCapsules);
                RenderSettings.fog = oldFog;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.sun = oldSun;
            }
        }
    }
}
