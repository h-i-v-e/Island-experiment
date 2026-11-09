using System;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldSquallFieldTests
    {
        [TestCase(0f)]
        [TestCase(.02f)]
        [TestCase(-.02f)]
        public void ThreeShaderFieldsSelectTheStrongestAdvectedCell(float timeOffset)
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat)) Assert.Ignore("Float rendering unavailable.");
            var baseline = new EnvironmentHostState();
            var fields = new[] {
                new WorldSquallField(new Vector2(40, 0), 100, .3f, 2, 1, new Vector2(8, 1)),
                new WorldSquallField(new Vector2(160, 0), 100, .4f, 4, .8f, new Vector2(3, -2)),
                new WorldSquallField(new Vector2(290, 0), 90, .25f, 3, 1, new Vector2(12, 2)) };
            const int count = 81;
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Squall Field"));
            var target = new RenderTexture(count, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var image = new Texture2D(count, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                for (var slot = 0; slot < fields.Length; slot++) WorldSquallShader.Publish(slot, fields[slot]);
                material.SetVector("_FieldTestLine", new Vector4(0, 0, 5, 0));
                material.SetFloat("_FieldTestCount", count);
                material.SetFloat("_FieldTestTimeOffset", timeOffset);
                Graphics.Blit(Texture2D.blackTexture, target, material);
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, count, 1), 0, 0);
                image.Apply();
                var actual = image.GetPixels();
                float Gain(Vector2 point)
                {
                    float gain = 1;
                    foreach (var field in fields)
                    {
                        var moved = new WorldSquallField(field.Center + field.Velocity * timeOffset,
                            field.RadiusMetres, field.EdgeFraction, field.WindMultiplier, field.Strength, field.Velocity);
                        gain = Mathf.Max(gain, moved.MultiplierAt(point));
                    }
                    return gain;
                }
                for (var i = 0; i < count; i++)
                {
                    var point = Vector2.right * (i * 5);
                    Assert.That(actual[i].r, Is.EqualTo(Gain(point)).Within(.00003f), "Three-cell gain at " + i);
                    const float step = .01f;
                    var gx = (Gain(point + Vector2.right * step) - Gain(point - Vector2.right * step)) / (2 * step);
                    var gz = (Gain(point + Vector2.up * step) - Gain(point - Vector2.up * step)) / (2 * step);
                    Assert.That(actual[i].b, Is.EqualTo(gx).Within(.0003f), "Strongest-cell derivative X at " + i);
                    Assert.That(actual[i].a, Is.EqualTo(gz).Within(.0003f), "Strongest-cell derivative Z at " + i);
                }
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(material); Object.DestroyImmediate(target); Object.DestroyImmediate(image);
                baseline.Restore();
            }
        }

        [Test]
        public void FootprintHasFullCoreSmoothEdgeAndNoEffectOutside()
        {
            var field = Field();
            Assert.That(field.MultiplierAt(field.Center), Is.EqualTo(2f));
            Assert.That(field.MultiplierAt(field.Center + Vector2.right * 85), Is.EqualTo(2f));
            Assert.That(field.MultiplierAt(field.Center + Vector2.right * 92), Is.InRange(1f, 2f));
            Assert.That(field.MultiplierAt(field.Center + Vector2.right * 100), Is.EqualTo(1f));
            Assert.That(field.MultiplierAt(field.Center + Vector2.up * 200), Is.EqualTo(1f));
            Assert.That(field.WeightAt(field.Center + Vector2.right * 92),
                Is.EqualTo(field.WeightAt(field.Center + Vector2.up * 92)));
            Assert.That(default(WorldSquallField).MultiplierAt(Vector2.zero), Is.EqualTo(1f));
        }

        [Test]
        public void LifecycleStrengthAndAdvectionDoNotAmplifyAmbientVelocity()
        {
            var field = Field(.5f);
            Assert.That(field.MultiplierAt(field.Center), Is.EqualTo(1.5f));
            Assert.That(field.ShaderWind, Is.EqualTo(new Vector4(.5f, 9, -4, 0)));
            Assert.That(Field(0).MultiplierAt(field.Center), Is.EqualTo(1f));
            var later = new WorldSquallField(field.Center + field.Velocity * 3,
                field.RadiusMetres, field.EdgeFraction, field.WindMultiplier, field.Strength, field.Velocity);
            Assert.That(later.Center - field.Center, Is.EqualTo(new Vector2(27, -12)));
            Assert.That(later.Velocity, Is.EqualTo(field.Velocity));
            Assert.That(later.MultiplierAt(later.Center), Is.EqualTo(1.5f));
        }

        [Test]
        public void HardEdgeAndInactiveRadiusRemainFinite()
        {
            var hard = new WorldSquallField(Vector2.zero, 50, 0, 2, 1, Vector2.zero);
            Assert.That(hard.MultiplierAt(new Vector2(49.99f, 0)), Is.EqualTo(2f));
            Assert.That(hard.MultiplierAt(new Vector2(50, 0)), Is.EqualTo(1f));
            Assert.That(new WorldSquallField(Vector2.zero, 0, .15f, 2, 1, Vector2.zero)
                .MultiplierAt(Vector2.zero), Is.EqualTo(1f));
            Assert.Throws<ArgumentException>(() => new WorldSquallField(Vector2.zero, float.NaN, .15f, 2, 1, Vector2.zero));
            Assert.Throws<ArgumentException>(() => new WorldSquallField(Vector2.zero, 50, .15f, 2, 1, new Vector2(float.PositiveInfinity, 0)));
        }

        [TestCase(0f)]
        [TestCase(.02f)]
        [TestCase(-.02f)]
        public void ShaderFootprintMatchesCpuAtCurrentAndVelocityQueryTimes(float timeOffset)
        {
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat)
                || !SystemInfo.SupportsTextureFormat(TextureFormat.RGBAFloat))
                Assert.Ignore("GPU field parity requires floating-point render targets.");
            var shader = Shader.Find("Hidden/Motu/Tests/Squall Field");
            Assert.That(shader, Is.Not.Null);
            Assert.That(shader.isSupported, Is.True);
            var material = new Material(shader);
            const int count = 65;
            var target = new RenderTexture(count, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var image = new Texture2D(count, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                var field = Field(.8f);
                var origin = field.Center - Vector2.right * 160;
                material.SetVector("_MotuSquall", field.ShaderBounds);
                material.SetVector("_MotuSquallWind", field.ShaderWind);
                material.SetVector("_FieldTestLine", new Vector4(origin.x, origin.y, 5, 0));
                material.SetFloat("_FieldTestCount", count);
                material.SetFloat("_FieldTestTimeOffset", timeOffset);
                Graphics.Blit(Texture2D.blackTexture, target, material);
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, count, 1), 0, 0);
                image.Apply();
                var actual = image.GetPixels();
                var sampled = new WorldSquallField(field.Center + field.Velocity * timeOffset,
                    field.RadiusMetres, field.EdgeFraction, field.WindMultiplier, field.Strength, field.Velocity);
                for (int i = 0; i < count; i++)
                {
                    var point = origin + Vector2.right * (5 * i);
                    Assert.That(actual[i].r, Is.EqualTo(sampled.MultiplierAt(point)).Within(.00002f), "Wind multiplier at sample " + i);
                    Assert.That(actual[i].g, Is.EqualTo(sampled.WeightAt(point)).Within(.00002f), "Spatial weight at sample " + i);
                    const float step = .01f;
                    var gradientX = (sampled.MultiplierAt(point + Vector2.right * step)
                        - sampled.MultiplierAt(point - Vector2.right * step)) / (2 * step);
                    var gradientY = (sampled.MultiplierAt(point + Vector2.up * step)
                        - sampled.MultiplierAt(point - Vector2.up * step)) / (2 * step);
                    // A central difference straddling the clipped rim has a
                    // small truncation residual; the analytic rim derivative is zero.
                    Assert.That(actual[i].b, Is.EqualTo(gradientX).Within(.0001f), "Wind gradient X at sample " + i);
                    Assert.That(actual[i].a, Is.EqualTo(gradientY).Within(.0001f), "Wind gradient Z at sample " + i);
                }
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(image);
            }
        }

        private static WorldSquallField Field(float strength = 1f) =>
            new WorldSquallField(new Vector2(37, -82), 100, .15f, 2, strength, new Vector2(9, -4));
    }
}
