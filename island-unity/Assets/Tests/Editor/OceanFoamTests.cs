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
