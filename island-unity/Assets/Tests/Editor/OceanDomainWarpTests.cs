using Motu.Rendering;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanDomainWarpTests
    {
        [Test]
        public void BroadWarpBreaksWaveRepeatsAndRemainsAnchoredToWorldWind()
        {
            const int width = 512;
            var noise = ProceduralNoiseTextures.CreateWeatherNoiseTexture();
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var target = new RenderTexture(width, 1, 0, RenderTextureFormat.ARGBFloat);
            var output = new Texture2D(width, 1, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            try
            {
                material.SetTexture("_MotuWindNoise", noise);
                material.SetVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                material.SetFloat("_WaveNoiseWorldSize", 2048);
                material.SetFloat("_WaveDomainWarp", 32);
                material.SetFloat("_ProbeDomainWarp", 1);
                material.SetVector("_ProbeWorldRect", new Vector4(0, 173, 2048, 0));
                Color[] Capture()
                {
                    Graphics.Blit(Texture2D.blackTexture, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, width, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                var warped = Capture();
                var localChange = 0f;
                var broadChange = 0f;
                for (var i = 0; i < width - 64; i++)
                {
                    var here = new Vector2(warped[i].r, warped[i].g);
                    localChange += Vector2.Distance(here, new Vector2(warped[i + 1].r, warped[i + 1].g));
                    broadChange += Vector2.Distance(here, new Vector2(warped[i + 64].r, warped[i + 64].g));
                }
                localChange /= width - 64;
                broadChange /= width - 64;
                Assert.That(broadChange, Is.GreaterThan(4), "The warp must move crests materially over 256 m.");
                Assert.That(broadChange, Is.GreaterThan(localChange * 8),
                    "Most bending must occur over hundreds of metres, not between nearby vertices.");

                material.SetVector("_MotuWindOffset", new Vector4(10000, -17000, 0, 0));
                material.SetVector("_ProbeWorldRect", new Vector4(10000, -16827, 2048, 0));
                var translated = Capture();
                for (var i = 0; i < width; i++)
                {
                    Assert.That(translated[i].r, Is.EqualTo(warped[i].r).Within(.01f));
                    Assert.That(translated[i].g, Is.EqualTo(warped[i].g).Within(.01f));
                }
                material.SetFloat("_WaveDomainWarp", 0);
                foreach (var pixel in Capture())
                {
                    Assert.That(pixel.r, Is.Zero);
                    Assert.That(pixel.g, Is.Zero);
                }

                material.SetFloat("_ProbeDomainWarp", 0);
                material.SetVector("_MotuWindOffset", Vector4.zero);
                material.SetVector("_OceanWave0", new Vector4(1, 0, 48, 1));
                material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 48, 0));
                foreach (var strength in new[] { 0f, 32f })
                {
                    material.SetFloat("_WaveDomainWarp", strength);
                    material.SetVector("_ProbeWorldRect", new Vector4(0, 173, 2048, 0));
                    var first = Capture();
                    material.SetVector("_ProbeWorldRect", new Vector4(48, 173, 2048, 0));
                    var nextCrest = Capture();
                    var difference = 0f;
                    for (var i = 0; i < width; i++) difference += Mathf.Abs(first[i].r - nextCrest[i].r);
                    difference /= width;
                    if (strength == 0)
                        Assert.That(difference, Is.LessThan(.0001f), "Disabling warp restores the original wave equations.");
                    else
                        Assert.That(difference, Is.GreaterThan(.08f), "Adjacent distant crests must no longer be identical copies.");
                }
                Debug.Log($"Broad wave warp: 4 m change={localChange:F3} m, 256 m change={broadChange:F3} m.");
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(noise);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
        }
    }
}
