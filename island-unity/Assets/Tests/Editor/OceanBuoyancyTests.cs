using System.Collections;
using Motu.Gameplay;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;

namespace Motu.Editor
{
    public sealed class OceanBuoyancyTests
    {
        [UnityTest]
        public IEnumerator ComponentFloatsUsingAsynchronousSamplesInPlayMode()
        {
            yield return new EnterPlayMode();
            var scene = SceneManager.CreateScene("Live buoyancy test");
            var root = new GameObject("Live ocean");
            SceneManager.MoveGameObjectToScene(root, scene);
            var hull = new GameObject("Floating hull");
            SceneManager.MoveGameObjectToScene(hull, scene);
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            try
            {
                var ocean = root.AddComponent<OceanSurfaceController>();
                ocean.Install(new Material(Shader.Find("Motu/Sea Water")), 128f, true);
                var weather = ocean.Weather;
                weather.Enabled = false;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 30f, .5f, 0f);
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0f;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0f;
                weather.OnshoreWaveEnabled = false;
                ocean.ApplyWaveWeather(weather);
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.grayTexture);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                hull.AddComponent<BoxCollider>().size = new Vector3(2, 1, 4);
                var body = hull.AddComponent<Rigidbody>();
                body.mass = 300000f;
                body.interpolation = RigidbodyInterpolation.Interpolate;
                body.position = Vector3.up;
                var buoyancy = hull.AddComponent<OceanBuoyancy>();
                buoyancy.Ocean = ocean;
                buoyancy.Draft = .5f;
                var until = Time.time + 5f;
                while (Time.time < until) yield return null;
                Assert.IsTrue(buoyancy.enabled);
                Assert.That(body.position.y, Is.EqualTo(0).Within(.05f));
                Assert.That(body.linearVelocity.magnitude, Is.LessThan(.05f));
                // A newly enabled wave must lift the actual component through
                // LateUpdate -> GPU -> callback -> FixedUpdate, without a raycast.
                weather.Enabled = true;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 30f, .5f, 0f);
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0f;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0f;
                weather.OnshoreWaveEnabled = false;
                ocean.ApplyWaveWeather(weather);
                body.position = new Vector3(7.5f, 0f, 0f);
                until = Time.time + 4f;
                while (Time.time < until) yield return null;
                Assert.That(body.position.y, Is.EqualTo(.5f).Within(.06f),
                    $"Hull position {body.position}, rotation {body.rotation.eulerAngles}, velocity {body.linearVelocity}");
            }
            finally
            {
                Object.Destroy(hull);
                Object.Destroy(root);
                SceneManager.UnloadSceneAsync(scene);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
            }
            yield return null;
            yield return new ExitPlayMode();
        }

        [Test]
        public void DelayedSamplesFadeInsteadOfSwitchingOffAtAQuarterSecond()
        {
            Assert.That(OceanSurfaceSampler.SampleConfidence(.249f), Is.EqualTo(1f));
            Assert.That(OceanSurfaceSampler.SampleConfidence(.251f), Is.GreaterThan(.999f));
            var previous = 1f;
            for (var age = .25f; age <= 1.01f; age += .01f)
            {
                var confidence = OceanSurfaceSampler.SampleConfidence(age);
                Assert.That(confidence, Is.InRange(0f, previous));
                Assert.That(previous - confidence, Is.LessThan(.025f));
                previous = confidence;
            }
            Assert.That(OceanSurfaceSampler.SampleConfidence(1.01f), Is.Zero);
        }

        [UnityTest]
        public IEnumerator SteppedWaveSamplesDoNotJerkTheHull()
        {
            yield return new EnterPlayMode();
            var scene = SceneManager.CreateScene("Buoyancy jitter test",
                new CreateSceneParameters(LocalPhysicsMode.Physics3D));
            try
            {
                var original = MeasureJitter(scene, 0f, true);
                var raw = MeasureJitter(scene, 0f);
                var smooth = MeasureJitter(scene, .15f);
                Debug.Log($"Buoyancy full-size regression: angular jerk RMS {original.x:F4} (old force mode) -> {raw.x:F4} (physical forces) -> {smooth.x:F4} (smoothed samples); heave range {raw.y:F3} -> {smooth.y:F3} m.");
                Assert.That(raw.x, Is.LessThan(original.x * .01f), "Probe torque must respect the full-size hull inertia.");
                Assert.That(smooth.x, Is.LessThan(raw.x * .6f), "Remove readback torque steps.");
                Assert.That(smooth.y, Is.GreaterThan(raw.y * .75f), "Keep the response to broad waves.");
            }
            finally { SceneManager.UnloadSceneAsync(scene); }
            yield return new ExitPlayMode();
        }

        private static Vector2 MeasureJitter(Scene scene, float smoothingSeconds, bool originalForceMode = false)
        {
            var hull = new GameObject("30 metre hull");
            SceneManager.MoveGameObjectToScene(hull, scene);
            try
            {
                hull.AddComponent<BoxCollider>().size = new Vector3(30f, 4.1f, 7.1f);
                var body = hull.AddComponent<Rigidbody>();
                body.mass = 300000f;
                body.position = new Vector3(0, -1.025f, 0);
                var filters = new OceanBuoyancy.WaterSampleFilter[6];
                var probes = new Vector3[6];
                for (var row = 0; row < 3; row++)
                for (var side = 0; side < 2; side++)
                    probes[row * 2 + side] = new Vector3((row - 1) * 9.75f, -1.025f, side == 0 ? -2.3f : 2.3f);
                var previousVelocity = Vector3.zero;
                var previousAcceleration = Vector3.zero;
                var jerkSquared = 0f;
                var minimum = float.PositiveInfinity;
                var maximum = float.NegativeInfinity;
                for (var step = 0; step < 1500; step++)
                {
                    // Independent side samples delivered at 10 Hz, including a
                    // short ripple superimposed on a six-second broad swell.
                    var sampleStep = step / 5;
                    var broad = .35f * Mathf.Sin(sampleStep * .1f * Mathf.PI / 3f);
                    var ripple = sampleStep % 2 == 0 ? .08f : -.08f;
                    for (var i = 0; i < probes.Length; i++)
                    {
                        filters[i].Update(broad + (i % 2 == 0 ? ripple : -ripple), 1f, .02f, smoothingSeconds);
                        var point = body.position + body.rotation * probes[i];
                        var acceleration = OceanBuoyancy.ProbeAcceleration(filters[i].Height - point.y,
                            body.GetPointVelocity(point), -Physics.gravity.y, 2.05f, 2f, .7f, .5f, .02f);
                        acceleration *= filters[i].Confidence / probes.Length;
                        if (originalForceMode) body.AddForceAtPosition(acceleration, point, ForceMode.Acceleration);
                        else OceanBuoyancy.ApplyProbeAcceleration(body, acceleration, point);
                    }
                    scene.GetPhysicsScene().Simulate(.02f);
                    var angularAcceleration = (body.angularVelocity - previousVelocity) / .02f;
                    if (step >= 500)
                    {
                        jerkSquared += ((angularAcceleration - previousAcceleration) / .02f).sqrMagnitude;
                        minimum = Mathf.Min(minimum, body.position.y);
                        maximum = Mathf.Max(maximum, body.position.y);
                    }
                    previousVelocity = body.angularVelocity;
                    previousAcceleration = angularAcceleration;
                }
                return new Vector2(Mathf.Sqrt(jerkSquared / 1000f), maximum - minimum);
            }
            finally { Object.DestroyImmediate(hull); }
        }

        [Test]
        public void DryProbesDoNotApplyForcesAndSubmergedForcesAreBounded()
        {
            Assert.AreEqual(Vector3.zero, OceanBuoyancy.ProbeAcceleration(-1f,
                Vector3.one * 100f, 9.81f, .5f, 2f, .7f, .5f, .02f));
            var force = OceanBuoyancy.ProbeAcceleration(100f,
                Vector3.down * 100f, 9.81f, .5f, 2f, .7f, .5f, .02f);
            Assert.That(force.y, Is.EqualTo(19.62f).Within(.001f));
        }

        [UnityTest]
        public IEnumerator BodiesOfDifferentMassSettleAndLevelInStillWater()
        {
            yield return new EnterPlayMode();
            var scene = SceneManager.CreateScene("Buoyancy physics test",
                new CreateSceneParameters(LocalPhysicsMode.Physics3D));
            var physics = scene.GetPhysicsScene();
            try
            {
                foreach (var mass in new[] { 1f, 1000f })
                {
                    var hull = new GameObject("Hull");
                    SceneManager.MoveGameObjectToScene(hull, scene);
                    hull.AddComponent<BoxCollider>().size = new Vector3(2f, 1f, 4f);
                    var body = hull.AddComponent<Rigidbody>();
                    body.mass = mass;
                    body.position = new Vector3(0, 1.5f, 0);
                    body.rotation = Quaternion.Euler(8f, 0f, 6f);
                    var probes = new[] { new Vector3(-.8f, -.5f, -1.6f), new Vector3(.8f, -.5f, -1.6f),
                        new Vector3(-.8f, -.5f, 1.6f), new Vector3(.8f, -.5f, 1.6f) };
                    for (var step = 0; step < 600; step++)
                    {
                        foreach (var probe in probes)
                        {
                            var p = body.position + body.rotation * probe;
                            var acceleration = OceanBuoyancy.ProbeAcceleration(-p.y, body.GetPointVelocity(p),
                                -Physics.gravity.y, .5f, 2f, .7f, .5f, .02f);
                            OceanBuoyancy.ApplyProbeAcceleration(body, acceleration / probes.Length, p);
                        }
                        physics.Simulate(.02f);
                    }
                    Assert.That(body.position.y, Is.EqualTo(0f).Within(.025f), $"Waterline for mass {mass}");
                    Assert.That(Vector3.Dot(body.rotation * Vector3.up, Vector3.up), Is.GreaterThan(.999f));
                    Assert.That(body.linearVelocity.magnitude, Is.LessThan(.03f));
                    Object.DestroyImmediate(hull);
                }
            }
            finally { SceneManager.UnloadSceneAsync(scene); }
            yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator GpuQueriesMatchWaveHeightChoppinessDepthAndWeather()
        {
            Assert.IsTrue(OceanSurfaceSampler.Supported);
            var root = new GameObject("Query ocean");
            root.transform.position = new Vector3(0, 3, 0);
            var ocean = root.AddComponent<OceanSurfaceController>();
            var material = new Material(Shader.Find("Motu/Sea Water"));
            var noise = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            noise.SetPixel(0, 0, new Color(.75f, .5f, 0, 1)); noise.Apply();
            var coast = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            coast.SetPixel(0, 0, new Color(.5f, 1, .05f, 1)); coast.Apply(); // Half-metre depth in the 10 m mask.
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var oldOffset = Shader.GetGlobalVector("_MotuWindOffset");
            OceanSurfaceSampler sampler = null;
            try
            {
                Shader.SetGlobalTexture("_MotuWindNoise", noise);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalVector("_MotuWindOffset", Vector4.zero);
                ocean.Install(material, 128f, true);
                var weather = ocean.Weather;
                weather.Wave0 = new OceanWaveComponent(Vector2.right, 30f, 1f, 0f);
                weather.Wave0.Choppiness = 0f;
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0f;
                weather.OnshoreWaveEnabled = false;
                weather.OnshoreWaveChoppiness = 0f;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0f;
                ocean.ApplyWaveWeather(weather);
                sampler = new OceanSurfaceSampler(2);
                var positions = new[] { new Vector3(7.5f, 0, 0), new Vector3(22.5f, 0, 0) };
                yield return Query(sampler, ocean, positions, new[] { 4f, 2f });
                // Deck rendering must not flatten the wave sampled by its own probes.
                var deck = root.AddComponent<Motu.Rendering.OceanDeckWaveClamp>();
                deck.Configure(Vector3.zero, Vector3.right * 30, 5, 2);
                deck.ConfigureTextures(Texture2D.blackTexture, null);
                Motu.Rendering.OceanShipWaveField.Render(new[] { deck }, Vector3.zero, 0);
                yield return Query(sampler, ocean, positions, new[] { 4f, 2f });
                ocean.SetWaveAttenuation(coast, Texture2D.blackTexture, new Vector4(0, -10, .01f, .05f));
                yield return Query(sampler, ocean, positions, new[] { 3.25f, 2.75f });
                ocean.SetWaveAttenuation(Texture2D.whiteTexture, Texture2D.blackTexture, new Vector4(-1, -1, .5f, .5f));
                weather.AmplitudeVariation = .6f;
                ocean.ApplyWaveWeather(weather);
                yield return Query(sampler, ocean, positions, new[] { 4.3f, 1.7f });
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 18, 2));
                yield return Query(sampler, ocean, positions, new[] { 5.6f, .4f });
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                weather.AmplitudeVariation = 0f;
                weather.Wave0.Choppiness = .8f;
                ocean.ApplyWaveWeather(weather);
                var displacementX = .8f * Mathf.Cos(Mathf.PI / 4f);
                positions[0] = new Vector3(3.75f + displacementX, 0, 0);
                positions[1] = new Vector3(18.75f - displacementX, 0, 0);
                yield return Query(sampler, ocean, positions,
                    new[] { 3f + Mathf.Sin(Mathf.PI / 4f), 3f - Mathf.Sin(Mathf.PI / 4f) });
                Assert.IsFalse(sampler.TryGetHeight(0, positions[0] + Vector3.right * 100f, .5f, out _), "Teleport must invalidate old samples.");
                Assert.IsFalse(ShaderUtil.ShaderHasError(Resources.Load<Shader>("OceanSurfaceQuery")));
                // Releasing a component during a pending readback must not release
                // the GPU texture prematurely or publish results after disposal.
                sampler.Submit(ocean, positions);
                sampler.Dispose();
                while (sampler.Pending) yield return null;
                Assert.IsFalse(sampler.TryGetHeight(0, positions[0], .5f, out _));
            }
            finally
            {
                sampler?.Dispose();
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(noise);
                Object.DestroyImmediate(coast);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalVector("_MotuWindOffset", oldOffset);
            }
        }

        private static IEnumerator Query(OceanSurfaceSampler sampler, OceanSurfaceController ocean,
            Vector3[] positions, float[] expected)
        {
            sampler.Submit(ocean, positions);
            var deadline = Time.realtimeSinceStartup + 10f;
            while (sampler.Pending && Time.realtimeSinceStartup < deadline) yield return null;
            Assert.IsFalse(sampler.Pending, "GPU query timed out.");
            for (var i = 0; i < expected.Length; i++)
            {
                Assert.IsTrue(sampler.TryGetHeight(i, positions[i], .5f, out var height), $"Missing sample {i}");
                Assert.That(height, Is.EqualTo(expected[i]).Within(.005f), $"Sample {i}");
            }
        }
    }
}
