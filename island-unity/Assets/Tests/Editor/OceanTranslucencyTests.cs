using System;
using System.IO;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanTranslucencyTests
    {
        [Test]
        public void CurvatureFollowsLocalCrestsSharpnessAndTransitionPhases()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var wind = Shader.GetGlobalVector("_MotuWeatherWind");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                material.SetFloat("_ProbeCurvature", 1);
                material.SetFloat("_WaveDomainWarp", 0);
                material.SetFloat("_WaveAmplitudeVariation", 0);
                material.SetVector("_ProbeWorldRect", new Vector4(0, 0, 12, 0));
                for (var i = 0; i < 4; i++)
                {
                    material.SetVector($"_OceanWave{i}", new Vector4(1, 0, 12, i == 0 ? 1 : 0));
                    material.SetVector($"_OceanWaveFrom{i}", new Vector4(1, 0, 12, 0));
                    material.SetVector($"_OceanWaveTo{i}", new Vector4(1, 0, 12, 0));
                }
                material.SetVector("_OceanWaveChoppiness", new Vector4(.7f, 0, 0, 0));
                var samples = ReadProbe(material);
                const float step = 12f / 512;
                for (var i = 1; i < samples.Length - 1; i++)
                {
                    var measured = -(samples[i + 1].r - 2 * samples[i].r + samples[i - 1].r) / (step * step);
                    Assert.That(samples[i].a, Is.EqualTo(measured).Within(.005f), "Curvature must match the rendered wave formula.");
                }
                material.SetVector("_ProbeWorldRect", Vector4.zero);
                material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 12, Mathf.PI * .5f));
                material.SetVector("_OceanWaveChoppiness", Vector4.zero);
                var rounded = ReadProbe(material)[0];
                material.SetVector("_OceanWaveChoppiness", new Vector4(.8f, 0, 0, 0));
                Assert.That(ReadProbe(material)[0].a, Is.GreaterThan(rounded.a * 1.6f));
                material.SetVector("_OceanWaveChoppiness", Vector4.zero);
                material.SetVector("_OceanWaveTo0", new Vector4(1, 0, 12, -Mathf.PI * .5f));
                material.SetFloat("_OceanWaveTransition", .5f);
                Assert.That(ReadProbe(material)[0].a, Is.EqualTo(0).Within(.00001f), "Opposite phases must cancel smoothly.");
                material.SetFloat("_OceanWaveTransition", 0);
                material.SetVector("_OceanWave0", new Vector4(1, 0, 120, 2));
                material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 120, -Mathf.PI * .5f));
                material.SetVector("_OceanWave1", new Vector4(1, 0, 12, .3f));
                material.SetVector("_OceanWaveFrom1", new Vector4(1, 0, 12, Mathf.PI * .5f));
                var lowCrest = ReadProbe(material)[0];
                Assert.That(lowCrest.r, Is.LessThan(-1), "Small crest rides in a deep broad trough.");
                Assert.That(lowCrest.a, Is.GreaterThan(.05f), "A local crest below sea level still has positive raw curvature.");
                material.SetVector("_OceanWaveFrom1", new Vector4(1, 0, 12, -Mathf.PI * .5f));
                Assert.That(ReadProbe(material)[0].a, Is.LessThan(0), "A true local trough must not scatter.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", wind);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void HullCeilingUsesRenderedDepthLimitEvenWithLargeWeatherWaves()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var wind = Shader.GetGlobalVector("_MotuWeatherWind");
            var shallow = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            try
            {
                material.SetFloat("_ProbeClampEnvelope", 1);
                material.SetVector("_WaveAttenuationWorldRect", new Vector4(-1, -1, .5f, .5f));
                material.SetTexture("_WaveAttenuationTex", Texture2D.whiteTexture);
                material.SetVector("_OceanWave0", new Vector4(1, 0, 30, 20));
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Assert.That(ReadProbe(material)[0].r, Is.EqualTo(10).Within(.0001f));
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 10));
                Assert.That(ReadProbe(material)[0].r, Is.EqualTo(10).Within(.0001f), "Weather must not steepen the hull rim after depth has capped the rendered waves.");
                shallow.SetPixel(0, 0, new Color(1, 1, .2f, 1));
                shallow.Apply();
                material.SetTexture("_WaveAttenuationTex", shallow);
                Assert.That(ReadProbe(material)[0].r, Is.EqualTo(2).Within(.0001f));
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, .01f));
                Assert.That(ReadProbe(material)[0].r, Is.LessThan(.3f), "Smaller waves should still use their actual envelope.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", wind);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(shallow);
            }
        }

        [Test]
        public void HeightTranslucencyUsesLargestProfilePeakAndSoftSeaLevelFade()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var wind = Shader.GetGlobalVector("_MotuWeatherWind");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                material.SetFloat("_ProbeHeightResponse", 1);
                material.SetFloat("_WaveAmplitudeVariation", 0);
                material.SetFloat("_OnshoreWaveEnabled", 0);
                for (var i = 0; i < 4; i++)
                    material.SetVector($"_OceanWave{i}", new Vector4(1, 0, 30, 0));
                material.SetVector("_OceanWave0", new Vector4(1, 0, 30, 2));
                material.SetVector("_OceanWave1", new Vector4(1, 0, 120, .3f));
                Color AtHeight(float height)
                {
                    material.SetFloat("_ProbeHeight", height);
                    return ReadProbe(material)[0];
                }
                Assert.That(AtHeight(1).r, Is.EqualTo(.5f).Within(.0001f), "Use the largest peak, not summed heights or longest wavelength.");
                Assert.That(AtHeight(2).r, Is.EqualTo(1).Within(.0001f));
                Assert.That(AtHeight(4).r, Is.EqualTo(1).Within(.0001f));
                Assert.That(AtHeight(-.5f).r, Is.EqualTo(0).Within(.0001f), "Ripples below sea level should not glow.");
                Assert.That(AtHeight(0).r, Is.EqualTo(0).Within(.0001f));
                Assert.That(AtHeight(.01f).r, Is.LessThan(.0001f), "Feather the sea-level edge gently.");
                material.SetVector("_OceanWave3", new Vector4(1, 0, 4, 4));
                Assert.That(AtHeight(1).r, Is.EqualTo(.25f).Within(.0001f));
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 2));
                Assert.That(AtHeight(2).r, Is.EqualTo(.25f).Within(.0001f), "Reference peak must follow weather-scaled height.");
                material.SetFloat("_WaveAmplitudeVariation", .5f);
                Assert.That(AtHeight(3).g, Is.EqualTo(12).Within(.0001f), "Include the configured noise amplitude allowance.");
                material.SetVector("_OceanWaveChoppiness", new Vector4(0, 0, 0, 1));
                Assert.That(AtHeight(3).g, Is.EqualTo(14.64f).Within(.001f));
                material.SetFloat("_OnshoreWaveEnabled", 1);
                material.SetVector("_OnshoreWaveParameters", new Vector4(30, 10, 1, 0));
                Assert.That(AtHeight(3).g, Is.EqualTo(20).Within(.001f), "Include the shore wave when it is the largest enabled component.");
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 0));
                Assert.That(AtHeight(3).r, Is.EqualTo(0).Within(.0001f));
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", wind);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void BreakingWaveCurvatureMatchesItsCompressedLeadingFace()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Shore Distance Probe"));
            try
            {
                material.SetFloat("_ProbeBreakerShape", 1);
                material.SetFloat("_ProbeCurvature", 1);
                material.SetFloat("_ProbeCoastDistance", 20);
                material.SetFloat("_ProbeWaterDepth", 4);
                material.SetVector("_OnshoreWaveParameters", new Vector4(12, 1, 1, .5f));
                material.SetVector("_OnshoreWaveBreaking", new Vector4(.95f, 96, 5, 3.5f));
                var samples = ReadProbe(material);
                const float step = 12f / 512;
                var checkedSamples = 0;
                for (var i = 1; i < samples.Length - 1; i++)
                {
                    // At the join between the compressed front and rear faces,
                    // curvature is discontinuous; compare the smooth sections.
                    if (Mathf.Abs(samples[i + 1].a - samples[i - 1].a) > .1f + Mathf.Abs(samples[i].a) * .1f) continue;
                    var measured = -(samples[i + 1].r - 2 * samples[i].r + samples[i - 1].r) / (step * step);
                    Assert.That(samples[i].a, Is.EqualTo(measured).Within(.02f + Mathf.Abs(measured) * .02f));
                    checkedSamples++;
                }
                Assert.That(checkedSamples, Is.GreaterThan(350));
            }
            finally { Object.DestroyImmediate(material); }
        }

        private static Color[] ReadProbe(Material material)
        {
            var target = new RenderTexture(512, 1, 0, RenderTextureFormat.ARGBFloat);
            var pixels = new Texture2D(512, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                Graphics.Blit(Texture2D.blackTexture, target, material);
                RenderTexture.active = target;
                pixels.ReadPixels(new Rect(0, 0, 512, 1), 0, 0);
                pixels.Apply();
                return pixels.GetPixels();
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(pixels);
            }
        }

        [Test]
        public void CrestsScatterLightMostWhenBacklitAndStayDarkWithoutLight()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var shader = Shader.Find("Motu/Sea Water");
            Assert.IsNotNull(shader);
            var material = new Material(shader);
            var defaultStrength = material.GetFloat("_WaveTranslucencyStrength");
            var water = new GameObject("Translucent ocean validation");
            var cameraObject = new GameObject("Ocean validation camera");
            var sunObject = new GameObject("Ocean validation sun");
            var mesh = Grid();
            var target = new RenderTexture(800, 500, 24, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(800, 500, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            var previousClouds = Shader.GetGlobalFloat("_MotuCloudEnabled");
            var previousShips = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
            var previousCapsules = Shader.GetGlobalInt("_MotuDeckCapsuleCount");
            var previousWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var previousAmbient = RenderSettings.ambientLight;
            var previousMode = RenderSettings.ambientMode;
            var previousFog = RenderSettings.fog;
            try
            {
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", 0);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(0, 1, 9, 1));
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = Color.black;
                RenderSettings.fog = false;
                water.AddComponent<MeshFilter>().sharedMesh = mesh;
                water.AddComponent<MeshRenderer>().sharedMaterial = material;
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.transform.position = new Vector3(0, 3, -20);
                camera.transform.LookAt(new Vector3(0, 0, 12));
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = new Color(.35f, .52f, .65f);
                camera.depthTextureMode = DepthTextureMode.Depth;
                camera.farClipPlane = 200;
                camera.targetTexture = target;
                var sun = sunObject.AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 1.25f;
                sun.shadows = LightShadows.None;
                RenderSettings.sun = sun;
                material.SetTexture("_NoiseTex", Texture2D.grayTexture);
                material.SetFloat("_WaveDomainWarp", 0);
                material.SetFloat("_WaveAmplitudeVariation", 0);
                material.SetFloat("_OnshoreWaveEnabled", 0);
                material.SetFloat("_WhitecapStrength", 0);
                material.SetFloat("_ReflectionStrength", 0);
                material.SetFloat("_SunGlintStrength", 0);
                material.SetFloat("_ShallowOpacity", 1);
                for (var i = 0; i < 4; i++)
                {
                    material.SetVector($"_OceanWave{i}", new Vector4(0, 1, 12, i == 0 ? 1.2f : 0));
                    material.SetVector($"_OceanWaveFrom{i}", new Vector4(0, 1, 12, 0));
                    material.SetVector($"_OceanWaveTo{i}", new Vector4(0, 1, 12, 0));
                }
                material.SetVector("_OceanWaveChoppiness", new Vector4(.3f, 0, 0, 0));
                Color[] Capture(float strength, string filename = null)
                {
                    material.SetFloat("_WaveTranslucencyStrength", strength);
                    camera.Render();
                    RenderTexture.active = target;
                    readback.ReadPixels(new Rect(0, 0, target.width, target.height), 0, 0);
                    readback.Apply();
                    if (filename != null && Environment.GetEnvironmentVariable("MOTU_OCEAN_TRANSLUCENCY_IMAGES") is string directory)
                    {
                        Directory.CreateDirectory(directory);
                        var png = new Texture2D(target.width, target.height, TextureFormat.RGB24, false);
                        png.SetPixels(readback.GetPixels());
                        png.Apply();
                        File.WriteAllBytes(Path.Combine(directory, filename), png.EncodeToPNG());
                        Object.DestroyImmediate(png);
                    }
                    return readback.GetPixels();
                }
                float Contribution()
                {
                    var before = Capture(0);
                    var after = Capture(defaultStrength);
                    var total = 0f;
                    for (var i = 0; i < after.Length; i++)
                    {
                        Assert.IsTrue(float.IsFinite(after[i].g));
                        total += Mathf.Max(0, after[i].g - before[i].g);
                    }
                    return total / after.Length;
                }
                sun.transform.rotation = Quaternion.LookRotation(new Vector3(0, -.35f, -1));
                // Ambient scattering stays small compared with direct backlighting.
                RenderSettings.ambientLight = new Color(.15f, .18f, .2f);
                var backlit = Contribution();
                var fadeStart = material.GetFloat("_WaveFadeStart");
                var fadeEnd = material.GetFloat("_WaveFadeEnd");
                material.SetFloat("_WaveFadeStart", .1f);
                material.SetFloat("_WaveFadeEnd", .2f);
                var distant = Contribution();
                Assert.That(distant, Is.GreaterThan(backlit * .25f),
                    "The flat distant mesh must retain translucency from its analytic wave crests.");
                Capture(defaultStrength, "ocean-distant-translucency.png");
                // Analytic waves remain large here, but the rendered vertices
                // are flat. Even a texture full of old foam must stay invisible.
                var noFoam = Capture(0);
                material.SetFloat("_WhitecapStrength", 1);
                material.SetFloat("_PersistentFoamStrength", 1);
                material.SetTexture("_OceanFoamHistory", Texture2D.whiteTexture);
                material.SetVector("_OceanFoamHistoryRect", new Vector4(-64, -64, 1f / 128, 1f / 128));
                var flatWithFoam = Capture(0, "flat-ocean-foam-rejected.png");
                for (var i = 0; i < flatWithFoam.Length; i++)
                    Assert.That(Mathf.Abs(flatWithFoam[i].r - noFoam[i].r)
                        + Mathf.Abs(flatWithFoam[i].g - noFoam[i].g)
                        + Mathf.Abs(flatWithFoam[i].b - noFoam[i].b), Is.LessThan(.001f),
                        "No white foam may appear where the rendered wave height is zero.");
                material.SetFloat("_WhitecapStrength", 0);
                material.SetFloat("_PersistentFoamStrength", 0);
                material.SetTexture("_OceanFoamHistory", Texture2D.blackTexture);
                material.SetFloat("_WaveFadeStart", fadeStart);
                material.SetFloat("_WaveFadeEnd", fadeEnd);
                var originalPosition = camera.transform.position;
                var originalRotation = camera.transform.rotation;
                camera.transform.position = new Vector3(-20, 3, 0);
                camera.transform.LookAt(Vector3.zero);
                var sideView = Contribution();
                camera.transform.position = new Vector3(-14, 3, -14);
                camera.transform.LookAt(Vector3.zero);
                var broadOblique = Contribution();
                var defaultFalloff = material.GetFloat("_WaveTranslucencyFalloff");
                material.SetFloat("_WaveTranslucencyFalloff", 8);
                var narrowOblique = Contribution();
                material.SetFloat("_WaveTranslucencyFalloff", defaultFalloff);
                Assert.That(broadOblique, Is.GreaterThan(narrowOblique * 1.3f),
                    "The broader sun lobe must brighten oblique views.");
                camera.transform.position = new Vector3(0, 30, 0);
                camera.transform.rotation = Quaternion.LookRotation(Vector3.down, Vector3.forward);
                var overhead = Contribution();
                camera.transform.SetPositionAndRotation(originalPosition, originalRotation);
                sun.transform.rotation = Quaternion.LookRotation(new Vector3(0, -.35f, 1));
                var frontlit = Contribution();
                Assert.That(backlit, Is.GreaterThan(.0005f), "Backlit crests need a measurable scattering contribution.");
                Assert.That(frontlit, Is.GreaterThan(.00001f), "Ambient light should gently illuminate crests away from the sun.");
                Assert.That(frontlit, Is.LessThan(backlit * .15f), "Ambient scattering must stay small compared with sunlight.");
                Assert.That(sideView, Is.LessThan(backlit * .3f), "Moving sideways must strongly reduce transmission.");
                Assert.That(overhead, Is.LessThan(backlit * .15f), "Looking down must strongly reduce transmission.");
                material.SetFloat("_GeometricWaves", 0);
                Assert.That(Contribution(), Is.LessThan(.00001f), "Flat sea has no glowing crests.");
                material.SetFloat("_GeometricWaves", 1);
                sun.intensity = 0;
                var ambientOnly = Contribution();
                Assert.That(ambientOnly, Is.GreaterThan(.00001f), "Sky light should scatter without direct sunlight.");
                Assert.That(ambientOnly, Is.LessThan(backlit * .15f), "Sky scattering should remain a small fill.");
                var defaultAmbient = material.GetFloat("_WaveTranslucencyAmbient");
                material.SetFloat("_WaveTranslucencyAmbient", 0);
                Assert.That(Contribution(), Is.LessThan(.00001f), "Ambient scattering can be disabled independently.");
                material.SetFloat("_WaveTranslucencyAmbient", defaultAmbient);
                RenderSettings.ambientLight = Color.black;
                Assert.That(Contribution(), Is.LessThan(.00001f), "Scattering is not an unlit emission.");
                sun.intensity = 1.25f;
                sun.transform.rotation = Quaternion.LookRotation(new Vector3(0, -.35f, -1));
                material.SetFloat("_ReflectionStrength", .65f);
                material.SetFloat("_SunGlintStrength", .8f);
                RenderSettings.ambientLight = new Color(.08f, .10f, .12f);
                material.SetVector("_OceanWave1", new Vector4(.6f, .8f, 7, .28f));
                material.SetVector("_OceanWaveFrom1", new Vector4(.6f, .8f, 7, .7f));
                material.SetVector("_OceanWaveTo1", new Vector4(.6f, .8f, 7, .7f));
                Capture(0, "ocean-before.png");
                Capture(defaultStrength, "ocean-after.png");
                Assert.IsFalse(ShaderUtil.ShaderHasError(shader));
                Debug.Log($"Ocean translucency: backlit={backlit:F5}, frontlit={frontlit:F5}, side={sideView:F5}, overhead={overhead:F5}, ambient={ambientOnly:F5}, oblique broad={broadOblique:F5}, narrow={narrowOblique:F5}");
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Shader.SetGlobalFloat("_MotuCloudEnabled", previousClouds);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", previousShips);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", previousCapsules);
                Shader.SetGlobalVector("_MotuWeatherWind", previousWind);
                RenderSettings.ambientLight = previousAmbient;
                RenderSettings.ambientMode = previousMode;
                RenderSettings.fog = previousFog;
                Object.DestroyImmediate(water);
                Object.DestroyImmediate(cameraObject);
                Object.DestroyImmediate(sunObject);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(mesh);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(readback);
            }
        }

        private static Mesh Grid()
        {
            const int cells = 128;
            var vertices = new Vector3[(cells + 1) * (cells + 1)];
            var triangles = new int[cells * cells * 6];
            for (var z = 0; z <= cells; z++)
                for (var x = 0; x <= cells; x++)
                    vertices[z * (cells + 1) + x] = new Vector3(x * .5f - 32, 0, z * .5f - 32);
            var index = 0;
            for (var z = 0; z < cells; z++)
                for (var x = 0; x < cells; x++)
                {
                    var a = z * (cells + 1) + x;
                    foreach (var vertex in new[] { a, a + cells + 1, a + 1, a + 1, a + cells + 1, a + cells + 2 })
                        triangles[index++] = vertex;
                }
            var mesh = new Mesh { vertices = vertices, triangles = triangles };
            mesh.RecalculateBounds();
            mesh.bounds = new Bounds(mesh.bounds.center, mesh.bounds.size + Vector3.up * 10);
            return mesh;
        }
    }
}
