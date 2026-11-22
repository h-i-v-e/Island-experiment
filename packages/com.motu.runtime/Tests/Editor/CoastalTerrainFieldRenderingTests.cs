using System.Threading;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalTerrainFieldRenderingTests
    {
        [Test]
        public void IndependentlyBakedMapsFadeAtCoastSeamsWhileIslandMapsRemainUnchanged()
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat))
                Assert.Ignore("Coastal baked map comparison requires floating-point render targets.");
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Coastal Terrain Field"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                material.SetFloat("_ProbeBakedMap", 1);
                material.SetVector("_MotuTerrainSeam", new Vector4(1000, 160, 0, 0));
                Assert.That(Sample(material, target, readback, 1000, 0).x, Is.EqualTo(0).Within(.0001f));
                Assert.That(Sample(material, target, readback, 920, 0).x, Is.EqualTo(.5f).Within(.0001f));
                Assert.That(Sample(material, target, readback, 0, 0).x, Is.EqualTo(1).Within(.0001f));
                material.SetVector("_MotuTerrainSeam", Vector4.zero);
                Assert.That(Sample(material, target, readback, 1000, 0).x, Is.EqualTo(1).Within(.0001f));
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }

        [TestCase(0, false)]
        [TestCase(7, false)]
        [TestCase(0, true)]
        [TestCase(7, true)]
        public void AdjacentAndRepeatedSectionEdgesSampleTheSameNoiseAndLayerPhase(int section, bool inlandSouth)
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat))
                Assert.Ignore("Coastal field comparison requires a floating-point render target.");
            var shader = Shader.Find("Hidden/Motu/Tests/Coastal Terrain Field");
            Assert.That(shader, Is.Not.Null);
            var material = new Material(shader);
            var noise = new Texture3D(16, 16, 16, TextureFormat.RGBA32, false)
            { wrapMode = TextureWrapMode.Repeat, filterMode = FilterMode.Bilinear };
            var pixels = new Color[16 * 16 * 16];
            for (var z = 0; z < 16; ++z)
            for (var y = 0; y < 16; ++y)
            for (var x = 0; x < 16; ++x)
                pixels[(z * 16 + y) * 16 + x] = new Color(x / 15f, y / 15f, z / 15f, 1);
            noise.SetPixels(pixels);
            noise.Apply();
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                material.SetTexture("_CliffNoise3D", noise);
                var direction = inlandSouth ? -1 : 1;
                var west = Sample(material, target, readback, direction * 1000, direction * section * 2000);
                var east = Sample(material, target, readback, -direction * 1000, direction * (section + 1) * 2000);
                Assert.That(Vector4.Distance(west, east), Is.LessThan(.0001f),
                    "Shared cliff noise and non-divisor three-metre beach texture must agree at the join.");
                var reset = Sample(material, target, readback, -direction * 1000, direction * section * 2000);
                Assert.That(Vector4.Distance(west, reset), Is.GreaterThan(.01f),
                    "The probe must distinguish the former per-section phase reset.");
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(noise);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void TerrainAndGrassKeepTheirCoastalFieldPropertyInEveryCompiledPass()
        {
            foreach (var name in new[] { "Motu/Terrain Unified", "Motu/Terrain Grass" })
            {
                var shader = Shader.Find(name);
                Assert.That(shader, Is.Not.Null, name);
                var material = new Material(shader);
                try
                {
                    Assert.That(material.HasProperty("_MotuTerrainFieldOffset"), Is.True, name);
                    material.SetVector("_MotuTerrainFieldOffset", new Vector4(2000, 0, 6000, 0));
                    for (var pass = 0; pass < material.passCount; ++pass)
                        ShaderUtil.CompilePass(material, pass, true);
                    Assert.That(ShaderUtil.ShaderHasError(shader), Is.False, name);
                }
                finally { Object.DestroyImmediate(material); }
            }
        }

        [Test]
        public void DirtRecipeFadesWithSlopeWhileProceduralNormalsAndDirtWeightsRemain()
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat))
                Assert.Ignore("Terrain projection comparison requires floating-point render targets.");
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Coastal Terrain Field"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var noise = new Texture3D(1, 1, 1, TextureFormat.RGBA32, false);
            var previous = RenderTexture.active;
            try
            {
                noise.SetPixels(new[] { new Color(1f, .5f, 0f, 1f) });
                noise.Apply();
                material.SetTexture("_CliffNoise3D", noise);
                material.SetFloat("_ProbeDirtProjection", 1f);

                Vector4 ProbeSlope(float degrees)
                {
                    material.SetFloat("_ProbeNormalUp", Mathf.Cos(degrees * Mathf.Deg2Rad));
                    return Sample(material, target, readback, 0, 0);
                }

                var shallow = ProbeSlope(30f);
                var transition = ProbeSlope(45f);
                var steep = ProbeSlope(60f);
                Assert.That(shallow.x, Is.EqualTo(1f).Within(.005f));
                Assert.That(transition.x, Is.EqualTo(.5f).Within(.01f));
                Assert.That(steep.x, Is.EqualTo(0f).Within(.005f));
                Assert.That(steep.y, Is.EqualTo(1f).Within(.005f));
                Assert.That(steep.z, Is.EqualTo(0f).Within(.005f));
                Assert.That(steep.w, Is.GreaterThan(.025f));
                material.SetFloat("_DirtNormalStrength", 0f);
                Assert.That(ProbeSlope(60f).w, Is.EqualTo(0f).Within(.005f));
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(noise);
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void EmptyWheatPlotsDoNotAllocateAMask()
        {
            Assert.That(WheatGroundMask.Build(System.Array.Empty<WheatField>(), CancellationToken.None), Is.Null);
        }

        [Test]
        public void RotatedWheatPlotsClearTerrainGrassAndLeavePathCoverageIntact()
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat))
                Assert.Ignore("Wheat coverage comparison requires floating-point render targets.");
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Coastal Terrain Field"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            Texture2D mask = null;
            try
            {
                const float centreX = -230;
                const float centreZ = 37;
                const float sectionOffset = 6000;
                float diagonal = Mathf.Sqrt(.5f);
                material.SetFloat("_ProbeWheatCoverage", 1);
                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, 0, 1));
                var unplanted = Sample(material, target, readback, centreX, sectionOffset, centreZ);
                Assert.That(Vector4.Distance(unplanted, new Vector4(0, 1, 0, 0)), Is.LessThan(.001f));

                var field = new WheatField(new Vector2(centreX, centreZ), new Vector2(6, 2), Mathf.PI / 4f, 1);
                mask = WheatGroundMask.Build(new[] { field }, CancellationToken.None).Bind(material);
                var planted = Sample(material, target, readback,
                    centreX + 4f * diagonal, sectionOffset, centreZ + 4f * diagonal);
                Assert.That(Vector4.Distance(planted, new Vector4(1, 0, 0, 0)), Is.LessThan(.001f));

                var outside = Sample(material, target, readback,
                    centreX + 4f * diagonal, sectionOffset, centreZ - 4f * diagonal);
                Assert.That(Vector4.Distance(outside, unplanted), Is.LessThan(.001f));
                var beyondCrop = Sample(material, target, readback, 600, sectionOffset, centreZ);
                Assert.That(Vector4.Distance(beyondCrop, unplanted), Is.LessThan(.001f));
                var edge = Sample(material, target, readback,
                    centreX + 5.775f * diagonal, sectionOffset, centreZ + 5.775f * diagonal);
                Assert.That(edge.x, Is.EqualTo(.5f).Within(.18f));

                material.SetVector("_ProbeEnvironment", new Vector4(1, 1, 0, 1));
                var unplantedForest = Sample(material, target, readback,
                    centreX + 4f * diagonal, sectionOffset, centreZ - 4f * diagonal);
                Assert.That(Vector4.Distance(unplantedForest, new Vector4(0, 0, 1, 1)), Is.LessThan(.001f));
                var clearedFloor = Sample(material, target, readback,
                    centreX + 4f * diagonal, sectionOffset, centreZ + 4f * diagonal);
                Assert.That(Vector4.Distance(clearedFloor, planted), Is.LessThan(.001f));

                material.SetFloat("_ProbeWheatCoverage", 0);
                material.SetFloat("_ProbeRoadCoverage", 1);
                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, .5f, 1));
                var shoulder = Sample(material, target, readback, centreX, sectionOffset, centreZ);
                Assert.That(Vector4.Distance(shoulder, new Vector4(1, 0, 0, 1)), Is.LessThan(.001f));
                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, .5f, 0));
                var footpath = Sample(material, target, readback, centreX, sectionOffset, centreZ);
                Assert.That(Vector4.Distance(footpath, new Vector4(1, 0, 1, 1)), Is.LessThan(.001f));
                material.SetVector("_ProbeEnvironment", Vector4.zero);
                var cartTrack = Sample(material, target, readback, centreX, sectionOffset, centreZ);
                Assert.That(Vector4.Distance(cartTrack, new Vector4(1, 0, 1, 1)), Is.LessThan(.001f));
                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, 1, 0));
                var road = Sample(material, target, readback, centreX, sectionOffset, centreZ);
                Assert.That(Vector4.Distance(road, new Vector4(1, 1, 0, 1)), Is.LessThan(.001f));
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(mask);
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void CartTrackFeathersIntoNaturalTerrainAndYieldsToTownPaving()
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat))
                Assert.Ignore("Terrain coverage comparison requires floating-point render targets.");
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Coastal Terrain Field"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                material.SetFloat("_ProbeRoadCoverage", 1);
                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, 0, 1));
                Assert.That(Sample(material, target, readback, 0, 0).z, Is.EqualTo(0).Within(.001f));
                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, 0, .75f));
                Assert.That(Sample(material, target, readback, 0, 0).z, Is.EqualTo(.25f).Within(.001f));
                material.SetVector("_ProbeEnvironment", Vector4.zero);
                Assert.That(Sample(material, target, readback, 0, 0).z, Is.EqualTo(1).Within(.001f));

                material.SetVector("_ProbeEnvironment", new Vector4(0, 0, 1, 0));
                var paved = Sample(material, target, readback, 0, 0);
                Assert.That(paved.y, Is.EqualTo(1).Within(.001f));
                Assert.That(paved.z, Is.EqualTo(0).Within(.001f));
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }

        private static Vector4 Sample(Material material, RenderTexture target, Texture2D readback,
            float localX, float logicalOffset, float localZ = 37)
        {
            material.SetVector("_ProbeLocalPosition", new Vector4(localX, 12, localZ, 0));
            material.SetVector("_MotuTerrainFieldOffset", new Vector4(logicalOffset, 0, 0, 0));
            Graphics.Blit(Texture2D.whiteTexture, target, material);
            RenderTexture.active = target;
            readback.ReadPixels(new Rect(0, 0, 1, 1), 0, 0);
            readback.Apply();
            return readback.GetPixel(0, 0);
        }
    }
}
