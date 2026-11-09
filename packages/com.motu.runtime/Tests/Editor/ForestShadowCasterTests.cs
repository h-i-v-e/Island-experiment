using System;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ForestShadowCasterTests
    {
        [TestCase(0u)]
        [TestCase(1u)]
        [TestCase(2u)]
        public void AllFarTreesInRangeCastIncludingOffscreenBatches(uint prototype)
        {
            using var fixture = new Fixture(600, 260, prototype);
            var forest = fixture.Forest;
            forest.Submit(fixture.Camera);
            Assert.That(forest.SubmittedDetailedCrowns, Is.Zero);
            Assert.That(forest.SubmittedFarTrees, Is.EqualTo(600));
            Assert.That(forest.SubmittedFarShadowTrees, Is.EqualTo(600));
            Assert.That(forest.SubmittedBatches, Is.EqualTo(3), "Casters remain packed in 256-instance draws.");

            fixture.Camera.transform.rotation = Quaternion.Euler(0, 180, 0);
            forest.Submit(fixture.Camera);
            Assert.That(forest.SubmittedFarTrees, Is.Zero);
            Assert.That(forest.SubmittedShadowOnlyTrees, Is.EqualTo(600), "Turning away must retain nearby offscreen casters.");

            fixture.Camera.farClipPlane = 250;
            forest.Submit(fixture.Camera);
            Assert.That(forest.SubmittedFarShadowTrees, Is.Zero);
            Assert.That(forest.SubmittedBatches, Is.Zero, "Out-of-range invisible forests must submit nothing.");

            fixture.Camera.farClipPlane = 1000;
            fixture.Camera.transform.rotation = Quaternion.identity;
            fixture.Camera.GetUniversalAdditionalCameraData().renderShadows = false;
            forest.Submit(fixture.Camera);
            Assert.That(forest.SubmittedFarTrees, Is.EqualTo(600));
            Assert.That(forest.SubmittedFarShadowTrees, Is.Zero, "Per-camera shadow settings apply immediately.");
        }

        [Test]
        public void DenseStandKeepsTheVisualBudgetButAddsAllRemainingShadowCasters()
        {
            using var fixture = new Fixture(600, 30, 0);
            fixture.Forest.Submit(fixture.Camera);
            Assert.That(fixture.Forest.SubmittedDetailedCrowns, Is.EqualTo(96));
            Assert.That(fixture.Forest.SubmittedFarTrees, Is.EqualTo(504));
            Assert.That(fixture.Forest.SubmittedFarShadowTrees, Is.EqualTo(504));
            Assert.That(fixture.Forest.SubmittedShadowOnlyTrees, Is.Zero);
        }

        [Test]
        public void ShadowRangeFollowsThePipelineCameraClipAndCameraOverride()
        {
            using var fixture = new Fixture(1, 260, 0);
            Assert.That(PlanetForestWorld.ShadowDistance(fixture.Camera), Is.EqualTo(300));
            fixture.Camera.farClipPlane = 120;
            Assert.That(PlanetForestWorld.ShadowDistance(fixture.Camera), Is.EqualTo(120));
            fixture.Camera.GetUniversalAdditionalCameraData().renderShadows = false;
            Assert.That(PlanetForestWorld.ShadowDistance(fixture.Camera), Is.Zero);
        }

        private sealed class Fixture : IDisposable
        {
            private readonly GameObject root = new GameObject("Forest caster fixture");
            private readonly UniversalRenderPipelineAsset pipeline;
            private readonly float previousDistance;
            internal readonly Camera Camera;
            internal readonly InstancedForestRenderer Forest;

            internal Fixture(int count, float distance, uint prototype)
            {
                pipeline = (UniversalRenderPipelineAsset)GraphicsSettings.currentRenderPipeline;
                previousDistance = pipeline.shadowDistance; pipeline.shadowDistance = 300;
                root.layer = 31;
                var cameraRoot = new GameObject("Caster fixture camera");
                cameraRoot.transform.SetParent(root.transform, false);
                Camera = cameraRoot.AddComponent<Camera>(); Camera.enabled = false;
                Camera.cullingMask = 1 << 31; Camera.farClipPlane = 1000;
                var source = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.up * 4, Vector3.right },
                    new[] { Vector3.forward, Vector3.forward, Vector3.forward }, new[] { 0, 1, 2 },
                    Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
                var parts = new[] { source, source, source, source, source, source, source, source };
                var trees = new HybridForestNative.Instance[count];
                for (var i = 0; i < count; ++i)
                    trees[i] = new HybridForestNative.Instance { anchor = new Vector3(.5f, .5f + distance / 1000, 0),
                        scale = 1, prototype = prototype };
                Forest = root.AddComponent<InstancedForestRenderer>();
                Forest.Initialize(root.transform, new HybridForestData(new[] { parts, parts, parts }, trees, 1, 1000));
            }

            public void Dispose()
            {
                Forest.Release(); Object.DestroyImmediate(root);
                pipeline.shadowDistance = previousDistance;
            }
        }
    }
}
