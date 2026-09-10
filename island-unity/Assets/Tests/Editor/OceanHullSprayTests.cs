using System.Collections;
using System.Collections.Generic;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanHullSprayTests
    {
        [Test]
        public void DryEdgesDoNotEmitAndLaunchSpeedIsProportionalUntilCapped()
        {
            Assert.That(OceanHullSpray.WetSegment.Between(-1, 0, 10).Integral, Is.Zero);
            Assert.That(OceanHullSpray.LaunchSpeed(-1, 6, 18), Is.Zero);
            Assert.That(OceanHullSpray.LaunchSpeed(.5f, 6, 18), Is.EqualTo(3));
            Assert.That(OceanHullSpray.LaunchSpeed(1, 6, 18), Is.EqualTo(6));
            Assert.That(OceanHullSpray.LaunchSpeed(10, 6, 18), Is.EqualTo(18));
        }

        [Test]
        public void StrengthUsesClosingRelativeMotionAndRemainsLinearInDepth()
        {
            float Factor(Vector3 ship, Vector3 water) => OceanHullSpray.StrengthMultiplier(ship, water, Vector3.right, .25f);
            Assert.That(Factor(Vector3.zero, Vector3.zero), Is.EqualTo(1));
            Assert.That(Factor(Vector3.right * 2, Vector3.left * 2), Is.EqualTo(2));
            Assert.That(Factor(Vector3.left * 2, Vector3.right * 2), Is.EqualTo(1), "Separating motion adds no impact.");
            Assert.That(Factor(Vector3.zero, Vector3.up * 2), Is.EqualTo(1.5f));
            Assert.That(Factor(Vector3.down * 2, Vector3.zero), Is.EqualTo(1.5f));
            var sharedMotion = new Vector3(7, -3, 4);
            Assert.That(Factor(sharedMotion, sharedMotion), Is.EqualTo(1));
            Assert.That(Factor(sharedMotion + Vector3.right * 2, sharedMotion + Vector3.left * 2), Is.EqualTo(2));
            Assert.That(Factor(Vector3.forward * 10, Vector3.zero), Is.EqualTo(1), "Tangential travel does not hit this hull segment.");
            var strength = Factor(Vector3.right * 2, Vector3.left * 2);
            Assert.That(OceanHullSpray.LaunchSpeed(.5f * strength, 6, 18), Is.EqualTo(6));
            Assert.That(OceanHullSpray.LaunchSpeed(1 * strength, 6, 18), Is.EqualTo(12));
            Assert.That(OceanHullSpray.LaunchSpeed(-1 * strength, 6, 18), Is.Zero);
        }

        [UnityTest]
        public IEnumerator GpuSamplesReturnLocalWaterVelocityWithoutChangingHeightQueries()
        {
            var root = new GameObject("Wave velocity query fixture");
            root.transform.position = Vector3.up * 3;
            var ocean = root.AddComponent<OceanSurfaceController>();
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            OceanSurfaceSampler motion = null, heightOnly = null;
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                ocean.Install(new Material(Shader.Find("Motu/Sea Water")), 128, true);
                var weather = ocean.Weather;
                weather.Enabled = true;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 30, 1, 3);
                weather.Wave0.Choppiness = .8f;
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0;
                weather.OnshoreWaveEnabled = false;
                ocean.ApplyWaveWeather(weather);
                motion = new OceanSurfaceSampler(3, includeVelocity: true);
                heightOnly = new OceanSurfaceSampler(3);
                var points = new[] { new Vector3(7.5f, 50, 0), new Vector3(22.5f, -20, 0), new Vector3(.8f, 0, 0) };
                var expectedHeights = new[] { 4.176f, 2.176f, 2.824f };
                var omega = 3 * 2 * Mathf.PI / 30;
                var expectedVelocities = new[] { Vector3.left * (.8f * omega), Vector3.right * (.8f * omega), Vector3.up * omega };
                foreach (var timeScale in new[] { 1f, 2f })
                {
                    ocean.ApplyWeatherWindScale(timeScale);
                    motion.Submit(ocean, points);
                    heightOnly.Submit(ocean, points);
                    var deadline = Time.realtimeSinceStartup + 10;
                    while ((motion.Pending || heightOnly.Pending) && Time.realtimeSinceStartup < deadline) yield return null;
                    Assert.IsFalse(motion.Pending || heightOnly.Pending);
                    for (var i = 0; i < points.Length; i++)
                    {
                        Assert.IsTrue(motion.TryGetSurface(i, points[i], 1, out var height, out var velocity));
                        Assert.IsTrue(heightOnly.TryGetHeight(i, points[i], 1, out var oldHeight));
                        Assert.That(height, Is.EqualTo(oldHeight).Within(.001f));
                        Assert.That(height, Is.EqualTo(expectedHeights[i]).Within(.003f));
                        Assert.That(Vector3.Distance(velocity, expectedVelocities[i] * timeScale), Is.LessThan(.003f));
                    }
                }
                weather.Wave0.SpeedMetresPerSecond = 0;
                ocean.ApplyWaveWeather(weather);
                motion.Submit(ocean, points);
                while (motion.Pending) yield return null;
                Assert.IsTrue(motion.TryGetSurface(0, points[0], 1, out _, out var stopped));
                Assert.That(stopped.magnitude, Is.LessThan(.001f), "A stationary wave has no orbital velocity.");
                Assert.IsFalse(ShaderUtil.ShaderHasError(Resources.Load<Shader>("OceanSurfaceQuery")));
            }
            finally
            {
                motion?.Dispose(); heightOnly?.Dispose();
                Object.DestroyImmediate(root);
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
            }
        }

        [Test]
        public void PartlyWetEdgesClipAtTheCrossingAndDensityDoesNotDependOnSubdivision()
        {
            var segment = OceanHullSpray.WetSegment.Between(-1, 3, 8);
            Assert.That(segment.Start, Is.EqualTo(.25f));
            Assert.That(segment.End, Is.EqualTo(1));
            Assert.That(segment.Integral, Is.EqualTo(9));
            var divided = OceanHullSpray.WetSegment.Between(-1, 1, 4).Integral
                + OceanHullSpray.WetSegment.Between(1, 3, 4).Integral;
            Assert.That(divided, Is.EqualTo(segment.Integral));
            var reverse = OceanHullSpray.WetSegment.Between(3, -1, 8);
            Assert.That(reverse.Integral, Is.EqualTo(segment.Integral));
            Assert.That(reverse.Sample(1), Is.EqualTo(.75f));
            var mean = 0f;
            for (var i = 0; i < 1000; i++)
            {
                var t = segment.Sample((i + .5f) / 1000);
                Assert.That(t, Is.InRange(.25f, 1));
                Assert.That(reverse.Sample((i + .5f) / 1000), Is.InRange(0, .75f));
                mean += t / 1000;
            }
            Assert.That(mean, Is.EqualTo(.75f).Within(.001), "Density grows toward the deeper end.");
        }

        [Test]
        public void OutlineFollowsConcaveHullInsteadOfPaddedTextureAndClosesWithinBudget()
        {
            var outline = new[] { new Vector2(-5,-5), new Vector2(5,-5), new Vector2(5,5),
                new Vector2(2,5), new Vector2(2,-2), new Vector2(-2,-2), new Vector2(-2,5), new Vector2(-5,5) };
            var slices = new List<ShipWaterlineTextureBuilder.Segment>();
            for (var i = 0; i < outline.Length; i++) slices.Add(new ShipWaterlineTextureBuilder.Segment(outline[i], outline[(i + 1) % outline.Length]));
            var hull = ShipWaterlineTextureBuilder.Build(slices, new ShipWaterlineTextureBuilder.Settings
                { resolution = 512, gapClosure = 0, blend = 12, bowWave = false });
            try
            {
                var points = ShipSprayLoopBuilder.Build(hull, .5f, 256);
                Assert.That(points.Length, Is.InRange(90, 150));
                Assert.That(ShipSprayLoopBuilder.SignedArea(points), Is.GreaterThan(0));
                var notch = false;
                for (var i = 0; i < points.Length; i++)
                {
                    Assert.That(Mathf.Abs(points[i].x), Is.LessThan(5.2f));
                    Assert.That(Mathf.Abs(points[i].y), Is.LessThan(5.2f));
                    Assert.That(Vector2.Distance(points[i], points[(i + 1) % points.Length]), Is.InRange(.01f, .51f));
                    notch |= Mathf.Abs(points[i].x) < 1 && Mathf.Abs(points[i].y + 2) < .2f;
                }
                Assert.IsTrue(notch, "Spray follows the concave notch.");
                Assert.That(ShipSprayLoopBuilder.Build(hull, .1f, 32).Length, Is.EqualTo(32));
            }
            finally { Object.DestroyImmediate(hull.Texture); }
        }

        [Test]
        public void SavedShipHasAClosedLoopAndCorrectNormalDespiteItsImportTransform()
        {
            EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            try
            {
                var spray = Object.FindFirstObjectByType<OceanHullSpray>();
                Assert.IsNotNull(spray);
                var data = new SerializedObject(spray);
                var points = data.FindProperty("waterline");
                Assert.That(points.arraySize, Is.InRange(32, 256));
                var normal = spray.transform.TransformDirection(data.FindProperty("waterlineUp").vector3Value);
                Assert.That(Vector3.Dot(normal, Vector3.up), Is.GreaterThan(.999f));
                var height = ShipWaterlineTextureWindow.EstimateWaterline(spray.transform);
                for (var i = 0; i < points.arraySize; i++)
                {
                    var a = spray.transform.TransformPoint(points.GetArrayElementAtIndex(i).vector3Value);
                    var b = spray.transform.TransformPoint(points.GetArrayElementAtIndex((i + 1) % points.arraySize).vector3Value);
                    Assert.That(a.y, Is.EqualTo(height).Within(.001f));
                    Assert.That(Vector3.Distance(a, b), Is.InRange(.01f, 1.01f));
                    Assert.That(Mathf.Abs(Vector3.Cross(normal, (b-a).normalized).y), Is.LessThan(.001f));
                }
                var material = (Material)data.FindProperty("sprayMaterial").objectReferenceValue;
                Assert.IsNotNull(material);
                Assert.IsTrue(material.shader.isSupported);
            }
            finally { EditorSceneManager.NewScene(NewSceneSetup.EmptyScene); }
        }

        private static float ClampedHeight(float naturalHeight, Texture field)
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ship Wave Field"));
            var input = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var output = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var previous = RenderTexture.active;
            try
            {
                material.SetTexture("_MotuShipWaveField", field);
                material.SetVector("_MotuShipWaveRect", new Vector4(-100, -100, .005f, .005f));
                material.SetFloat("_MotuShipWaveEnabled", 1);
                input.SetPixel(0, 0, new Color(7.5f, 0, naturalHeight, 0));
                input.Apply();
                Graphics.Blit(input, target, material);
                RenderTexture.active = target;
                output.ReadPixels(new Rect(0, 0, 1, 1), 0, 0);
                output.Apply();
                return output.GetPixel(0, 0).r;
            }
            finally
            {
                RenderTexture.active = previous;
                Object.Destroy(material); Object.Destroy(input); Object.Destroy(output); Object.Destroy(target);
            }
        }

        [UnityTest]
        public IEnumerator LiveSprayUsesUnclampedSamplesFollowsTransformedHullAndStopsWhenDry()
        {
            yield return new EnterPlayMode();
            var oceanObject = new GameObject("Spray test ocean");
            var ship = new GameObject("Scaled rotated spray test hull");
            var oceanMaterial = new Material(Shader.Find("Motu/Sea Water"));
            var sprayMaterial = new Material(Shader.Find("Motu/Waterfall Spray Particle"));
            var oldField = Shader.GetGlobalTexture("_MotuShipWaveField");
            var oldRect = Shader.GetGlobalVector("_MotuShipWaveRect");
            var oldEnabled = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var field = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            field.SetPixel(0, 0, Color.red);
            field.Apply();
            OceanSurfaceSampler query = null;
            try
            {
                var ocean = oceanObject.AddComponent<OceanSurfaceController>();
                ocean.Install(oceanMaterial, 128, true);
                var weather = ocean.Weather;
                weather.Enabled = true;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 30, 1, 0);
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0;
                weather.OnshoreWaveEnabled = false;
                ocean.ApplyWaveWeather(weather);
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                // A full ship clamp caps the rendered height at the sea plane.
                Shader.SetGlobalTexture("_MotuShipWaveField", field);
                Shader.SetGlobalVector("_MotuShipWaveRect", new Vector4(-100, -100, .005f, .005f));
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 1);
                ship.transform.SetPositionAndRotation(new Vector3(7.5f, .25f, 0), Quaternion.Euler(-90, 0, 0));
                ship.transform.localScale = Vector3.one * 1500;
                var world = new[] { new Vector3(6.5f,.25f,-1), new Vector3(8.5f,.25f,-1), new Vector3(8.5f,.25f,1), new Vector3(6.5f,.25f,1) };
                var local = new Vector3[world.Length];
                for (var i = 0; i < world.Length; i++) local[i] = ship.transform.InverseTransformPoint(world[i]);
                var spray = ship.AddComponent<OceanHullSpray>();
                spray.SetWaterline(local, sprayMaterial);
                var serialized = new SerializedObject(spray);
                serialized.FindProperty("ocean").objectReferenceValue = ocean;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                query = new OceanSurfaceSampler(128);
                var probes = new Vector3[128];
                for (var i = 0; i < probes.Length; i++) probes[i] = new Vector3(7.5f, .25f, 0);
                var until = Time.time + 2;
                while (Time.time < until) { query.Submit(ocean, probes); yield return null; }
                Assert.IsTrue(query.TryGetHeight(0, probes[0], 1, out var height));
                Assert.That(height, Is.GreaterThan(.8f), "Natural crest survives ship clamping in queries.");
                Assert.That(ClampedHeight(height, field), Is.EqualTo(0).Within(.001f), "The same crest is suppressed by the rendered ship clamp.");
                Assert.That(spray.Particles.particleCount, Is.GreaterThan(5));
                Assert.That(spray.Particles.collision.enabled, Is.False);
                var snapshot = new ParticleSystem.Particle[2500];
                var count = spray.Particles.GetParticles(snapshot);
                var rising = 0;
                for (var i = 0; i < count; i++)
                {
                    Assert.That(Vector3.Distance(snapshot[i].position, ship.transform.position), Is.LessThan(15), "World particles ignore imported mesh scale.");
                    rising += snapshot[i].velocity.y > 0 ? 1 : 0;
                }
                Assert.That(rising, Is.GreaterThan(0));
                Assert.That(spray.Particles.main.maxParticles, Is.EqualTo(2500));
                // Teleport from this crest to a dry trough. Old crest samples must
                // not keep emitting at the new position, even for one fresh frame.
                ship.transform.position += Vector3.right * 15;
                spray.Particles.Clear();
                yield return null;
                Assert.That(spray.Particles.particleCount, Is.Zero);
                until = Time.time + .3f;
                while (Time.time < until) yield return null;
                Assert.That(spray.Particles.particleCount, Is.Zero);
                ship.transform.position -= Vector3.right * 15;
                ship.transform.position += Vector3.up * 20;
                until = Time.time + 1.5f;
                while (Time.time < until) yield return null;
                Assert.That(spray.Particles.particleCount, Is.Zero, "No lingering emission above waves.");
                ship.transform.position -= Vector3.up * 20;
                spray.enabled = false;
                yield return null;
                spray.enabled = true;
                until = Time.time + .5f;
                while (Time.time < until) yield return null;
                Assert.That(spray.Particles.particleCount, Is.GreaterThan(0), "Re-enable resumes simulation and sampling.");
                serialized.Update();
                serialized.FindProperty("maximumEmissionRate").intValue = 30;
                serialized.FindProperty("lifetime").floatValue = 10;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                ship.transform.position -= Vector3.up * 10;
                yield return null;
                spray.Particles.Clear();
                var began = Time.time;
                until = began + 1;
                while (Time.time < until) yield return null;
                Assert.That(spray.Particles.particleCount, Is.InRange(1, Mathf.CeilToInt(30 * (Time.time - began)) + 4),
                    "Even saturated segments respect the ship-wide emission budget.");
                serialized.Update();
                serialized.FindProperty("maximumParticles").intValue = 12;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                yield return null;
                Assert.That(spray.Particles.particleCount, Is.LessThanOrEqualTo(12));
            }
            finally
            {
                query?.Dispose();
                Object.Destroy(ship);
                Object.Destroy(oceanObject);
                Object.Destroy(sprayMaterial);
                Object.Destroy(field);
                Object.Destroy(oceanMaterial);
                Shader.SetGlobalTexture("_MotuShipWaveField", oldField);
                Shader.SetGlobalVector("_MotuShipWaveRect", oldRect);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", oldEnabled);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
            }
            yield return null;
            yield return new ExitPlayMode();
        }
    }
}
