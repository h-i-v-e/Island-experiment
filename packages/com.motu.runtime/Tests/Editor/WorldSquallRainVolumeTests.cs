using System;
using System.Collections;
using System.IO;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldSquallRainVolumeTests
    {
        private sealed class Fixture : IDisposable
        {
            private const int Width = 640, Height = 360;
            private readonly EnvironmentHostState baseline = new EnvironmentHostState();
            private readonly GameObject root = new GameObject("Rain volume fixture") { layer = 31 };
            private readonly GameObject environmentObject = new GameObject("Rain volume lighting");
            private readonly GameObject cameraObject = new GameObject("Rain volume camera");
            private readonly RenderTexture target = new RenderTexture(Width, Height, 24);
            private readonly Texture2D image = new Texture2D(Width, Height, TextureFormat.RGB24, false);
            private readonly RenderTexture previous = RenderTexture.active;
            private readonly SquallVisualRenderer visuals;
            private readonly SquallWeatherSettings settings;
            internal readonly Camera Camera;
            internal Vector2 WindVelocity = new Vector2(36, 0);
            internal static readonly RectInt Band = new RectInt(40, 164, 560, 32);
            internal float CloudBaseHeight => visuals.Root.transform.position.y + settings.CloudBaseMetres;
            internal float HorizonSubmersion => visuals.HorizonSubmersion;

            internal Fixture()
            {
                Camera = cameraObject.AddComponent<Camera>();
                Camera.enabled = false;
                Camera.clearFlags = CameraClearFlags.SolidColor;
                Camera.backgroundColor = new Color(.3f, .4f, .5f);
                Camera.nearClipPlane = .1f; Camera.farClipPlane = 1500;
                Camera.fieldOfView = 55; Camera.cullingMask = 1 << 31;
                Camera.allowHDR = false; Camera.allowMSAA = false;
                Camera.targetTexture = target;
                Camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                Camera.transform.position = new Vector3(0, 12, -400);
                Camera.transform.rotation = Quaternion.identity;
                var environment = environmentObject.AddComponent<WorldEnvironmentController>();
                environment.Initialize(new WorldEnvironmentSettings().WithSeed(7129),
                    new IslandCloudSettings(), 64, 128, Camera.transform);
                environment.SetSeaVisible(false);
                settings = SquallWeatherSettings.Default;
                settings.LightningBrightness = 0;
                visuals = new SquallVisualRenderer(root.transform);
                foreach (var particles in root.GetComponentsInChildren<ParticleSystemRenderer>(true))
                    particles.enabled = false;
            }

            internal Color32[] Render(float rain, float age, Vector3 centre, string capture = null, float diameter = 400)
            {
                var weather = settings;
                weather.RainIntensity = rain;
                var field = new WorldSquallField(new Vector2(centre.x, centre.z), diameter * .5f,
                    .25f, 4, 1, new Vector2(8, 0));
                visuals.UpdateVisuals(new WorldSquallState(centre, diameter, age, 1, 7129, field, 1),
                    weather, Camera.transform.position, 0, windVelocity: WindVelocity);
                foreach (var particles in root.GetComponentsInChildren<ParticleSystem>(true))
                    particles.Clear(false);
                Assert.That(visuals.RainParticleCount, Is.Zero,
                    "Volume regressions must not be satisfied by nearby rain droplets.");
                Assert.That(SquallVisualRenderer.NeedsRender(Camera), Is.True);
                RenderPipeline.SubmitRenderRequest(Camera,
                    new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, Width, Height), 0, 0); image.Apply();
                if (capture != null)
                {
                    var directory = Path.Combine(Path.GetTempPath(), "motu-squall-rain-volume");
                    Directory.CreateDirectory(directory);
                    File.WriteAllBytes(Path.Combine(directory, capture + ".png"), image.EncodeToPNG());
                }
                return image.GetPixels32();
            }

            internal float[] RainResponse(float age, Vector3 centre, string label, float diameter = 400)
            {
                var dry = Render(0, age, centre, label + "-no-rain", diameter);
                var wet = Render(1, age, centre, label + "-rain", diameter);
                var response = new float[wet.Length];
                for (var i = 0; i < response.Length; i++) response[i] = Difference(wet[i], dry[i]);
                return response;
            }

            internal GameObject Blocker(out Material material)
            {
                var blocker = GameObject.CreatePrimitive(PrimitiveType.Cube);
                blocker.name = "Rain scene depth blocker";
                blocker.layer = 31;
                blocker.transform.localScale = new Vector3(1200, 1200, 1);
                material = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
                material.SetColor("_BaseColor", Camera.backgroundColor);
                blocker.GetComponent<Renderer>().sharedMaterial = material;
                return blocker;
            }

            internal GameObject Sea(out Material material)
            {
                var sea = GameObject.CreatePrimitive(PrimitiveType.Plane);
                sea.name = "Actual sea level depth surface";
                sea.layer = 31;
                sea.transform.localScale = new Vector3(600, 1, 600);
                material = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
                material.SetColor("_BaseColor", new Color(.08f, .15f, .2f));
                sea.GetComponent<Renderer>().sharedMaterial = material;
                return sea;
            }

            public void Dispose()
            {
                visuals.Dispose();
                RenderTexture.active = previous;
                Object.DestroyImmediate(root); Object.DestroyImmediate(environmentObject);
                Object.DestroyImmediate(cameraObject); Object.DestroyImmediate(target);
                Object.DestroyImmediate(image);
                baseline.Restore();
            }
        }

        [UnityTest]
        public IEnumerator RainVolumeIsVisibleWithoutDropletsAndAnimatesOnlyWhenItsClockAdvances()
        {
            RequireUrp();
            using var fixture = new Fixture();
            yield return null;
            var dry = fixture.Render(0, 0, Vector3.zero, "outside-no-rain");
            var wet = fixture.Render(1, 0, Vector3.zero, "outside-rain");
            AssertVisible(dry, wet, "Outside the storm, below the cloud");
            var paused = fixture.Render(1, 0, Vector3.zero, "outside-paused");
            Assert.That(MeanDifference(wet, paused), Is.LessThan(1f / 255),
                "A paused weather clock must keep the world-space rain volume stable.");
            var later = fixture.Render(1, 2, Vector3.zero, "outside-animated");
            foreach (var diameter in new[] { 200f, 1000f, 2000f })
            {
                var noRain = fixture.Render(0, 2, Vector3.zero, null, diameter);
                foreach (var intensity in new[] { 1f, 4f })
                {
                    var rain = fixture.Render(intensity, 2, Vector3.zero, $"diameter-{diameter}-rain-{intensity}", diameter);
                    AssertVisible(noRain, rain, $"Rain curtain at {diameter} m diameter and intensity {intensity}");
                }
            }
            Assert.That(MeanDifference(wet, later), Is.GreaterThan(.001f),
                "Rain shafts must visibly evolve without a camera or storm translation.");
        }

        [UnityTest]
        public IEnumerator RainCurtainLeansDownwindAndLeansFurtherInStrongerWind()
        {
            RequireUrp();
            using var fixture = new Fixture();
            fixture.Camera.orthographic = true;
            fixture.Camera.orthographicSize = 120;
            fixture.Camera.transform.position = new Vector3(0, 80, -700);
            yield return null;
            var calm = RainLean(fixture, Vector2.zero, "wind-calm");
            var light = RainLean(fixture, Vector2.right * 9, "wind-light");
            var right = RainLean(fixture, Vector2.right * 36, "wind-right");
            var left = RainLean(fixture, Vector2.left * 36, "wind-left");
            Debug.Log($"Rain curtain lean (pixels): calm={calm:F2}, light={light:F2}, right={right:F2}, left={left:F2}");
            Assert.That(right - calm, Is.GreaterThan(25), "Rain near the sea must be visibly farther downwind than its cloud source.");
            Assert.That(calm - left, Is.GreaterThan(25), "Reversing wind must reverse the curtain's lean.");
            Assert.That(right - light, Is.GreaterThan(15), "Stronger wind must produce a stronger slant.");
        }

        private static float RainLean(Fixture fixture, Vector2 wind, string label)
        {
            fixture.WindVelocity = wind;
            var response = fixture.RainResponse(0, Vector3.zero, label, 200);
            var bottom = Mathf.RoundToInt(fixture.Camera.WorldToScreenPoint(Vector3.up * 8).y);
            var top = Mathf.RoundToInt(fixture.Camera.WorldToScreenPoint(Vector3.up * 50).y);
            return CentroidX(response, new RectInt(16, bottom - 3, 608, 6))
                - CentroidX(response, new RectInt(16, top - 3, 608, 6));
        }

        [UnityTest]
        public IEnumerator RainVolumeTravelsWithItsStormAndRemainsVisibleFromInsideAndTheEdge()
        {
            RequireUrp();
            using var fixture = new Fixture();
            yield return null;
            var before = fixture.RainResponse(0, Vector3.zero, "movement-before");
            var after = fixture.RainResponse(0, Vector3.right * 160, "movement-after");
            Assert.That(CentroidX(after) - CentroidX(before), Is.GreaterThan(60),
                "Moving only the storm must move its rain across the actual camera image.");
            fixture.Camera.transform.position = new Vector3(0, 12, 0);
            AssertVisible(fixture.Render(0, 0, Vector3.zero, "inside-no-rain"),
                fixture.Render(1, 0, Vector3.zero, "inside-rain"), "Inside the storm");
            fixture.Camera.transform.position = new Vector3(190, 12, -30);
            fixture.Camera.transform.LookAt(new Vector3(0, 12, 80));
            AssertVisible(fixture.Render(0, 0, Vector3.zero, "edge-no-rain"),
                fixture.Render(1, 0, Vector3.zero, "edge-rain"), "Looking inward from the edge");
        }

        [UnityTest]
        public IEnumerator SceneDepthClipsTheRainAtTheActualForegroundAndInteriorSurfaces()
        {
            RequireUrp();
            using var fixture = new Fixture();
            var blocker = fixture.Blocker(out var material);
            try
            {
                yield return null;
                blocker.transform.position = new Vector3(0, 12, 250);
                var behind = Mean(fixture.RainResponse(0, Vector3.zero, "depth-behind"));
                blocker.transform.position = new Vector3(0, 12, 0);
                var interior = Mean(fixture.RainResponse(0, Vector3.zero, "depth-interior"));
                blocker.transform.position = new Vector3(0, 12, -250);
                var foreground = Mean(fixture.RainResponse(0, Vector3.zero, "depth-foreground"));
                Assert.That(behind, Is.GreaterThan(.01f), "Rain in front of a background surface must remain visible.");
                Assert.That(interior, Is.InRange(behind * .15f, behind * .95f),
                    "A surface inside the storm must retain the nearer rain and remove the farther rain.");
                Assert.That(foreground, Is.LessThan(behind * .03f),
                    "An opaque surface before the volume must hide the rain behind it.");
            }
            finally { Object.DestroyImmediate(blocker); Object.DestroyImmediate(material); }
        }

        [UnityTest]
        public IEnumerator HalfResolutionRainDoesNotBleedAcrossThinForegroundSilhouettes()
        {
            RequireUrp();
            using var fixture = new Fixture();
            var blocker = fixture.Blocker(out var material);
            try
            {
                blocker.transform.position = new Vector3(.3f, 12, -250);
                yield return null;
                foreach (var width in new[] { 20f, .8f })
                {
                    blocker.transform.localScale = new Vector3(width, 200, 1);
                    var dry = fixture.Render(0, 0, Vector3.zero);
                    var wet = fixture.Render(1, 0, Vector3.zero, "foreground-width-" + width);
                    var centre = fixture.Camera.WorldToScreenPoint(blocker.transform.position);
                    var left = fixture.Camera.WorldToScreenPoint(blocker.transform.position - Vector3.right * width * .5f);
                    var right = fixture.Camera.WorldToScreenPoint(blocker.transform.position + Vector3.right * width * .5f);
                    var checkedPixels = 0;
                    var row = Mathf.FloorToInt(centre.y);
                    for (var x = Mathf.CeilToInt(left.x - .5f); x + .5f < right.x; ++x)
                    {
                        Assert.That(Difference(dry[row * 640 + x], wet[row * 640 + x]), Is.LessThan(2f / 255),
                            "Foreground deck/mast pixels must occlude rain even when narrower than the low-resolution volume texel.");
                        checkedPixels++;
                    }
                    Assert.That(checkedPixels, Is.GreaterThan(0));
                    var outside = row * 640 + Mathf.CeilToInt(right.x) + 5;
                    Assert.That(Difference(dry[outside], wet[outside]), Is.GreaterThan(.01f),
                        "Rain beside the foreground silhouette must remain visible.");
                }
            }
            finally { Object.DestroyImmediate(blocker); Object.DestroyImmediate(material); }
        }

        [UnityTest]
        public IEnumerator DistantRainRemainsReadableBelowTheCloudAndAtActualSeaLevel()
        {
            RequireUrp();
            using var fixture = new Fixture();
            fixture.Camera.transform.position = new Vector3(0, 12, -2400);
            fixture.Camera.farClipPlane = 5000;
            var sea = fixture.Sea(out var material);
            try
            {
                yield return null;
                var dry = fixture.Render(0, 0, Vector3.zero, "distant-no-rain");
                var wet = fixture.Render(1, 0, Vector3.zero, "distant-rain");
                Assert.That(fixture.HorizonSubmersion, Is.GreaterThan(0),
                    "The distant regression must exercise the sinking visual proxy.");
                var left = fixture.Camera.WorldToScreenPoint(Vector3.left * 120);
                var right = fixture.Camera.WorldToScreenPoint(Vector3.right * 120);
                var seaPoint = fixture.Camera.WorldToScreenPoint(Vector3.zero);
                var cloudBase = fixture.Camera.WorldToScreenPoint(Vector3.up * fixture.CloudBaseHeight);
                var x = Mathf.CeilToInt(left.x);
                var width = Mathf.FloorToInt(right.x) - x;
                var y = Mathf.CeilToInt(seaPoint.y);
                var belowCloud = new RectInt(x, y, width, Mathf.FloorToInt(cloudBase.y) - 1 - y);
                Assert.That(belowCloud.height, Is.GreaterThan(1), "The rain-only region must resolve below the cloud.");
                AssertDistantContrast(dry, wet, belowCloud, 2f / 255, "Distant rain below cloud");
                // This row crosses the real sea surface behind the storm's core.
                // A rain base that sinks with the proxy must still reach it.
                var seaAdjacent = new RectInt(x, Mathf.FloorToInt(seaPoint.y), width, 1);
                AssertDistantContrast(dry, wet, seaAdjacent, 1f / 255, "Distant rain at actual sea level");
            }
            finally { Object.DestroyImmediate(sea); Object.DestroyImmediate(material); }
        }

        private static void RequireUrp()
        {
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset))
                Assert.Ignore("The rain volume regression requires URP and the Motu renderer feature.");
        }

        private static float Difference(Color32 a, Color32 b) =>
            (Mathf.Abs(a.r - b.r) + Mathf.Abs(a.g - b.g) + Mathf.Abs(a.b - b.b)) / (3f * 255);

        private static void AssertDistantContrast(Color32[] dry, Color32[] wet,
            RectInt region, float minimumMean, string label)
        {
            var sum = 0f; var changed = 0;
            for (var y = region.yMin; y < region.yMax; y++)
            for (var x = region.xMin; x < region.xMax; x++)
            {
                var difference = Difference(dry[y * 640 + x], wet[y * 640 + x]);
                sum += difference;
                if (difference > 1f / 255) changed++;
            }
            var count = region.width * region.height;
            var mean = sum / count;
            Debug.Log($"{label}: mean RGB difference={mean:F6}, changed={changed}/{count}, region={region}");
            Assert.That(mean, Is.GreaterThan(minimumMean),
                label + " must retain contrast across the projected rain core.");
            Assert.That(changed, Is.GreaterThan(count / 2),
                label + " must affect most of the measured core, rather than isolated pixels.");
        }

        private static void AssertVisible(Color32[] dry, Color32[] wet, string view)
        {
            var changed = 0;
            var band = Fixture.Band;
            for (var y = band.yMin; y < band.yMax; y++)
            for (var x = band.xMin; x < band.xMax; x++)
                if (Difference(dry[y * 640 + x], wet[y * 640 + x]) > .01f) changed++;
            Assert.That(changed, Is.GreaterThan(band.width * band.height / 20),
                view + " must show a broad volume of rain with nearby particles disabled.");
            Assert.That(MeanDifference(dry, wet), Is.GreaterThan(.005f), view);
        }

        private static float MeanDifference(Color32[] first, Color32[] second)
        {
            var response = new float[first.Length];
            for (var i = 0; i < response.Length; i++) response[i] = Difference(first[i], second[i]);
            return Mean(response);
        }

        private static float Mean(float[] response)
        {
            var sum = 0f;
            var band = Fixture.Band;
            for (var y = band.yMin; y < band.yMax; y++)
            for (var x = band.xMin; x < band.xMax; x++) sum += response[y * 640 + x];
            return sum / (band.width * band.height);
        }

        private static float CentroidX(float[] response, RectInt? region = null)
        {
            var weight = 0f; var weightedX = 0f;
            var band = region ?? Fixture.Band;
            for (var y = band.yMin; y < band.yMax; y++)
            for (var x = band.xMin; x < band.xMax; x++)
            {
                var value = response[y * 640 + x];
                weight += value; weightedX += x * value;
            }
            Assert.That(weight, Is.GreaterThan(20), "A rain image is required to measure its movement.");
            return weightedX / weight;
        }
    }
}
