using System;
using System.IO;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanFoamTests
    {
        [Test]
        public void StoredAndWakeFoamFadeGraduallyBeforeTheirTextureBoundaries()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ocean Optics"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var output = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            try
            {
                material.SetFloat("_ProbeMode", 5);
                material.SetFloat("_PersistentFoamStrength", 1);
                material.SetTexture("_OceanFoamHistory", Texture2D.whiteTexture);
                material.SetTexture("_MotuShipWaveField", Texture2D.whiteTexture);
                material.SetFloat("_MotuShipWaveEnabled", 1);
                foreach (var centre in new[] { Vector2.zero, new Vector2(120, -64) })
                {
                    material.SetVector("_OceanFoamHistoryRect",
                        new Vector4(centre.x - 128, centre.y - 128, 1f / 256, 1f / 256));
                    material.SetVector("_MotuShipWaveRect",
                        new Vector4(centre.x - 128, centre.y - 128, 1f / 256, 1f / 256));
                    float Sample(float x, float z)
                    {
                        material.SetVector("_ProbeView", new Vector4(centre.x + x, 0, centre.y + z, 0));
                        Graphics.Blit(Texture2D.blackTexture, target, material);
                        RenderTexture.active = target;
                        output.ReadPixels(new Rect(0, 0, 1, 1), 0, 0);
                        output.Apply();
                        var pixel = output.GetPixel(0, 0);
                        Assert.That(pixel.g, Is.EqualTo(pixel.r).Within(.001f),
                            "Direct wake foam must use the same broad fade as stored foam.");
                        if (Mathf.Max(Mathf.Abs(x), Mathf.Abs(z)) <= 112)
                        {
                            Assert.That(pixel.b, Is.EqualTo(1).Within(.001f), "Hull clamping must stay unchanged.");
                            Assert.That(pixel.a, Is.EqualTo(1).Within(.001f), "Wave height must stay unchanged.");
                        }
                        return pixel.r;
                    }
                    Assert.That(Sample(0, 0), Is.EqualTo(1).Within(.001f));
                    Assert.That(Sample(60, 0), Is.EqualTo(1).Within(.001f));
                    Assert.That(Sample(96, 0), Is.InRange(.3f, .7f),
                        "Foam should already be softly fading well before the old square edge.");
                    var previous = 1f;
                    for (var radius = 64; radius <= 132; radius++)
                    {
                        var value = Sample(radius, 0);
                        Assert.That(value, Is.LessThanOrEqualTo(previous + .001f));
                        Assert.That(previous - value, Is.LessThan(.03f), "No abrupt one-metre cutoff.");
                        Assert.That(Sample(radius * .6f, radius * .8f), Is.EqualTo(value).Within(.001f),
                            "The fade must be rounded, including towards texture corners.");
                        previous = value;
                    }
                    Assert.That(Sample(128, 0), Is.Zero.Within(.001f));
                    Assert.That(Sample(120, 120), Is.Zero.Within(.001f));
                    Assert.That(Sample(-140, 0), Is.Zero.Within(.001f));
                }
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
        }

        [Test]
        public void SingleFoamPatternDriftsAndDeformsContinuously()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var target = new RenderTexture(512, 512, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(512, 512, TextureFormat.RGBAFloat, false, true);
            var noise = new Texture2D(128, 128, TextureFormat.RGBA32, false, true)
                { wrapMode = TextureWrapMode.Repeat, filterMode = FilterMode.Bilinear };
            var oldTarget = RenderTexture.active;
            var oldNoise = Shader.GetGlobalTexture("_MotuWindNoise");
            try
            {
                var texels = new Color[128 * 128];
                for (var y = 0; y < 128; y++)
                for (var x = 0; x < 128; x++)
                {
                    // Periodic input avoids a texture seam affecting wrap checks.
                    var u = x * Mathf.PI * 2 / 128;
                    var v = y * Mathf.PI * 2 / 128;
                    var n = .5f + .2f * Mathf.Sin(u + Mathf.Sin(v * 2))
                        + .2f * Mathf.Cos(v * 3 + Mathf.Sin(u * 2));
                    texels[y * 128 + x] = new Color(n, n, n, 1);
                }
                noise.SetPixels(texels);
                noise.Apply();
                Shader.SetGlobalTexture("_MotuWindNoise", noise);
                material.SetFloat("_ProbeFoam", 1);
                material.SetFloat("_WhitecapNoiseWorldSize", 7);
                material.SetFloat("_WhitecapCoverage", .58f);
                material.SetFloat("_WhitecapDistortionScale", .32f);
                material.SetVector("_ProbeWorldRect", new Vector4(-14, -14, 28, 28));
                Color[] Capture(string name = null)
                {
                    Graphics.Blit(Texture2D.blackTexture, target, material);
                    RenderTexture.active = target;
                    readback.ReadPixels(new Rect(0, 0, 512, 512), 0, 0);
                    readback.Apply();
                    var result = readback.GetPixels();
                    if (name != null && Environment.GetEnvironmentVariable("MOTU_OCEAN_FOAM_IMAGES") is string directory)
                    {
                        Directory.CreateDirectory(directory);
                        var png = new Texture2D(512, 512, TextureFormat.RGB24, false);
                        var colours = new Color[result.Length];
                        for (var i = 0; i < result.Length; i++)
                            colours[i] = Color.Lerp(new Color(.02f, .12f, .16f), Color.white, result[i].b);
                        png.SetPixels(colours);
                        png.Apply();
                        File.WriteAllBytes(Path.Combine(directory, name), png.EncodeToPNG());
                        Object.DestroyImmediate(png);
                    }
                    return result;
                }
                material.SetFloat("_WhitecapDistortionStrength", 0);
                var plain = Capture("foam-undistorted.png");
                material.SetVector("_OceanFoamTravel", new Vector4(0, 0, 1.2f, 0));
                var frozen = Capture();
                for (var i = 0; i < plain.Length; i++)
                    Assert.That(frozen[i].b, Is.EqualTo(plain[i].b).Within(.00001f));

                material.SetFloat("_WhitecapDistortionStrength", .65f);
                material.SetVector("_OceanFoamTravel", Vector4.zero);
                var warped = Capture("foam-distorted-phase-0.png");
                material.SetVector("_OceanFoamTravel", new Vector4(0, 0, 1.2f, 0));
                var animated = Capture("foam-distorted-phase-1.png");
                var deformation = 0f;
                var animation = 0f;
                for (var i = 0; i < plain.Length; i++)
                {
                    Assert.IsTrue(float.IsFinite(animated[i].r) && float.IsFinite(animated[i].g));
                    deformation += Mathf.Abs(warped[i].b - plain[i].b);
                    animation += Mathf.Abs(animated[i].b - warped[i].b);
                }
                Assert.That(deformation / plain.Length, Is.GreaterThan(.1f), "Distortion must visibly reshape the patches.");
                Assert.That(animation / plain.Length, Is.GreaterThan(.1f), "Advancing phase must deform the foam without requiring drift.");
                material.SetVector("_OceanFoamTravel", new Vector4(0, 0, 2 * Mathf.PI, 0));
                var wrapped = Capture();
                for (var i = 0; i < warped.Length; i++)
                {
                    Assert.That(wrapped[i].r, Is.EqualTo(warped[i].r).Within(.0001f));
                    Assert.That(wrapped[i].g, Is.EqualTo(warped[i].g).Within(.0001f));
                    // GPU texture interpolation can quantize nearby UVs differently;
                    // allow less than one displayed colour level at the coverage edge.
                    Assert.That(wrapped[i].b, Is.EqualTo(warped[i].b).Within(1f / 255), "Distortion phase must wrap without a visible jump.");
                }
                material.SetVector("_OceanFoamTravel", new Vector4(3, -2, 0, 0));
                material.SetVector("_ProbeWorldRect", new Vector4(-11, -16, 28, 28));
                var translated = Capture();
                for (var i = 0; i < warped.Length; i++)
                {
                    Assert.That(translated[i].r, Is.EqualTo(warped[i].r).Within(.0001f));
                    Assert.That(translated[i].g, Is.EqualTo(warped[i].g).Within(.0001f));
                    Assert.That(translated[i].b, Is.EqualTo(warped[i].b).Within(1f / 255), "The distorted pattern must move coherently with accumulated travel.");
                }
                if (Environment.GetEnvironmentVariable("MOTU_OCEAN_FOAM_IMAGES") != null)
                {
                    var weatherNoise = Motu.Rendering.ProceduralNoiseTextures.CreateWeatherNoiseTexture();
                    try
                    {
                        Shader.SetGlobalTexture("_MotuWindNoise", weatherNoise);
                        material.SetVector("_ProbeWorldRect", new Vector4(-14, -14, 28, 28));
                        material.SetVector("_OceanFoamTravel", Vector4.zero);
                        Capture("foam-weather-noise-phase-0.png");
                        material.SetVector("_OceanFoamTravel", new Vector4(0, 0, 1.2f, 0));
                        Capture("foam-weather-noise-phase-1.png");
                    }
                    finally { Object.DestroyImmediate(weatherNoise); }
                }
                Debug.Log($"Ocean foam: mean distortion change={deformation / plain.Length:F3}, animation change={animation / plain.Length:F3}");
            }
            finally
            {
                RenderTexture.active = oldTarget;
                Shader.SetGlobalTexture("_MotuWindNoise", oldNoise);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(noise);
            }
        }
    }
}
