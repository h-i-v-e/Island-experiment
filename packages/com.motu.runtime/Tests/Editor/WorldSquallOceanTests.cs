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
    public sealed class WorldSquallOceanTests
    {
        [Test]
        public void BoundsExpandForSquallsWithoutChangingAmbientPhasesOrTimeScale()
        {
            using var fixture = new Fixture();
            var ocean = fixture.Ocean;
            var baseline = ocean.SurfaceMesh.bounds;
            var phase = ocean.SurfaceMaterial.GetVector("_OceanWaveFrom0");
            ocean.ApplySquallWindMultiplier(2);
            Assert.That(ocean.SurfaceMesh.bounds.size.y, Is.GreaterThan(baseline.size.y));
            Assert.That(ocean.WaveTimeScale, Is.EqualTo(1f));
            Assert.That(ocean.SurfaceMaterial.GetVector("_OceanWaveFrom0"), Is.EqualTo(phase));
            ocean.ApplySquallWindMultiplier(1);
            Assert.That(ocean.SurfaceMesh.bounds, Is.EqualTo(baseline));
        }

        [Test]
        public void GpuWavesStayAmbientOutsideAndFollowLocalWindAndItsEdgeGradient()
        {
            RequireFloatGpu();
            using var fixture = new Fixture();
            var field = fixture.Field;
            foreach (float x in new[] { 10f, 110f, 92f })
            {
                var point = new Vector2(x, 0);
                const float step = .01f;
                float slopeX = (ReferenceDisplacement(point + Vector2.right * step, field).y
                    - ReferenceDisplacement(point - Vector2.right * step, field).y) / (2 * step);
                float slopeZ = (ReferenceDisplacement(point + Vector2.up * step, field).y
                    - ReferenceDisplacement(point - Vector2.up * step, field).y) / (2 * step);
                var actual = fixture.Probe(point);
                Assert.That(actual.r, Is.EqualTo(ReferenceDisplacement(point, field).y).Within(.0002f), "Height at " + x);
                Assert.That(actual.g, Is.EqualTo(slopeX).Within(.0003f), "Slope at " + x);
                Assert.That(actual.b, Is.EqualTo(slopeZ).Within(.0003f));
                float multiplier = field.MultiplierAt(point);
                Assert.That(actual.a, Is.EqualTo(Mathf.Sqrt(multiplier) * (1 + .2f * (1 - 1 / multiplier))).Within(.0001f), "Wave envelope at " + x);
            }
            Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 40,
                WorldEnvironmentController.EvaluateWaveHeightScale(40)));
            var stormAtHighWind = fixture.Probe(new Vector2(10, 0));
            Assert.That(stormAtHighWind.r, Is.EqualTo(ReferenceDisplacement(new Vector2(10, 0), field).y
                * WorldEnvironmentController.EvaluateWaveHeightScale(40)).Within(.0003f),
                "A local storm retains its extra height even above the ordinary ambient response cap.");
            Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
            fixture.AssertDepthLimitedEdges();
        }

        [Test]
        public void StormRaisesHeightSteepnessAndFoamAfterAmbientWhitecapsReachFullWind()
        {
            RequireFloatGpu();
            using var fixture = new Fixture();
            ConfigureFoam(fixture.Ocean.SurfaceMaterial);
            Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 25, 1));
            WorldSquallShader.Publish(0, default);
            var ambient = Measure(fixture.GridProbe());
            var outside = fixture.Probe(new Vector2(140, 0));
            var field = new WorldSquallField(Vector2.zero, 100, .25f, 4, 1, Vector2.right * 9);
            WorldSquallShader.Publish(0, field);
            var storm = Measure(fixture.GridProbe());
            Assert.That(storm.x, Is.GreaterThan(ambient.x * 1.5f), "Wave RMS height must rise inside the footprint.");
            Assert.That(storm.y, Is.GreaterThan(ambient.y * 1.5f), "Short wind waves must increase actual geometric steepness.");
            Assert.That(storm.z, Is.GreaterThan(ambient.z * 1.5f), "Foam must keep increasing after the ambient wind foam factor saturates.");
            Assert.That(fixture.Probe(new Vector2(140, 0)), Is.EqualTo(outside), "Water outside the storm stays unchanged.");
            var directory = Path.Combine(Path.GetTempPath(), "motu-squall-ocean"); Directory.CreateDirectory(directory);
            File.WriteAllText(Path.Combine(directory, "response.txt"), $"Ambient RMS height, RMS slope, mean foam: {ambient}\nStorm: {storm}\n");
        }

        [UnityTest]
        public IEnumerator ActualOceanCameraShowsRougherFoamCoveredWaterInsideTheStorm()
        {
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset)) Assert.Ignore("URP is required.");
            var baseline = new EnvironmentHostState();
            var lightingObject = new GameObject("Squall ocean render lighting");
            var cameraObject = new GameObject("Squall ocean render camera");
            try
            {
                var camera = cameraObject.AddComponent<Camera>(); camera.enabled = false;
                camera.transform.position = new Vector3(0, 14, -55);
                camera.transform.LookAt(new Vector3(0, 3, 15));
                camera.clearFlags = CameraClearFlags.SolidColor; camera.backgroundColor = new Color(.18f, .28f, .38f);
                camera.cullingMask = 1 << 31; camera.nearClipPlane = .3f; camera.farClipPlane = 500;
                camera.allowHDR = false; camera.allowMSAA = false;
                camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                var environment = lightingObject.AddComponent<WorldEnvironmentController>();
                environment.Initialize(new WorldEnvironmentSettings().WithSeed(7129), new IslandCloudSettings(), 256, 512, camera.transform);
                environment.SetSeaVisible(false);
                var noise = Shader.GetGlobalTexture("_MotuWindNoise");
                using var fixture = new Fixture();
                fixture.Ocean.SurfaceTransform.gameObject.layer = 31;
                fixture.Ocean.SurfaceTransform.gameObject.SetActive(true);
                Shader.SetGlobalTexture("_MotuWindNoise", noise);
                ConfigureFoam(fixture.Ocean.SurfaceMaterial);
                yield return null;
                WorldSquallShader.Publish(0, default);
                var ambient = Capture(camera, "ambient-sea");
                fixture.Ocean.ApplySquallWindMultiplier(4);
                WorldSquallShader.Publish(0, new WorldSquallField(Vector2.zero, 100, .25f, 4, 1, Vector2.right * 9));
                var storm = Capture(camera, "storm-sea");
                var changed = 0;
                for (var i = 0; i < ambient.Length; i++)
                    if (Mathf.Abs(ambient[i].r - storm[i].r) + Mathf.Abs(ambient[i].g - storm[i].g) + Mathf.Abs(ambient[i].b - storm[i].b) > 20) changed++;
                Assert.That(changed, Is.GreaterThan(ambient.Length / 20), "The rendered sea must visibly respond to its local storm.");
            }
            finally { Object.DestroyImmediate(cameraObject); Object.DestroyImmediate(lightingObject); baseline.Restore(); }
        }

        private static void ConfigureFoam(Material material)
        {
            material.SetFloat("_WhitecapStrength", 1);
            material.SetFloat("_WhitecapCoverage", .8f);
            material.SetFloat("_WhitecapFullWindSpeed", 18);
        }

        private static Vector3 Measure(Color[] values)
        {
            double height = 0, slope = 0, foam = 0;
            foreach (var value in values) { height += value.r * value.r; slope += value.g * value.g; foam += value.b; }
            return new Vector3(Mathf.Sqrt((float)(height / values.Length)), Mathf.Sqrt((float)(slope / values.Length)), (float)(foam / values.Length));
        }

        private static Color32[] Capture(Camera camera, string name)
        {
            var target = new RenderTexture(768, 432, 24);
            var image = new Texture2D(768, 432, TextureFormat.RGB24, false);
            var previous = RenderTexture.active;
            try
            {
                camera.targetTexture = target;
                RenderPipeline.SubmitRenderRequest(camera, new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0); image.Apply();
                var directory = Path.Combine(Path.GetTempPath(), "motu-squall-ocean"); Directory.CreateDirectory(directory);
                File.WriteAllBytes(Path.Combine(directory, name + ".png"), image.EncodeToPNG());
                return image.GetPixels32();
            }
            finally { camera.targetTexture = null; RenderTexture.active = previous; Object.DestroyImmediate(target); Object.DestroyImmediate(image); }
        }

        [UnityTest]
        public IEnumerator BuoyancyQuerySnapshotsTheFootprintAndAdvectsItForWaterVelocity()
        {
            Assert.That(OceanSurfaceSampler.Supported, Is.True);
            using var fixture = new Fixture();
            using var sampler = new OceanSurfaceSampler(3, includeVelocity: true);
            var positions = new[] { new Vector3(10, 0, 0), new Vector3(110, 0, 0), new Vector3(92, 0, 0) };
            sampler.Submit(fixture.Ocean, positions);
            // Weather can advance or release its globals before readback completes.
            Shader.SetGlobalVector("_MotuSquall", Vector4.zero);
            Shader.SetGlobalVector("_MotuSquallWind", Vector4.zero);
            float until = Time.realtimeSinceStartup + 10;
            while (sampler.Pending && Time.realtimeSinceStartup < until) yield return null;
            Assert.That(sampler.Pending, Is.False, "Squall query timed out.");
            for (int i = 0; i < positions.Length; i++)
            {
                var position = positions[i];
                var point = new Vector2(position.x, position.z);
                var basePoint = ReferenceBasePoint(point, fixture.Field);
                Assert.That(sampler.TryGetSurface(i, position, .5f, out var height, out var velocity), Is.True);
                Assert.That(height, Is.EqualTo(3f + ReferenceDisplacement(basePoint, fixture.Field).y).Within(.005f), "Captured height at " + i);
                const float step = .02f;
                var expectedVelocity = (ReferenceDisplacement(basePoint, fixture.Field, step)
                    - ReferenceDisplacement(basePoint, fixture.Field, -step)) / (2 * step);
                Assert.That(Vector3.Distance(velocity, expectedVelocity), Is.LessThan(.008f), "Advected water velocity at " + i);
            }
        }

        [UnityTest]
        public IEnumerator BuoyancySnapshotsEveryCellBeforeShaderGlobalsChange()
        {
            Assert.That(OceanSurfaceSampler.Supported, Is.True);
            using var fixture = new Fixture();
            using var sampler = new OceanSurfaceSampler(3, includeVelocity: true);
            var fields = new[] {
                new WorldSquallField(new Vector2(10, 0), 50, .25f, 2, 1, Vector2.right * 9),
                new WorldSquallField(new Vector2(110, 0), 50, .25f, 3, 1, Vector2.right * 5),
                new WorldSquallField(new Vector2(210, 0), 50, .25f, 4, 1, Vector2.right * 12) };
            var positions = new[] { new Vector3(10, 0, 0), new Vector3(110, 0, 0), new Vector3(210, 0, 0) };
            for (var slot = 0; slot < fields.Length; slot++) WorldSquallShader.Publish(slot, fields[slot]);
            var expectedHeights = new float[positions.Length];
            for (var i = 0; i < positions.Length; i++)
                expectedHeights[i] = 3f + ReferenceDisplacement(ReferenceBasePoint(
                    new Vector2(positions[i].x, positions[i].z), fields[i]), fields[i]).y;
            sampler.Submit(fixture.Ocean, positions);
            for (var slot = 0; slot < fields.Length; slot++) WorldSquallShader.Publish(slot, default);
            var until = Time.realtimeSinceStartup + 10;
            while (sampler.Pending && Time.realtimeSinceStartup < until) yield return null;
            Assert.That(sampler.Pending, Is.False);
            for (var i = 0; i < positions.Length; i++)
            {
                Assert.That(sampler.TryGetSurface(i, positions[i], .5f, out var height, out var velocity), Is.True);
                var wave = Mathf.Sin(positions[i].x * (2 * Mathf.PI / 40));
                Assert.That(height, Is.EqualTo(expectedHeights[i]).Within(.005f), "Physics must match the displayed, depth-limited water at each cell.");
                var point = ReferenceBasePoint(new Vector2(positions[i].x, positions[i].z), fields[i]);
                var expectedVelocity = (ReferenceDisplacement(point, fields[i], .02f)
                    - ReferenceDisplacement(point, fields[i], -.02f)) / .04f;
                Assert.That(Vector3.Distance(velocity, expectedVelocity), Is.LessThan(.008f), "Moving short wind waves use the captured cell.");
            }
        }

        private static Vector3 ReferenceDisplacement(Vector2 point, WorldSquallField field, float time = 0)
        {
            var moving = new WorldSquallField(field.Center + field.Velocity * time, field.RadiusMetres,
                field.EdgeFraction, field.WindMultiplier, field.Strength, field.Velocity);
            float multiplier = moving.MultiplierAt(point), activity = 1 - 1 / multiplier;
            var result = new Vector3(0, Mathf.Sin(point.x * (2 * Mathf.PI / 40)), 0);
            var directions = new[] { Vector2.right, new Vector2(.8660254f, .5f), new Vector2(.9396926f, -.3420201f) };
            var lengths = new[] { 18f, 10f, 6f }; var weights = new[] { .5f, .3f, .2f };
            for (var i = 0; i < directions.Length; i++)
            {
                float phase = (Vector2.Dot(directions[i], point) - directions[i].x * 9 * time * .35f)
                    * (2 * Mathf.PI / lengths[i]);
                float amplitude = (.2f / 1.22f) * activity * weights[i];
                result += new Vector3(directions[i].x * amplitude * Mathf.Cos(phase),
                    amplitude * (Mathf.Sin(phase) - .22f * Mathf.Cos(2 * phase)),
                    directions[i].y * amplitude * Mathf.Cos(phase));
            }
            return result * Mathf.Sqrt(multiplier);
        }

        private static Vector2 ReferenceBasePoint(Vector2 target, WorldSquallField field)
        {
            var point = target;
            for (var i = 0; i < 10; i++)
            {
                var value = ReferenceDisplacement(point, field);
                var dx = (ReferenceDisplacement(point + Vector2.right * .01f, field) - value) / .01f;
                var dz = (ReferenceDisplacement(point + Vector2.up * .01f, field) - value) / .01f;
                var error = point + new Vector2(value.x, value.z) - target;
                float determinant = (1 + dx.x) * (1 + dz.z) - dz.x * dx.z;
                point -= new Vector2((error.x * (1 + dz.z) - error.y * dz.x) / determinant,
                    (error.y * (1 + dx.x) - error.x * dx.z) / determinant);
            }
            return point;
        }

        private static void RequireFloatGpu()
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat)
                || !SystemInfo.SupportsTextureFormat(TextureFormat.RGBAFloat))
                Assert.Ignore("Ocean field parity requires floating-point render targets.");
        }

        private sealed class Fixture : IDisposable
        {
            private readonly EnvironmentHostState baseline = new EnvironmentHostState();
            private readonly GameObject root = new GameObject("Squall ocean field test");
            internal readonly OceanSurfaceController Ocean;
            internal readonly WorldSquallField Field = new WorldSquallField(Vector2.zero, 100, .15f, 2, 1, Vector2.right * 9);

            internal Fixture()
            {
                root.transform.position = Vector3.up * 3;
                Ocean = root.AddComponent<OceanSurfaceController>();
                Ocean.Install(new Material(Shader.Find("Motu/Sea Water")), 128, false);
                var weather = Ocean.Weather;
                weather.Enabled = true;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 40, 1, 0, 0);
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0;
                weather.OnshoreWaveEnabled = false;
                weather.OnshoreWaveChoppiness = 0;
                Ocean.ApplyWaveWeather(weather);
                Ocean.ApplyWeatherWindScale(1);
                // Keep analytic parity probes fully resolved; geometry LOD has
                // separate tests and must not filter the short reference waves.
                Ocean.SurfaceMaterial.SetVector("_OceanGeometryLod", Vector4.zero);
                Ocean.SurfaceMaterial.SetFloat("_WaveFadeStart", 10000);
                Ocean.SurfaceMaterial.SetFloat("_WaveFadeEnd", 20000);
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalVector("_MotuWindOffset", Vector4.zero);
                Shader.SetGlobalVector("_MotuSquall", Field.ShaderBounds);
                Shader.SetGlobalVector("_MotuSquallWind", Field.ShaderWind);
            }

            internal WorldSquallField AdvancedField(float time) => new WorldSquallField(
                Field.Center + Field.Velocity * time, Field.RadiusMetres, Field.EdgeFraction,
                Field.WindMultiplier, Field.Strength, Field.Velocity);

            internal Color Probe(Vector2 point, float distance = -1f)
                => ReadProbe(new Vector4(point.x, point.y, Mathf.Max(0, distance), distance >= 0 ? 1 : 0), 1)[0];

            internal Color[] GridProbe() => ReadProbe(new Vector4(0, 0, 160, 2), 128);

            private Color[] ReadProbe(Vector4 request, int size)
            {
                var shader = Shader.Find("Hidden/Motu/Tests/Squall Ocean");
                Assert.That(shader, Is.Not.Null);
                var material = new Material(shader);
                var target = new RenderTexture(size, size, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
                var image = new Texture2D(size, size, TextureFormat.RGBAFloat, false, true);
                var previous = RenderTexture.active;
                try
                {
                    material.CopyPropertiesFromMaterial(Ocean.SurfaceMaterial);
                    material.SetVector("_SquallTestPosition", request);
                    Graphics.Blit(Texture2D.blackTexture, target, material);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, size, size), 0, 0);
                    image.Apply();
                    return image.GetPixels();
                }
                finally
                {
                    RenderTexture.active = previous;
                    Object.DestroyImmediate(material); Object.DestroyImmediate(target); Object.DestroyImmediate(image);
                }
            }

            internal void AssertDepthLimitedEdges()
            {
                var coast = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
                coast.SetPixel(0, 0, new Color(1, 1, .05f, 1)); coast.Apply();
                Ocean.SetWaveAttenuation(coast, Texture2D.blackTexture, new Vector4(-200, -200, 1f / 400, 1f / 400));
                try
                {
                    foreach (float x in new[] { 90f, 92f, 95f })
                    {
                        Shader.SetGlobalVector("_MotuSquall", Vector4.zero);
                        Shader.SetGlobalVector("_MotuSquallWind", Vector4.zero);
                        var ambient = Probe(new Vector2(x, 0));
                        Shader.SetGlobalVector("_MotuSquall", Field.ShaderBounds);
                        Shader.SetGlobalVector("_MotuSquallWind", Field.ShaderWind);
                        var squall = Probe(new Vector2(x, 0));
                        Assert.That(ambient.a, Is.EqualTo(.5f).Within(.0001f), "Half-metre depth limit at " + x);
                        Assert.That(Mathf.Abs(squall.r), Is.LessThanOrEqualTo(.5001f), "Storm chop must respect shallow water at " + x);
                        const float step = .01f;
                        float slopeX = (Probe(new Vector2(x + step, 0)).r - Probe(new Vector2(x - step, 0)).r) / (2 * step);
                        float slopeZ = (Probe(new Vector2(x, step)).r - Probe(new Vector2(x, -step)).r) / (2 * step);
                        Assert.That(squall.g, Is.EqualTo(slopeX).Within(.0005f), "Depth-limited edge normal at " + x);
                        Assert.That(squall.b, Is.EqualTo(slopeZ).Within(.0005f));
                        Assert.That(squall.a, Is.EqualTo(ambient.a).Within(.0001f));
                    }
                }
                finally
                {
                    Ocean.SetWaveAttenuation(Texture2D.whiteTexture, Texture2D.blackTexture, new Vector4(-1, -1, .5f, .5f));
                    Object.DestroyImmediate(coast);
                }
            }

            public void Dispose() { Object.DestroyImmediate(root); baseline.Restore(); }
        }
    }
}
