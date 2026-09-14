using Motu.Rendering;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanAntialiasingTests
    {
        [TestCase(4f)]
        [TestCase(16f)]
        [TestCase(64f)]
        public void WavesFadeBeforeUndersamplingAndRetainUnresolvedSlopeVariance(float wavelength)
        {
            using (var probe = new Probe())
            {
                probe.Material.SetVector("_OceanWave0", new Vector4(1, 0, wavelength, 1));
                probe.Material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, wavelength, 0));
                probe.Material.SetVector("_OceanWaveChoppiness", new Vector4(.3f, 0, 0, 0));
                var physical = probe.Read();
                probe.Material.SetFloat("_ProbeFilteredWaves", 1);
                var previousEnergy = float.MaxValue;
                for (var step = 0; step <= 20; step++)
                {
                    probe.Material.SetFloat("_ProbePixelFootprint", wavelength * step / 40);
                    var filtered = probe.Read();
                    var energy = 0f;
                    for (var i = 0; i < filtered.Length; i++)
                    {
                        energy += filtered[i].r * filtered[i].r;
                        if (step <= 2)
                        {
                            Assert.That(filtered[i].r, Is.EqualTo(physical[i].r).Within(.0001f));
                            Assert.That(filtered[i].g, Is.EqualTo(physical[i].g).Within(.0001f));
                        }
                        if (step == 20)
                        {
                            Assert.That(filtered[i].r, Is.Zero.Within(.0001f));
                            Assert.That(filtered[i].g, Is.Zero.Within(.0001f));
                            Assert.That(filtered[i].a, Is.GreaterThan(0), "Unresolved slopes must still broaden the highlight.");
                        }
                    }
                    Assert.That(energy, Is.LessThanOrEqualTo(previousEnergy + .001f));
                    previousEnergy = energy;
                }
                // A fragment footprint must never leak into physical wave queries.
                probe.Material.SetFloat("_ProbeFilteredWaves", 0);
                var unchanged = probe.Read();
                for (var i = 0; i < unchanged.Length; i++)
                    Assert.That(unchanged[i].r, Is.EqualTo(physical[i].r).Within(.0001f));
            }
        }

        [Test]
        public void DistantFoamIsStableUnderSmallCameraMovements()
        {
            using (var probe = new Probe())
            {
                var noise = ProceduralNoiseTextures.CreateWeatherNoiseTexture();
                try
                {
                    probe.Material.SetTexture("_MotuWindNoise", noise);
                    probe.Material.SetFloat("_ProbeFilteredFoam", 1);
                    probe.Material.SetFloat("_WhitecapNoiseWorldSize", 7);
                    probe.Material.SetFloat("_WhitecapCoverage", .58f);
                    float Jitter(float footprint)
                    {
                        probe.Material.SetFloat("_ProbePixelFootprint", footprint);
                        probe.Material.SetVector("_ProbeWorldRect", new Vector4(0, 0, 512, 0));
                        var a = probe.Read();
                        probe.Material.SetVector("_ProbeWorldRect", new Vector4(.015f, .01f, 512, 0));
                        var b = probe.Read();
                        var change = 0f;
                        for (var i = 0; i < a.Length; i++)
                        {
                            change += Mathf.Abs(a[i].r - b[i].r);
                            if (footprint > 0) Assert.That(a[i].r, Is.InRange(.3f, .85f), "Minified foam should retain partial coverage.");
                        }
                        return change / a.Length;
                    }
                    var unfiltered = Jitter(0);
                    var filtered = Jitter(4);
                    Assert.That(unfiltered, Is.GreaterThan(.01f), "The fixture must expose subpixel foam noise.");
                    Assert.That(filtered, Is.LessThan(unfiltered * .25f));
                    Debug.Log($"Foam camera jitter: unfiltered={unfiltered:F5}, filtered={filtered:F5}.");
                }
                finally { Object.DestroyImmediate(noise); }
            }
        }

        private sealed class Probe : System.IDisposable
        {
            public readonly Material Material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            private readonly RenderTexture target = new RenderTexture(256, 1, 0, RenderTextureFormat.ARGBFloat);
            private readonly Texture2D output = new Texture2D(256, 1, TextureFormat.RGBAFloat, false, true);
            public Probe()
            {
                Material.SetTexture("_MotuWindNoise", Texture2D.grayTexture);
                Material.SetVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Material.SetVector("_ProbeWorldRect", new Vector4(0, 0, 256, 0));
            }
            public Color[] Read()
            {
                var previous = RenderTexture.active;
                try
                {
                    Graphics.Blit(Texture2D.blackTexture, target, Material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 256, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                finally { RenderTexture.active = previous; }
            }
            public void Dispose()
            {
                Object.DestroyImmediate(Material);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
        }
    }
}
