using System;
using System.IO;
using System.Reflection;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class SquallWetnessRenderTests
    {
        [Test]
        public void ActualCameraWetsOpaqueSurfacesHoldsThroughRainAndEventuallyDries()
        {
            using var fixture = new Fixture();
            var dry = fixture.Capture("surfaces-dry");
            var effect = fixture.Camera.GetComponent<SquallSurfaceWetness>();
            Assert.That(effect, Is.Not.Null, "The weather owner must bind game cameras automatically.");
            Assert.That(effect.Wetness, Is.Zero);
            Assert.That(fixture.Environment.SpawnSquall(Vector3.zero, 500), Is.True);
            Assert.That(fixture.Environment.SquallRainIntensityAt(fixture.Camera.transform.position), Is.GreaterThan(0));
            effect.Advance(30);
            Assert.That(effect.Wetness, Is.GreaterThan(.5f));
            var controls = fixture.Environment.Weather;
            controls.Squalls.SurfaceWetness = 0;
            fixture.Environment.ApplyWeather(controls);
            var stormDry = fixture.Capture();
            controls.Squalls.SurfaceWetness = .00001f;
            fixture.Environment.ApplyWeather(controls);
            Assert.That(Difference(stormDry, fixture.Capture()), Is.LessThan(.002f), "An effectively zero-strength pass must preserve scene colour.");
            controls.Squalls.SurfaceWetness = .75f;
            fixture.Environment.ApplyWeather(controls);
            var wet = fixture.Capture("surfaces-wet");
            Assert.That(Difference(stormDry, wet), Is.GreaterThan(.003f), "The actual renderer must visibly change surface lighting.");
            var highlight = 0f;
            for (var i = 0; i < wet.Length; ++i) highlight = Mathf.Max(highlight, wet[i].grayscale - stormDry[i].grayscale);
            Assert.That(highlight, Is.GreaterThan(.01f), "Wet surfaces must gain a sheen as well as darkening.");
            var marker = fixture.Camera.WorldToViewportPoint(new Vector3(4, 1, -2));
            var markerPixel = (int)(marker.y * 256) * 256 + (int)(marker.x * 256);
            Assert.That(Vector4.Distance(stormDry[markerPixel], wet[markerPixel]), Is.LessThan(.002f),
                "Transparent-pass geometry must be composited after wetness.");
            Assert.That(Mathf.Abs(dry[250 * 256 + 8].grayscale - wet[250 * 256 + 8].grayscale), Is.LessThan(.002f), "The sky must not receive a wetness overlay.");
            var before = effect.Wetness;
            var weather = fixture.Environment.Weather;
            weather.Squalls.RainIntensity = .01f;
            fixture.Environment.ApplyWeather(weather);
            effect.Advance(60);
            Assert.That(effect.Wetness, Is.EqualTo(before), "Lighter rain must hold the existing wetness.");
            fixture.Environment.ClearSquall();
            effect.Advance(WorldSquallRain.ParticleLifetime);
            Assert.That(effect.Wetness, Is.EqualTo(before));
            effect.Advance(50);
            Assert.That(effect.Wetness, Is.Zero);
            Assert.That(Difference(dry, fixture.Capture("surfaces-dried")), Is.LessThan(.002f));
            fixture.Environment.enabled = false;
            ReleaseWorld(fixture.Environment);
            Assert.That(fixture.Camera.GetComponent<SquallSurfaceWetness>(), Is.Null, "An automatically added view must be removed with its owner.");
        }

        [Test]
        public void AuthoredCameraComponentIsRestoredInsteadOfBeingRemoved()
        {
            var priorRoot = new GameObject("Prior wetness weather source");
            try
            {
                var prior = priorRoot.AddComponent<WorldEnvironmentController>();
                using var fixture = new Fixture();
                var authored = fixture.Camera.gameObject.AddComponent<SquallSurfaceWetness>();
                authored.Configure(prior);
                fixture.Capture();
                Assert.That(authored.Environment, Is.EqualTo(fixture.Environment));
                fixture.Environment.enabled = false;
                ReleaseWorld(fixture.Environment);
                Assert.That(fixture.Camera.GetComponent<SquallSurfaceWetness>(), Is.SameAs(authored));
                Assert.That(authored.Environment, Is.EqualTo(prior));
            }
            finally { Object.DestroyImmediate(priorRoot); }
        }

        private static float Difference(Color[] first, Color[] second)
        {
            float sum = 0;
            for (var i = 0; i < first.Length; ++i) sum += Mathf.Abs(first[i].grayscale - second[i].grayscale);
            return sum / first.Length;
        }

        // Non-ExecuteAlways owners do not receive automatic lifecycle messages
        // in an EditMode render audit. Invoke the same cleanup directly.
        private static void ReleaseWorld(WorldEnvironmentController environment) =>
            typeof(WorldEnvironmentController).GetMethod("OnDisable", BindingFlags.Instance | BindingFlags.NonPublic)
                .Invoke(environment, null);

        private sealed class Fixture : IDisposable
        {
            private readonly EnvironmentHostState baseline = new EnvironmentHostState();
            private readonly GameObject root = new GameObject("Wet surface render fixture");
            private readonly Material ground = new Material(Shader.Find("Universal Render Pipeline/Lit"));
            private readonly Material wood = new Material(Shader.Find("Universal Render Pipeline/Lit"));
            private readonly Material transparent = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
            internal readonly Camera Camera;
            internal readonly WorldEnvironmentController Environment;

            internal Fixture()
            {
                Assert.That(GraphicsSettings.currentRenderPipeline, Is.InstanceOf<UniversalRenderPipelineAsset>());
                Camera = new GameObject("Wetness camera").AddComponent<Camera>(); Camera.transform.SetParent(root.transform);
                Camera.enabled = false; Camera.aspect = 1; Camera.fieldOfView = 55;
                Camera.transform.position = new Vector3(0, 6, -12); Camera.transform.LookAt(new Vector3(0, 1, 0));
                Camera.nearClipPlane = .1f; Camera.farClipPlane = 500; Camera.cullingMask = 1 << 31;
                Camera.clearFlags = CameraClearFlags.SolidColor; Camera.backgroundColor = new Color(.12f, .2f, .3f);
                Camera.allowHDR = false; Camera.allowMSAA = false;
                Camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                var sun = new GameObject("Wetness render sun").AddComponent<Light>(); sun.transform.SetParent(root.transform);
                sun.type = LightType.Directional; sun.intensity = 2.5f; sun.shadows = LightShadows.Hard;
                RenderSettings.sun = sun;
                var weatherRoot = new GameObject("Wetness weather"); weatherRoot.transform.SetParent(root.transform);
                Environment = weatherRoot.AddComponent<WorldEnvironmentController>();
                Environment.Initialize(new WorldEnvironmentSettings().WithSeed(7129), new IslandCloudSettings(), 64, 128, Camera.transform);
                Environment.SetSeaVisible(false);
                var weather = Environment.Weather;
                weather.Squalls.AutomaticSpawning = false; weather.Squalls.FadeSeconds = 0;
                weather.Squalls.RainIntensity = 4; weather.Squalls.WindMultiplier = 1;
                weather.Squalls.MinimumSpeedMultiplier = weather.Squalls.MaximumSpeedMultiplier = 8;
                weather.Squalls.LightningBrightness = 0;
                weather.WindSpeedMetresPerSecond = 0;
                Environment.ApplyWeather(weather);
                sun.transform.rotation = Quaternion.Euler(27, 180, 0);
                sun.enabled = true; sun.intensity = 2.5f; RenderSettings.sun = sun;
                if (Environment.MoonLight != null) Environment.MoonLight.enabled = false;
                RenderSettings.ambientMode = AmbientMode.Flat; RenderSettings.ambientLight = Color.gray * .35f;
                RenderSettings.fog = false;
                ground.SetColor("_BaseColor", new Color(.28f, .29f, .28f)); ground.SetFloat("_Smoothness", 0);
                wood.SetColor("_BaseColor", new Color(.25f, .12f, .05f)); wood.SetFloat("_Smoothness", 0);
                Cube("Ground", new Vector3(0, -.5f, 0), new Vector3(24, 1, 24), ground);
                Cube("Wooden deck", new Vector3(-2, .3f, 0), new Vector3(3, .6f, 6), wood);
                Cube("Stone block", new Vector3(2, 1, 1), new Vector3(2, 2, 2), ground);
                // Fully opaque colour in the transparent queue isolates pass
                // ordering from changes to the opaque ground behind it.
                transparent.SetColor("_BaseColor", new Color(.08f, .3f, .6f));
                transparent.SetFloat("_ZWrite", 0); transparent.renderQueue = 3000;
                Cube("Transparent-pass marker", new Vector3(4, 1, -2), new Vector3(1, 2, 1), transparent);
            }

            private void Cube(string name, Vector3 position, Vector3 scale, Material material)
            {
                var item = GameObject.CreatePrimitive(PrimitiveType.Cube); item.name = name; item.layer = 31;
                item.transform.SetParent(root.transform); item.transform.position = position; item.transform.localScale = scale;
                item.GetComponent<MeshRenderer>().sharedMaterial = material;
            }

            internal Color[] Capture(string name = null)
            {
                var target = new RenderTexture(256, 256, 24);
                var image = new Texture2D(256, 256, TextureFormat.RGBA32, false);
                var previous = RenderTexture.active;
                try
                {
                    RenderPipeline.SubmitRenderRequest(Camera, new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                    RenderTexture.active = target; image.ReadPixels(new Rect(0, 0, 256, 256), 0, 0); image.Apply();
                    if (name != null)
                    {
                        var directory = Path.Combine(Path.GetTempPath(), "motu-squall-wetness"); Directory.CreateDirectory(directory);
                        File.WriteAllBytes(Path.Combine(directory, name + ".png"), image.EncodeToPNG());
                    }
                    return image.GetPixels();
                }
                finally { RenderTexture.active = previous; Object.DestroyImmediate(target); Object.DestroyImmediate(image); }
            }

            public void Dispose()
            {
                ReleaseWorld(Environment);
                Object.DestroyImmediate(root); Object.DestroyImmediate(ground); Object.DestroyImmediate(wood);
                Object.DestroyImmediate(transparent); baseline.Restore();
            }
        }
    }
}
