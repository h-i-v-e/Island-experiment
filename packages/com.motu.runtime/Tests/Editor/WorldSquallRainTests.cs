using System;
using System.Collections;
using System.IO;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldSquallRainTests
    {
        private sealed class Fixture : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Rain edge fixture") { layer = 31 };
            internal readonly SquallVisualRenderer Visuals;
            internal readonly SquallWeatherSettings Settings = SquallWeatherSettings.Default;
            internal readonly Vector3 Viewer = Vector3.up * 12;
            internal ParticleSystem Drops => Root.GetComponentInChildren<ParticleSystem>();

            internal Fixture() => Visuals = new SquallVisualRenderer(Root.transform);

            internal void Step(float age, Vector3 centre)
            {
                var field = new WorldSquallField(new Vector2(centre.x, centre.z), 200, .25f, 4, 1, Vector2.zero);
                Visuals.UpdateVisuals(new WorldSquallState(centre, 400, age, 1, 7129, field, 0), Settings, Viewer, .1f);
            }

            public void Dispose() { Visuals.Dispose(); Object.DestroyImmediate(Root); }
        }

        [Test]
        public void RainBuildsGraduallyAcrossTheWholeRadius()
        {
            var counts = new int[4];
            var distances = new[] { 0f, 100f, 150f, 200f };
            for (var i = 0; i < distances.Length; i++)
            {
                using var fixture = new Fixture();
                var centre = Vector3.right * distances[i];
                fixture.Step(0, centre); fixture.Step(.1f, centre);
                counts[i] = fixture.Visuals.RainParticleCount;
            }
            Assert.That(counts[0], Is.GreaterThan(40));
            Assert.That(counts[1] / (float)counts[0], Is.InRange(.4f, .6f), "Halfway out has intermediate rain rather than full intensity.");
            Assert.That(counts[2], Is.GreaterThan(0).And.LessThan(counts[1]), "The outer quarter remains light and patchy.");
            Assert.That(counts[3], Is.Zero, "The boundary does not emit new drops.");
        }

        [Test]
        public void CrossingTheEdgePreservesFallingDropsAndLetsThemFadeBeforeExpiry()
        {
            using var fixture = new Fixture();
            fixture.Step(0, Vector3.zero); fixture.Step(.1f, Vector3.zero);
            var before = new ParticleSystem.Particle[1800];
            var count = fixture.Drops.GetParticles(before);
            var outside = Vector3.forward * 1000;
            fixture.Step(.15f, outside);
            var after = new ParticleSystem.Particle[1800];
            Assert.That(fixture.Drops.GetParticles(after), Is.EqualTo(count), "Leaving the cell must not erase airborne drops.");
            Assert.That(after[0].randomSeed, Is.EqualTo(before[0].randomSeed));
            Assert.That(after[0].position.y, Is.LessThan(before[0].position.y));
            Assert.That(Vector3.Distance(after[0].position, before[0].position), Is.LessThan(3), "Drops stay in world space as the storm moves.");
            fixture.Step(.15f, outside);
            Assert.That(fixture.Visuals.RainParticleCount, Is.EqualTo(count), "A paused weather clock keeps its remaining drops.");
            for (var i = 1; i <= 10; i++) fixture.Step(.15f + i * .1f, outside);
            Assert.That(fixture.Drops.GetParticles(after), Is.GreaterThan(0));
            Assert.That(after[0].GetCurrentColor(fixture.Drops).a, Is.LessThan(after[0].startColor.a), "An old drop fades before its lifetime ends.");
            for (var i = 11; i <= 15; i++) fixture.Step(.15f + i * .1f, outside);
            Assert.That(fixture.Visuals.RainParticleCount, Is.Zero, "The trailing shower expires without further emission.");
        }

        [UnityTest]
        public IEnumerator AirborneDropsRemainVisibleAfterTheCloudLeavesTheCameraFrustum()
        {
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset)) Assert.Ignore("URP is required.");
            using var fixture = new Fixture();
            var cameraObject = new GameObject("Trailing rain camera");
            var texture = new RenderTexture(640, 360, 24);
            var image = new Texture2D(640, 360, TextureFormat.RGB24, false);
            var previous = RenderTexture.active;
            try
            {
                var camera = cameraObject.AddComponent<Camera>(); camera.enabled = false;
                camera.clearFlags = CameraClearFlags.SolidColor; camera.backgroundColor = new Color(.03f, .04f, .05f);
                camera.cullingMask = 1 << 31; camera.nearClipPlane = .1f; camera.farClipPlane = 3000;
                camera.fieldOfView = 80; camera.allowHDR = false; camera.allowMSAA = false;
                camera.targetTexture = texture; camera.transform.position = fixture.Viewer;
                camera.transform.LookAt(fixture.Viewer + new Vector3(0, 10, -20));
                camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                fixture.Step(0, Vector3.zero); fixture.Step(.1f, Vector3.zero);
                fixture.Step(.15f, Vector3.forward * 1000);
                yield return null;
                Assert.That(SquallVisualRenderer.NeedsRender(camera), Is.True, "Visible drops still need the rain pass when their cloud is behind the camera.");
                RenderPipeline.SubmitRenderRequest(camera,
                    new UniversalRenderPipeline.SingleCameraRequest { destination = texture });
                RenderTexture.active = texture;
                image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0); image.Apply();
                var wet = image.GetPixels32();
                var directory = Path.Combine(Path.GetTempPath(), "motu-squall-rain"); Directory.CreateDirectory(directory);
                File.WriteAllBytes(Path.Combine(directory, "trailing-rain.png"), image.EncodeToPNG());
                fixture.Visuals.UpdateVisuals(default, fixture.Settings, fixture.Viewer, 0);
                RenderPipeline.SubmitRenderRequest(camera,
                    new UniversalRenderPipeline.SingleCameraRequest { destination = texture });
                image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0); image.Apply();
                var dry = image.GetPixels32(); var changed = 0;
                for (var i = 0; i < wet.Length; i++)
                    if (Mathf.Abs(wet[i].r - dry[i].r) + Mathf.Abs(wet[i].g - dry[i].g) + Mathf.Abs(wet[i].b - dry[i].b) > 8) changed++;
                Assert.That(changed, Is.GreaterThan(20), "The actual camera must render the surviving droplets.");
                File.WriteAllBytes(Path.Combine(directory, "after-rain.png"), image.EncodeToPNG());
            }
            finally
            {
                RenderTexture.active = previous; Object.DestroyImmediate(cameraObject);
                Object.DestroyImmediate(texture); Object.DestroyImmediate(image);
            }
        }
    }
}
