using System.Collections;
using System.IO;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanOpticsTests
    {
        [Test]
        public void ReflectionsBlendAtEveryViewportEdgeInsteadOfSwitchingAbruptly()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Optics"));
            try
            {
                material.SetFloat("_ProbeMode", 7);
                material.SetTexture("_PlanarReflectionTexture", Texture2D.whiteTexture);
                material.SetMatrix("_PlanarReflectionMatrix", Matrix4x4.identity);
                material.SetFloat("_PlanarReflectionAvailable", 1);
                material.SetFloat("_PlanarReflectionWeight", 1);
                material.SetFloat("_PlanarReflectionDistortion", 1);
                material.SetFloat("_ReflectionStrength", 1);
                material.SetFloat("_WaterSkyExposure", 1);
                material.SetColor("_ReflectionHorizonColor", Color.black);
                material.SetColor("_ReflectionColor", Color.black);
                material.SetFloat("_SunGlintStrength", 0);
                foreach (var distortion in new[] { Vector2.zero, new Vector2(.02f, -.015f) })
                foreach (var direction in new[] { Vector2.left, Vector2.right, Vector2.up, Vector2.down })
                {
                    material.SetVector("_ProbeLight", new Vector4(distortion.x, distortion.y, 0, 0));
                    var previous = 1f;
                    for (var step = 0; step <= 26; step++)
                    {
                        var uv = Vector2.one * .5f + direction * (.4f + step * .005f);
                        material.SetVector("_ProbeView", new Vector4(uv.x - distortion.x, uv.y - distortion.y, 1, 0));
                        var colour = Read(material, 1)[0].r;
                        Assert.That(colour, Is.InRange(-.001f, 1.001f));
                        Assert.That(colour, Is.LessThanOrEqualTo(previous + .001f));
                        Assert.That(previous - colour, Is.LessThan(.15f), "No hard reflection/sky switch at the viewport edge.");
                        if (step == 0) Assert.That(colour, Is.EqualTo(1).Within(.001f), "Interior reflections remain full strength.");
                        if (step >= 20) Assert.That(colour, Is.Zero.Within(.001f), "No clamped reflection streak outside the texture.");
                        previous = colour;
                    }
                }
            }
            finally { Object.DestroyImmediate(material); }
        }

        [Test]
        public void CoastalTintFadesWithDepthDistanceAndMaskBounds()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Optics"));
            var mask = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            try
            {
                material.SetFloat("_ProbeMode", 4);
                material.SetFloat("_CoastalOpacity", .16f);
                material.SetTexture("_WaveAttenuationTex", mask);
                material.SetVector("_WaveAttenuationWorldRect", new Vector4(-64, -64, 1f / 128, 1f / 128));
                material.SetVector("_ProbeView", Vector4.zero);
                foreach (var depth in new[] { 0f, 5f, 10f })
                foreach (var shoreDistance in new[] { 0f, 8f, 16f, 128f })
                {
                    mask.SetPixel(0, 0, new Color(1, shoreDistance / 128, depth / 10, 1));
                    mask.Apply();
                    var expected = .16f * (1 - depth / 10)
                        * Mathf.Lerp(1, .35f, Mathf.Clamp01(shoreDistance / 16));
                    Assert.That(Read(material, 1)[0].r, Is.EqualTo(expected).Within(.0001f));
                }
                mask.SetPixel(0, 0, new Color(1, 0, 0, 1));
                mask.Apply();
                material.SetVector("_ProbeView", new Vector4(52, 0, 0, 0));
                Assert.That(Read(material, 1)[0].r, Is.EqualTo(.08f).Within(.0001f));
                foreach (var x in new[] { 64f, 80f, -64f, -80f })
                {
                    material.SetVector("_ProbeView", new Vector4(x, 0, 0, 0));
                    Assert.That(Read(material, 1)[0].r, Is.Zero.Within(.0001f));
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(mask);
            }
        }

        [Test]
        public void CalmAndRiverSuppressedSeaRejectsNewAndStoredFoam()
        {
            var probe = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var history = new Material(Resources.Load<Shader>("OceanFoamHistory"));
            var mask = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var oldShips = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
            var oldCapsules = Shader.GetGlobalInt("_MotuDeckCapsuleCount");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.whiteTexture);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", 0);
                foreach (var material in new[] { probe, history })
                {
                    material.SetFloat("_GeometricWaves", 1);
                    material.SetFloat("_WaveFadeStart", 100);
                    material.SetFloat("_WaveFadeEnd", 200);
                    material.SetFloat("_WhitecapHeightThreshold", .5f);
                    material.SetFloat("_WhitecapCoverage", 1);
                    material.SetFloat("_WhitecapStrength", 1);
                    material.SetTexture("_WaveAttenuationTex", mask);
                    material.SetVector("_WaveAttenuationWorldRect", new Vector4(-8, -8, 1f / 16, 1f / 16));
                    material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 32, Mathf.PI * .5f));
                }
                probe.SetFloat("_ProbeFoamActivity", 1);
                history.SetTexture("_FoamPrevious", Texture2D.whiteTexture);
                history.SetVector("_FoamPreviousRect", new Vector4(-8, -8, 1f / 16, 1f / 16));
                history.SetVector("_FoamCurrentRect", new Vector4(-8, -8, 1f / 16, 1f / 16));
                history.SetFloat("_FoamHistoryValid", 1);
                history.SetFloat("_FoamDeltaTime", .1f);
                history.SetFloat("_FoamLifetime", 6);
                history.SetFloat("_FoamDepositRate", 2);
                foreach (var amplitude in new[] { 0f, .02f, .06f, .175f, 1f })
                foreach (var riverAllowance in new[] { 0f, .01f, .05f, 1f })
                {
                    mask.SetPixel(0, 0, new Color(riverAllowance, 1, 1, riverAllowance));
                    mask.Apply();
                    foreach (var material in new[] { probe, history })
                        material.SetVector("_OceanWave0", new Vector4(1, 0, 32, amplitude));
                    var source = Read(probe)[0];
                    var stored = Read(history)[32 * 64 + 32].r;
                    if (amplitude <= .10f || riverAllowance <= .02f)
                    {
                        Assert.That(source.r, Is.Zero, "Tiny waves or flattened river channels must not generate whitecaps.");
                        Assert.That(source.g, Is.Zero);
                        Assert.That(stored, Is.Zero, "Previously stored patches must clear in flattened water.");
                    }
                    else if (amplitude == 1 && riverAllowance == 1)
                    {
                        Assert.That(source.r, Is.GreaterThan(.1f));
                        Assert.That(source.g, Is.EqualTo(1));
                        Assert.That(stored, Is.GreaterThan(.8f), "Active seas must retain foam history.");
                    }
                    else if (amplitude == .175f && riverAllowance == 1)
                        Assert.That(source.g, Is.InRange(.1f, .9f), "The small-wave cutoff must blend smoothly.");
                }
                // Nearby hull clamps still control translucency, while the far
                // flat surface uses the analytic crest at this same wave phase.
                probe.SetFloat("_ProbeFoamActivity", 0);
                probe.SetFloat("_ProbeSurfaceTranslucency", 1);
                probe.SetFloat("_ProbeHeight", 0);
                probe.SetFloat("_ProbeRadius", 0);
                Assert.That(Read(probe)[0].r, Is.Zero, "A nearby flattened hull crest must stay dark.");
                probe.SetFloat("_ProbeRadius", 200);
                var distant = Read(probe)[0];
                Assert.That(distant.r, Is.EqualTo(distant.g).Within(.001f));
                Assert.That(distant.r, Is.GreaterThan(.9f), "Distant flat geometry must retain wave translucency.");
            }
            finally
            {
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", oldShips);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", oldCapsules);
                Object.DestroyImmediate(probe); Object.DestroyImmediate(history); Object.DestroyImmediate(mask);
            }
        }

        [Test]
        public void ExistingFoamSurvivesCrestsSeaLevelAndTroughs()
        {
            var history = new Material(Resources.Load<Shader>("OceanFoamHistory"));
            var surface = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Optics"));
            var oldShips = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
            var oldCapsules = Shader.GetGlobalInt("_MotuDeckCapsuleCount");
            try
            {
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", 0);
                history.SetVector("_MotuWeatherWind", new Vector4(1, 0, 0, 1));
                history.SetTexture("_MotuWindNoise", Texture2D.whiteTexture);
                history.SetTexture("_WaveAttenuationTex", Texture2D.whiteTexture);
                history.SetVector("_WaveAttenuationWorldRect", new Vector4(-8, -8, 1f / 16, 1f / 16));
                history.SetFloat("_GeometricWaves", 1);
                history.SetFloat("_WaveFadeStart", 100);
                history.SetFloat("_WaveFadeEnd", 200);
                history.SetFloat("_WhitecapStrength", 0);
                history.SetFloat("_FoamDepositRate", 0);
                history.SetFloat("_FoamLifetime", 6);
                history.SetFloat("_FoamDeltaTime", .1f);
                history.SetFloat("_FoamHistoryValid", 1);
                history.SetTexture("_FoamPrevious", Texture2D.whiteTexture);
                history.SetVector("_FoamPreviousRect", new Vector4(-8, -8, 1f / 16, 1f / 16));
                history.SetVector("_FoamCurrentRect", new Vector4(-8, -8, 1f / 16, 1f / 16));
                surface.SetFloat("_ProbeMode", 6);
                history.SetVector("_OceanWave0", new Vector4(1, 0, 32, 1));
                for (var step = 0; step <= 32; step++)
                {
                    var phase = -Mathf.PI + step * Mathf.PI / 16;
                    history.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 32, phase));
                    var stored = Read(history, 1)[0].r;
                    Assert.That(stored, Is.EqualTo(Mathf.Exp(-.1f / 6)).Within(.001f),
                        $"Old foam must decay with age, not disappear at wave phase {phase}.");
                    surface.SetVector("_ProbeView", new Vector4(0, stored, 1, Mathf.Sin(phase)));
                    Assert.That(Read(surface, 1)[0].r, Is.EqualTo(stored).Within(.001f),
                        "The visible surface must retain stored foam below the crest-height cutoff too.");
                }
                surface.SetVector("_ProbeView", new Vector4(0, 1, 0, 1));
                Assert.That(Read(surface, 1)[0].r, Is.Zero, "Fully calm water must still hide stored foam.");
                surface.SetVector("_ProbeView", new Vector4(1, 0, 1, 0));
                Assert.That(Read(surface, 1)[0].r, Is.Zero, "A flattened crest must not generate fresh foam.");
                surface.SetVector("_ProbeView", new Vector4(1, 0, 1, 1));
                Assert.That(Read(surface, 1)[0].r, Is.EqualTo(1), "Real crests must retain fresh whitecaps.");
            }
            finally
            {
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", oldShips);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", oldCapsules);
                Object.DestroyImmediate(history);
                Object.DestroyImmediate(surface);
            }
        }

        [Test]
        public void CoastalDepthEncodingPreservesBreakingAndProtectsTenMetreSeabed()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Attenuation"));
            var mask = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            try
            {
                material.SetTexture("_SeaMask", mask);
                material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                material.SetFloat("_DepthAllowancePower", 1);
                material.SetFloat("_DistanceAllowancePower", 1);
                foreach (var depth in new[] { 0f, 1f, 2.5f, 5f, 7.5f, 10f })
                {
                    mask.SetPixel(0, 0, new Color(1 - depth / 10, 1, 0, 1));
                    mask.Apply();
                    var packed = Read(material)[0];
                    Assert.That(packed.b * 10, Is.EqualTo(depth).Within(.01), "Composed blue must preserve metres.");
                    Assert.That(packed.r, Is.EqualTo(Mathf.Clamp01(depth / 5)).Within(.001),
                        "Swell attenuation must keep its existing five-metre range.");
                }
            }
            finally { Object.DestroyImmediate(material); Object.DestroyImmediate(mask); }
            OceanShoreDistanceValidation.BatchValidateShoreDistance();
        }

        [Test]
        public void SeabedFadesBeforeMeshCutoffAtDifferentCameraAnglesAndSeaLevels()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var water = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var floor = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var cameraObject = new GameObject("Seabed cutoff validation camera");
            var material = new Material(Shader.Find("Motu/Sea Water"));
            var bed = new Material(Shader.Find("Standard"));
            var target = new RenderTexture(128, 128, 24, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(128, 128, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            try
            {
                material.SetFloat("_GeometricWaves", 0);
                material.SetFloat("_OnshoreWaveEnabled", 0);
                material.SetFloat("_WhitecapStrength", 0);
                material.SetFloat("_PersistentFoamStrength", 0);
                material.SetFloat("_RippleStrength", 0);
                material.SetFloat("_ReflectionStrength", 0);
                material.SetFloat("_SunGlintStrength", 0);
                material.SetFloat("_WaveTranslucencyStrength", 0);
                material.SetFloat("_AbsorptionStrength", 0); // Isolate cutoff fade from physical absorption.
                material.SetFloat("_RefractionStrength", .08f);
                water.GetComponent<Renderer>().sharedMaterial = material;
                water.transform.localScale = Vector3.one * 20;
                bed.color = Color.black;
                bed.EnableKeyword("_EMISSION");
                bed.SetColor("_EmissionColor", Color.green);
                bed.SetFloat("_Glossiness", 0);
                floor.GetComponent<Renderer>().sharedMaterial = bed;
                floor.transform.localScale = Vector3.one * 6;
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.depthTextureMode = DepthTextureMode.Depth;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.farClipPlane = 200;
                camera.orthographicSize = 18;
                camera.targetTexture = target;
                float Capture(bool visible)
                {
                    floor.SetActive(visible);
                    camera.Render();
                    RenderTexture.active = target;
                    readback.ReadPixels(new Rect(0, 0, 128, 128), 0, 0);
                    readback.Apply();
                    return readback.GetPixel(64, 64).g;
                }
                foreach (var orthographic in new[] { false, true })
                foreach (var tilt in new[] { 0f, 40f })
                foreach (var seaLevel in new[] { 0f, 4f })
                {
                    camera.orthographic = orthographic;
                    water.transform.position = Vector3.up * seaLevel;
                    camera.transform.position = new Vector3(0, seaLevel + 20, -tilt * .3f);
                    camera.transform.rotation = Quaternion.LookRotation(water.transform.position - camera.transform.position,
                        tilt == 0 ? Vector3.forward : Vector3.up);
                    var baseline = Capture(false);
                    floor.transform.position = Vector3.up * (seaLevel - 5);
                    var shallow = Capture(true) - baseline;
                    Assert.That(shallow, Is.GreaterThan(.25f), "Fixture must expose the bright bed in shallow water.");
                    floor.transform.position = Vector3.up * (seaLevel - 7.75f);
                    var middle = Capture(true) - baseline;
                    Assert.That(middle / shallow, Is.EqualTo(.5f).Within(.035f),
                        $"Vertical seabed fade must be independent of camera angle and sea level: ortho={orthographic}, tilt={tilt}, sea={seaLevel}.");
                    foreach (var depth in new[] { 9.5f, 10f })
                    {
                        floor.transform.position = Vector3.up * (seaLevel - depth);
                        Assert.That(Mathf.Abs(Capture(true) - baseline), Is.LessThan(.003f),
                            "The rendered bed must match missing geometry before the cutoff.");
                    }
                    if (!orthographic && tilt == 40 && seaLevel == 0)
                        SaveImage(readback, "ocean-ten-metre-cutoff.png");
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = oldTarget;
                foreach (var item in new Object[] { water, floor, cameraObject, material, bed, target, readback })
                    Object.DestroyImmediate(item);
            }
        }

        private static Color[] Read(Material material, int size = 64)
        {
            var target = new RenderTexture(size, size, 0, RenderTextureFormat.ARGBFloat);
            var output = new Texture2D(size, size, TextureFormat.RGBAFloat, false, true);
            var old = RenderTexture.active;
            try
            {
                RenderTexture.active = target;
                GL.Clear(false, true, Color.white);
                Graphics.Blit(Texture2D.blackTexture, target, material);
                RenderTexture.active = target;
                output.ReadPixels(new Rect(0, 0, size, size), 0, 0);
                output.Apply();
                return output.GetPixels();
            }
            finally { RenderTexture.active = old; Object.DestroyImmediate(output); Object.DestroyImmediate(target); }
        }

        [Test]
        public void AbsorptionUsesMetresAndRgbExtinctionAndFresnelIncreasesAtGrazingAngles()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Optics"));
            try
            {
                material.SetVector("_AbsorptionCoefficients", new Vector4(.45f, .12f, .055f, 0));
                material.SetFloat("_AbsorptionStrength", 1);
                material.SetFloat("_ProbeDistance", 0);
                Assert.That(Read(material)[0].r, Is.EqualTo(1).Within(.0001));
                material.SetFloat("_ProbeDistance", 5);
                var pixels = Read(material);
                Assert.That(pixels[0].r, Is.EqualTo(Mathf.Exp(-2.25f)).Within(.001));
                Assert.That(pixels[0].g, Is.EqualTo(Mathf.Exp(-.6f)).Within(.001));
                Assert.That(pixels[0].b, Is.EqualTo(Mathf.Exp(-.275f)).Within(.001));
                Assert.That(pixels[0].a, Is.GreaterThan(.95));
                Assert.That(pixels[63].a, Is.EqualTo(.02037).Within(.0001));
                material.SetFloat("_AbsorptionStrength", 0);
                Assert.That(Read(material)[0].r, Is.EqualTo(1).Within(.0001));
                material.SetFloat("_ProbeMode", 3);
                material.SetFloat("_ProbeDistance", 9);
                Assert.That(Read(material)[0].r, Is.Zero, "Foreground geometry rejects distorted refraction.");
                material.SetFloat("_ProbeDistance", 11);
                Assert.That(Read(material)[0].r, Is.EqualTo(1));
            }
            finally { Object.DestroyImmediate(material); }
        }

        [Test]
        public void WindRipplesAffectNormalsAndDistantDetailBecomesRoughness()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Optics"));
            var noise = ProceduralNoiseTextures.CreateWeatherNoiseTexture();
            try
            {
                material.SetTexture("_NoiseTex", noise);
                material.SetFloat("_ProbeMode", 1);
                material.SetFloat("_ProbeSpan", 4);
                material.SetFloat("_RippleStrength", .12f);
                material.SetFloat("_RippleWorldSize", 1.2f);
                material.SetFloat("_RippleSpeed", 1);
                material.SetVector("_MotuWindOffset", Vector4.zero);
                material.SetFloat("_SurfaceRoughness", .16f);
                material.SetFloat("_WindRoughness", .12f);
                material.SetVector("_MotuWeatherWind", new Vector4(1, 0, 0, 0));
                var calm = Read(material);
                material.SetVector("_MotuWeatherWind", new Vector4(1, 0, 12, 1));
                var near = Read(material);
                material.SetVector("_MotuWindOffset", new Vector4(6, 0, 0, 0));
                var moving = Read(material); // Half a second of travel at 12 m/s.
                material.SetFloat("_RippleSpeed", 0);
                var frozen = Read(material);
                material.SetFloat("_RippleSpeed", 1);
                material.SetFloat("_ProbeSpan", 512);
                var distant = Read(material);
                float change = 0, animation = 0, nearVariance = 0, distantVariance = 0;
                for (var i = 0; i < near.Length; i++)
                {
                    Assert.That(calm[i].g, Is.EqualTo(1).Within(.001));
                    change += Mathf.Abs(near[i].r) + Mathf.Abs(near[i].b);
                    animation += Mathf.Abs(moving[i].r-near[i].r) + Mathf.Abs(moving[i].b-near[i].b);
                    Assert.That(near[i].g, Is.GreaterThan(.98f), "Fine ripples must remain a gentle surface perturbation.");
                    Assert.That(frozen[i].r, Is.EqualTo(near[i].r).Within(.00001));
                    Assert.That(frozen[i].b, Is.EqualTo(near[i].b).Within(.00001));
                    nearVariance += near[i].a;
                    distantVariance += distant[i].a;
                    Assert.That(Mathf.Abs(distant[i].r) + Mathf.Abs(distant[i].b), Is.LessThan(.001));
                }
                Assert.That(change / near.Length, Is.InRange(.005f, .06f));
                Assert.That(animation / near.Length, Is.GreaterThan(.005f), "Ripple normals must visibly change as wind travel advances.");
                Debug.Log($"Ocean detail: mean slope={change / near.Length:F5}, half-second movement={animation / near.Length:F5}");
                Assert.That(distantVariance, Is.GreaterThan(nearVariance));
                material.SetFloat("_ProbeMode", 2);
                material.SetVector("_ProbeView", Vector3.up);
                material.SetVector("_ProbeLight", Vector3.up);
                material.SetFloat("_SurfaceRoughness", .1f);
                var smoothPeak = Read(material)[0].r;
                material.SetFloat("_SurfaceRoughness", .35f);
                Assert.That(Read(material)[0].r, Is.LessThan(smoothPeak * .2f), "Rough water broadens the sun highlight.");
            }
            finally { Object.DestroyImmediate(material); Object.DestroyImmediate(noise); }
        }

        [Test]
        public void RefractionRejectsRenderedForegroundInBothTextureAxes()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var water = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var floor = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var hull = GameObject.CreatePrimitive(PrimitiveType.Cube);
            var cameraObject = new GameObject("Refraction validation camera");
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Refraction"));
            var red = new Material(Shader.Find("Standard"));
            var green = new Material(Shader.Find("Standard"));
            var target = new RenderTexture(320, 320, 24);
            var readback = new Texture2D(320, 320, TextureFormat.RGB24, false);
            var oldTarget = RenderTexture.active;
            try
            {
                foreach (var opaque in new[] { red, green })
                {
                    opaque.color = Color.black;
                    opaque.EnableKeyword("_EMISSION");
                    opaque.SetFloat("_Glossiness", 0);
                }
                red.SetColor("_EmissionColor", Color.red);
                green.SetColor("_EmissionColor", Color.green);
                water.GetComponent<Renderer>().sharedMaterial = material;
                water.transform.localScale = Vector3.one * 3;
                floor.GetComponent<Renderer>().sharedMaterial = green;
                floor.transform.position = Vector3.down * 3;
                floor.transform.localScale = Vector3.one * 3;
                hull.GetComponent<Renderer>().sharedMaterial = red;
                hull.transform.position = Vector3.up * 1.5f;
                hull.transform.localScale = new Vector3(3, 2, 3);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.transform.position = Vector3.up * 14;
                camera.transform.rotation = Quaternion.LookRotation(Vector3.down, Vector3.forward);
                camera.depthTextureMode = DepthTextureMode.Depth;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.green;
                camera.targetTexture = target;
                Color[] Capture(float strength, bool safe, string name)
                {
                    material.SetFloat("_RefractionStrength", strength);
                    material.SetFloat("_ProbeSafe", safe ? 1 : 0);
                    camera.Render();
                    RenderTexture.active = target;
                    readback.ReadPixels(new Rect(0, 0, 320, 320), 0, 0);
                    readback.Apply();
                    SaveImage(readback, name);
                    return readback.GetPixels();
                }
                var baseline = Capture(0, true, "refraction-baseline.png");
                foreach (var direction in new[] { Vector2.right, Vector2.up, Vector2.left, Vector2.down })
                {
                    material.SetVector("_ProbeRipple", new Vector4(direction.x, direction.y, 0, 0));
                    var unsafePixels = Capture(.12f, false, $"refraction-unsafe-{direction.x}-{direction.y}.png");
                    var safePixels = Capture(.12f, true, $"refraction-safe-{direction.x}-{direction.y}.png");
                    var unsafeLeaks = 0;
                    var safeLeaks = 0;
                    for (var i = 0; i < baseline.Length; i++)
                    {
                        if (baseline[i].r > .1f) continue;
                        if (unsafePixels[i].r > .5f) unsafeLeaks++;
                        if (safePixels[i].r > .5f) safeLeaks++;
                    }
                    Assert.That(unsafeLeaks, Is.GreaterThan(100), "Fixture must visibly pull foreground into the water without depth rejection.");
                    Assert.That(safeLeaks, Is.LessThan(unsafeLeaks * .02f), $"Foreground must be rejected for screen offset {direction}.");
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = oldTarget;
                foreach (var item in new Object[] { water, floor, hull, cameraObject, material, red, green, target, readback })
                    Object.DestroyImmediate(item);
            }
        }

        private static void SaveImage(Texture2D texture, string name)
        {
            var directory = System.Environment.GetEnvironmentVariable("MOTU_OCEAN_OPTICS_IMAGES");
            if (string.IsNullOrEmpty(directory)) return;
            Directory.CreateDirectory(directory);
            File.WriteAllBytes(Path.Combine(directory, name), texture.EncodeToPNG());
        }

        [TestCase(true, 0.07f)]
        [TestCase(false, 0.07f)]
        [TestCase(true, 0f)]
        [TestCase(false, 0f)]
        public void PlanarReflectionCrossesClipPlaneWithoutFrustumErrors(bool simplified, float offset)
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var cameraObject = new GameObject("Reflection crossing camera");
            var planeObject = new GameObject("Reflection crossing plane");
            var target = new RenderTexture(320, 180, 24);
            var above = GameObject.CreatePrimitive(PrimitiveType.Cube);
            var below = GameObject.CreatePrimitive(PrimitiveType.Cube);
            var objectShader = Shader.Find(simplified ? "Motu/Terrain Unified" : "Unlit/Color");
            var red = new Material(objectShader) { color = Color.red };
            var blue = new Material(objectShader) { color = Color.blue };
            var pixels = new Texture2D(160, 90, TextureFormat.RGBA32, false);
            var previousActive = RenderTexture.active;
            var previousInvertCulling = GL.invertCulling;
            try
            {
                // Both paths should show the red object above sea level and
                // reject the blue one below it, including after resurfacing.
                above.GetComponent<Renderer>().sharedMaterial = red;
                below.GetComponent<Renderer>().sharedMaterial = blue;
                above.transform.position = new Vector3(-1, 2, 0);
                below.transform.position = new Vector3(1, -2, 0);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.farClipPlane = 16000;
                camera.targetTexture = target;
                target.Create();
                var reflection = cameraObject.AddComponent<PlanarWaterReflection>();
                reflection.Configure(planeObject.transform);
                var settings = new SerializedObject(reflection);
                settings.FindProperty("frameInterval").intValue = 1;
                settings.FindProperty("clipPlaneOffset").floatValue = offset;
                settings.FindProperty("useSimplifiedShader").boolValue = simplified;
                settings.ApplyModifiedPropertiesWithoutUndo();

                foreach (var tilt in new[] { 0f, 13f })
                {
                    planeObject.transform.rotation = Quaternion.Euler(0, 0, tilt);
                    foreach (var height in new[] { 10f, 0.01f, 0f, -offset + 0.001f,
                        -offset, -offset - 0.001f, -1f, -10f, 2f })
                    foreach (var pitch in new[] { -90f, -30f, -1f, 0f, 1f, 30f, 90f })
                    {
                        cameraObject.transform.SetPositionAndRotation(
                            planeObject.transform.TransformPoint(new Vector3(0, height, -10)),
                            planeObject.transform.rotation * Quaternion.Euler(pitch, 0, 0));
                        var previousCount = reflection.ReflectionRenderCount;
                        reflection.PrepareReflection();
                        Assert.That(reflection.ReflectionRenderCount, Is.EqualTo(previousCount + 1));
                        Assert.That(GL.invertCulling, Is.EqualTo(previousInvertCulling));
                        Assert.That(reflection.ReflectionCamera.projectionMatrix.determinant,
                            Is.Not.EqualTo(0), $"height={height}, pitch={pitch}, tilt={tilt}");
                    }
                }
                planeObject.transform.rotation = Quaternion.identity;
                cameraObject.transform.SetPositionAndRotation(
                    new Vector3(0, 2, -10), Quaternion.Euler(10, 0, 0));
                reflection.PrepareReflection();
                RenderTexture.active = reflection.ReflectionCamera.targetTexture;
                pixels.ReadPixels(new Rect(0, 0, 160, 90), 0, 0);
                pixels.Apply();
                var redPixels = 0;
                var bluePixels = 0;
                foreach (var pixel in pixels.GetPixels())
                {
                    if (pixel.r > 0.02f && pixel.r > pixel.b * 2) redPixels++;
                    if (pixel.b > 0.02f && pixel.b > pixel.r * 2) bluePixels++;
                }
                Assert.That(redPixels, Is.GreaterThan(10), "Above-water geometry must still reflect.");
                Assert.That(bluePixels, Is.Zero, "Submerged geometry must remain clipped.");
                Assert.That(reflection.LastRenderUsedSimplifiedShader, Is.EqualTo(simplified));
            }
            finally
            {
                RenderTexture.active = previousActive;
                Object.DestroyImmediate(cameraObject);
                Object.DestroyImmediate(planeObject);
                Object.DestroyImmediate(above);
                Object.DestroyImmediate(below);
                Object.DestroyImmediate(red);
                Object.DestroyImmediate(blue);
                Object.DestroyImmediate(pixels);
                target.Release();
                Object.DestroyImmediate(target);
            }
        }

        [UnityTest]
        public IEnumerator PlanarReflectionsRenderAndGenerateRoughnessMips()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            ViewRenderingValidation.ValidatePlanarWaterReflectionRender();
            yield return new ExitPlayMode();
        }

        private static float FoamAt(OceanFoamHistory history, Vector2 world)
        {
            var rect = history.WorldRect;
            var x = Mathf.Clamp((int)((world.x - rect.x) * rect.z * OceanFoamHistory.Resolution), 0, OceanFoamHistory.Resolution-1);
            var y = Mathf.Clamp((int)((world.y - rect.y) * rect.w * OceanFoamHistory.Resolution), 0, OceanFoamHistory.Resolution-1);
            var texture = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var old = RenderTexture.active;
            try
            {
                RenderTexture.active = history.Texture;
                texture.ReadPixels(new Rect(x, y, 1, 1), 0, 0);
                texture.Apply();
                return texture.GetPixel(0, 0).r;
            }
            finally { RenderTexture.active = old; Object.DestroyImmediate(texture); }
        }

        [UnityTest]
        public IEnumerator FoamPersistsDecaysReprojectsAndResetsOnTeleport()
        {
            var material = new Material(Shader.Find("Motu/Sea Water"));
            var history = new OceanFoamHistory();
            var oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            var oldShips = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
            var oldCapsules = Shader.GetGlobalInt("_MotuDeckCapsuleCount");
            try
            {
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 0, 1));
                Shader.SetGlobalTexture("_MotuWindNoise", Texture2D.whiteTexture);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", 0);
                material.SetFloat("_WaveDomainWarp", 0);
                material.SetFloat("_WaveAmplitudeVariation", 0);
                material.SetFloat("_OnshoreWaveEnabled", 0);
                material.SetFloat("_WhitecapCoverage", 1);
                material.SetFloat("_FoamLifetime", 1);
                material.SetFloat("_FoamDepositRate", 5);
                for (var i = 0; i < 4; i++)
                {
                    material.SetVector($"_OceanWave{i}", new Vector4(1, 0, 32, i == 0 ? 1 : 0));
                    material.SetVector($"_OceanWaveFrom{i}", new Vector4(1, 0, 32, 0));
                }
                material.SetVector("_OceanWaveChoppiness", Vector4.zero);
                for (var i = 0; i < 5; i++) { history.Update(material, Vector3.zero, .05f); yield return null; }
                var deposited = FoamAt(history, new Vector2(8, 0));
                Assert.That(deposited, Is.GreaterThan(.05));
                material.SetFloat("_WhitecapStrength", 0);
                history.Update(material, new Vector3(4, 0, 0), .05f);
                yield return null;
                var carried = FoamAt(history, new Vector2(8, 0));
                Assert.That(carried, Is.EqualTo(deposited * Mathf.Exp(-.05f)).Within(.004), "Recentring retains world-space foam.");
                // At 20 m/s wind, surface drift is .4 m/s: .1 texel in .25 s.
                // Compare against the actual neighbouring texels at a foam edge.
                var edgeX = 0;
                var edge = 0f;
                var neighbour = 0f;
                for (var x = -16; x <= 16; x++)
                {
                    var a = FoamAt(history, new Vector2(x, 0));
                    var b = FoamAt(history, new Vector2(x-1, 0));
                    if (Mathf.Abs(a-b) <= Mathf.Abs(edge-neighbour)) continue;
                    edgeX = x; edge = a; neighbour = b;
                }
                Assert.That(Mathf.Abs(edge-neighbour), Is.GreaterThan(.03f));
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 20, 1));
                history.Update(material, new Vector3(4, 0, 0), .25f);
                yield return null;
                Assert.That(FoamAt(history, new Vector2(edgeX, 0)),
                    Is.EqualTo(Mathf.Lerp(edge, neighbour, .1f) * Mathf.Exp(-.25f)).Within(.001),
                    "Wind carries foam across the world grid, independently of new deposits.");
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 0, 1));
                for (var i = 0; i < 10; i++) { history.Update(material, new Vector3(4, 0, 0), .1f); yield return null; }
                Assert.That(FoamAt(history, new Vector2(8, 0)), Is.LessThan(carried * .4f));
                history.Update(material, Vector3.right * 1000, .05f);
                yield return null;
                Assert.That(FoamAt(history, new Vector2(1000, 0)), Is.Zero);
                history.Dispose();
                Assert.IsNull(history.Texture);
                Assert.IsFalse(ShaderUtil.ShaderHasError(Resources.Load<Shader>("OceanFoamHistory")));
            }
            finally
            {
                history.Dispose(); Object.DestroyImmediate(material);
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", oldShips);
                Shader.SetGlobalInt("_MotuDeckCapsuleCount", oldCapsules);
            }
        }
    }
}
