using System;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ForestShadowTests
    {
        [Test]
        public void PreparedRustPixelsUploadWithoutACpuCopyAndRespectIslandBounds()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Forest Shadow"));
            using var mask = new ForestShadowMask(Forest(64,
                Tree(.25f, .75f, 2, Mathf.PI / 2), Tree(.99f, .02f)), material);
            try
            {
                mask.SetVisible(true);
                Assert.That(mask.Texture.format, Is.EqualTo(TextureFormat.R8));
                Assert.That(mask.Texture.isReadable, Is.False, "The island must not retain a second CPU texture copy.");
                Assert.That(mask.Texture.mipmapCount, Is.GreaterThan(1));
                Assert.That(Probe(material, new Vector3(-16, 0, 8)).r, Is.LessThan(.01f), "Occupied prepared pixels.");
                Assert.That(Probe(material, Vector3.zero).r, Is.GreaterThan(.99f), "Unoccupied ground stays clear.");
                Assert.That(Probe(material, new Vector3(-31, 0, -30)).r, Is.GreaterThan(.99f), "Edge stamps must be clipped, never repeated.");
                Assert.That(Probe(material, new Vector3(34, 0, -30)).r, Is.GreaterThan(.99f), "The collar outside the island stays clear.");
                mask.SetVisible(false);
                Assert.That(Probe(material, new Vector3(-16, 0, 8)).r, Is.EqualTo(1).Within(.001f));
            }
            finally { mask.Dispose(); Object.DestroyImmediate(material); }
        }

        [Test]
        public void MapsStayIndependentAndUnbindBeforeTheyAreReleased()
        {
            var first = new Material(Shader.Find("Motu/Terrain Unified"));
            var second = new Material(first);
            var a = new ForestShadowMask(Forest(2000, Tree(.2f, .7f)), first);
            var b = new ForestShadowMask(Forest(4000, Tree(.8f, .2f)), second);
            try
            {
                a.SetVisible(true); b.SetVisible(true);
                Assert.That(a.Texture.width, Is.EqualTo(1024));
                Assert.That(b.Texture.width, Is.EqualTo(2048));
                Assert.That(first.GetTexture("_MotuForestShadowMask"), Is.Not.SameAs(second.GetTexture("_MotuForestShadowMask")));
                Assert.That(second.GetVector("_MotuForestShadowUV").x, Is.EqualTo(1f / 4000));
                a.Dispose();
                Assert.That(first.GetTexture("_MotuForestShadowMask"), Is.Null);
                Assert.That(first.GetFloat("_MotuForestShadowStrength"), Is.Zero);
                Assert.That(second.GetTexture("_MotuForestShadowMask"), Is.EqualTo(b.Texture));
            }
            finally { a.Dispose(); b.Dispose(); Object.DestroyImmediate(first); Object.DestroyImmediate(second); }
        }

        [TestCase(0f, 0f)]
        [TestCase(.25f, 0f)]
        [TestCase(.5f, .4f)]
        [TestCase(1f, .4f)]
        [TestCase(1f, 1f)]
        public void GpuBlendReplacesTheRealtimeFadeEndpointWithoutDoubleFading(float fade, float rawRealtime)
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Forest Shadow"));
            var previousCamera = Shader.GetGlobalVector("_WorldSpaceCameraPos");
            var previousParams = Shader.GetGlobalVector("_MainLightShadowParams");
            try
            {
                material.EnableKeyword("_MAIN_LIGHT_SHADOWS");
                material.SetTexture("_MotuForestShadowMask", Texture2D.whiteTexture);
                material.SetVector("_MotuForestShadowUV", new Vector4(0, 0, .5f, .5f));
                material.SetFloat("_MotuForestShadowStrength", .8f);
                Shader.SetGlobalVector("_WorldSpaceCameraPos", Vector4.zero);
                Shader.SetGlobalVector("_MainLightShadowParams", new Vector4(1, 0, 0, fade));
                var result = Probe(material, Vector3.zero, Mathf.Lerp(rawRealtime, 1, fade));
                Assert.That(result.g, Is.EqualTo(fade).Within(.001f));
                Assert.That(result.b, Is.EqualTo(Mathf.Lerp(rawRealtime, .2f, fade)).Within(.002f));
            }
            finally
            {
                Shader.SetGlobalVector("_WorldSpaceCameraPos", previousCamera);
                Shader.SetGlobalVector("_MainLightShadowParams", previousParams);
                Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void UpdatedShadersCompile()
        {
            foreach (var name in new[] { "Motu/Terrain Unified", "Motu/Planet Forest", "Motu/Planar Reflection Simplified", "Hidden/Motu/Tests/Forest Shadow" })
            {
                var shader = Shader.Find(name);
                Assert.That(shader, Is.Not.Null, name);
                Assert.That(ShaderUtil.ShaderHasError(shader), Is.False, name);
            }
        }

        internal static HybridForestNative.Instance Tree(float x, float y, float scale = 1, float yaw = 0) =>
            new HybridForestNative.Instance { anchor = new Vector3(x, y, 0), scale = scale, yawRadians = yaw };

        internal static HybridForestData Forest(float size, params HybridForestNative.Instance[] trees)
        {
            var crown = new IslandPreparedMesh(new[] { new Vector3(-1, 10, -3), new Vector3(7, 10, 5) },
                Array.Empty<Vector3>(), Array.Empty<int>(), Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
            // Controlled prepared pixels test the upload and lighting, not a
            // second implementation of Rust's canopy projection algorithm.
            var resolution = Mathf.Clamp(Mathf.NextPowerOfTwo(Mathf.CeilToInt(size / 2)), 256, 2048);
            var pixels = new byte[resolution * resolution];
            FillMaskRectangle(pixels, resolution, new Rect(.125f, .45f, .325f, .35f));
            FillMaskRectangle(pixels, resolution, new Rect(.6f, .2f, .25f, .25f));
            return new HybridForestData(new[] { new[] { crown, crown } }, trees, 1, size, resolution, pixels);
        }

        private static void FillMaskRectangle(byte[] pixels, int resolution, Rect rectangle)
        {
            var startX = Mathf.CeilToInt(rectangle.xMin * resolution);
            var endX = Mathf.FloorToInt(rectangle.xMax * resolution);
            for (var y = Mathf.CeilToInt(rectangle.yMin * resolution); y < rectangle.yMax * resolution; ++y)
                Array.Fill(pixels, (byte)255, y * resolution + startX, endX - startX);
        }

        private static Color Probe(Material material, Vector3 local, float realtime = 1)
        {
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var pixel = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                material.SetVector("_ForestTestLocalPosition", local);
                material.SetVector("_ForestTestPosition", new Vector4(0, 0, 0, realtime));
                Graphics.Blit(Texture2D.blackTexture, target, material);
                RenderTexture.active = target; pixel.ReadPixels(new Rect(0, 0, 1, 1), 0, 0); pixel.Apply();
                return pixel.GetPixel(0, 0);
            }
            finally { RenderTexture.active = previous; Object.DestroyImmediate(target); Object.DestroyImmediate(pixel); }
        }
    }
}
