using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanShelteredAudioTests
    {
        [TestCase(0f, .25f)]
        [TestCase(1f / 3f, .25f)]
        [TestCase(1f, .25f)]
        [TestCase(0f, 5f)]
        [TestCase(1f / 3f, 5f)]
        [TestCase(1f, 5f)]
        public void ShoreAudioAndShipSurfaceSamplesShareLocalShelterAcrossTheWaveCycle(float retained, float amplitude)
        {
            using var coast = new OceanShoreAudioTests.Coast();
            using var shore = new OceanShoreSampler(2);
            using var surface = new OceanSurfaceSampler(2, includeVelocity: true);
            var shelter = new Texture2D(16, 16, TextureFormat.RGBAFloat, false, true)
            { wrapMode = TextureWrapMode.Clamp, filterMode = FilterMode.Bilinear };
            try
            {
                var pixels = new Color[256];
                for (int z = 0; z < 16; z++)
                    for (int x = 0; x < 16; x++)
                        pixels[z * 16 + x] = z < 8 ? Color.black : Color.white;
                shelter.SetPixels(pixels); shelter.Apply();
                coast.Fill(uniformDepth: 4f);
                coast.Material.SetVector("_OnshoreWaveParameters", new Vector4(12f, amplitude, 0f, 0f));
                coast.Material.SetFloat("_WaveFadeStart", 1000f);
                coast.Material.SetFloat("_WaveFadeEnd", 2000f);
                coast.Material.SetTexture("_OceanShelterTex", shelter);
                coast.Material.SetVector("_OceanShelterTex_TexelSize", new Vector4(1f / 16, 1f / 16, 16, 16));
                coast.Material.SetVector("_OceanShelterWorldRect", new Vector4(-128f, -128f, 1f / 256, 1f / 256));
                coast.Material.SetVector("_OceanShelterParameters", new Vector4(retained, 0f, 1f, 0f));
                var positions = new[] { new Vector3(20.125f, 0f, -48f), new Vector3(20.125f, 0f, 48f) };
                for (int phase = 0; phase < 16; phase++)
                {
                    coast.Material.SetFloat("_OnshoreWavePhase", 2f * Mathf.PI * phase / 16);
                    Assert.IsTrue(shore.SubmitWaves(coast.Ocean, positions));
                    surface.Submit(coast.Ocean, positions);
                    AsyncGPUReadback.WaitAllRequests();
                    Assert.IsTrue(shore.TryGetSample(0, out var exposed));
                    Assert.IsTrue(shore.TryGetSample(1, out var lee));
                    Assert.That(exposed.Amplitude, Is.EqualTo(Mathf.Min(amplitude, 4f)).Within(.001f));
                    Assert.That(lee.Amplitude, Is.EqualTo(exposed.Amplitude * retained).Within(.001f));
                    Assert.That(exposed.WaveScale, Is.EqualTo(1f).Within(.0001f));
                    Assert.That(lee.WaveScale, Is.EqualTo(retained).Within(.0001f));
                    Assert.IsTrue(surface.TryGetSurface(0, positions[0], 1f,
                        out var exposedHeight, out _, out var exposedScale));
                    Assert.IsTrue(surface.TryGetSurface(1, positions[1], 1f,
                        out var leeHeight, out _, out var leeScale));
                    Assert.That(leeHeight, Is.EqualTo(exposedHeight * retained).Within(.001f));
                    Assert.That(lee.Position.y, Is.EqualTo(leeHeight).Within(.001f));
                    Assert.That(exposedScale, Is.EqualTo(exposed.WaveScale).Within(.0001f));
                    Assert.That(leeScale, Is.EqualTo(lee.WaveScale).Within(.0001f));
                }

                // Disabling shelter must replace previously calm cached samples.
                coast.Material.SetVector("_OceanShelterParameters", Vector4.zero);
                surface.Submit(coast.Ocean, positions);
                AsyncGPUReadback.WaitAllRequests();
                Assert.IsTrue(surface.TryGetSurface(1, positions[1], 1f, out _, out _, out var resetScale));
                Assert.That(resetScale, Is.EqualTo(1f));
            }
            finally { Object.DestroyImmediate(shelter); }
        }
    }
}
