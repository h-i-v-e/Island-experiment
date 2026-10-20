using System;
using System.Collections;
using System.Reflection;
using Motu.Gameplay;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class HullDraftLoadingTests
    {
        private const BindingFlags Private = BindingFlags.Instance | BindingFlags.NonPublic;

        private static ShipHullGeometry Box()
        {
            var profile = ScriptableObject.CreateInstance<ShipHullGeometry>();
            typeof(ShipHullGeometry).GetField("sections", Private).SetValue(profile,
                new[] { new ShipHullGeometry.Section { station = 0, length = 4,
                    outline = new[] { new Vector2(-1,-1), new Vector2(1,-1), new Vector2(1,1), new Vector2(-1,1) } } });
            return profile;
        }

        [TestCase(.25f, 2050f)]
        [TestCase(.75f, 6150f)]
        public void DraftSetsTheMassOfDisplacedWater(float draft, float expectedMass)
        {
            var profile = Box();
            try
            {
                Assert.IsTrue(HullDraftLoading.TryCalculateMass(profile, Vector3.one, draft, 1025f, out var mass));
                Assert.That(mass, Is.EqualTo(expectedMass).Within(.01f));
            }
            finally { Object.DestroyImmediate(profile); }
        }

        [Test]
        public void DraftUsesWorldMetresAndIncludesScaledBeamAndLength()
        {
            var profile = Box();
            try
            {
                Assert.IsTrue(HullDraftLoading.TryCalculateMass(profile, new Vector3(2, 2, 3), .5f, 1025f, out var mass));
                Assert.That(mass, Is.EqualTo(24600f).Within(.01f));
                foreach (var draft in new[] { 0f, -1f, 4f, float.NaN, float.PositiveInfinity })
                    Assert.IsFalse(HullDraftLoading.TryCalculateMass(profile, new Vector3(2,2,3), draft, 1025f, out _));
            }
            finally { Object.DestroyImmediate(profile); }
        }

        [Test]
        public void TaperedBottomAndOffsetMeshOriginUseActualSectionVolume()
        {
            var profile = Box();
            try
            {
                profile.Sections[0].outline = new[] { new Vector2(0, 4), new Vector2(1, 6), new Vector2(-1, 6) };
                Assert.IsTrue(HullDraftLoading.TryCalculateMass(profile, Vector3.one, .5f, 1025f, out var mass));
                Assert.That(mass, Is.EqualTo(512.5f).Within(.01f));
            }
            finally { Object.DestroyImmediate(profile); }
        }

        [Test]
        public void ReferencePitchUsesVerticalDraftAndVolumeWeightedCentre()
        {
            var profile = Box();
            try
            {
                var outline = profile.Sections[0].outline;
                typeof(ShipHullGeometry).GetField("sections", Private).SetValue(profile,
                    new[] { new ShipHullGeometry.Section { station = -1, length = 2, outline = outline },
                        new ShipHullGeometry.Section { station = 1, length = 2, outline = outline } });
                var up = Quaternion.Inverse(Quaternion.Euler(30, 0, 0)) * Vector3.up;
                Assert.IsTrue(HullDraftLoading.TryCalculateLoading(profile, Vector3.one, 1f, 1025f, up, out var loading));
                // Independent rectangle clipping: one station is dry; the other
                // has width 2 and submerged height 1/cos(30 degrees).
                float volume = 4f / Mathf.Cos(Mathf.PI / 6);
                Assert.That(loading.Mass, Is.EqualTo(volume * 1025).Within(.03f));
                Assert.That(loading.CentreOfBuoyancy.z, Is.EqualTo(1).Within(.001f));
                Assert.That(loading.MinimumHeight, Is.EqualTo(-Mathf.Cos(Mathf.PI / 6) - .5f).Within(.001f));
                foreach (var invalid in new[] { Vector3.zero, new Vector3(float.NaN, 1, 0) })
                    Assert.IsFalse(HullDraftLoading.TryCalculateLoading(profile, Vector3.one, 1, 1025, invalid, out _));
            }
            finally { Object.DestroyImmediate(profile); }
        }

        [UnityTest]
        public IEnumerator CorrectedReferencePitchRemainsBalancedWhenDraftChanges()
        {
            yield return new EnterPlayMode();
            var scene = SceneManager.CreateScene("Corrected hull reference", new CreateSceneParameters(LocalPhysicsMode.Physics3D));
            var root = new GameObject("Trim-calibrated hull"); SceneManager.MoveGameObjectToScene(root, scene);
            var profile = Box();
            try
            {
                var outline = profile.Sections[0].outline;
                var sections = new ShipHullGeometry.Section[32];
                for (int i = 0; i < sections.Length; i++) sections[i] = new ShipHullGeometry.Section
                    { station = -2 + (i + .5f) * .125f, length = .125f, outline = outline };
                typeof(ShipHullGeometry).GetField("sections", Private).SetValue(profile, sections);
                root.AddComponent<BoxCollider>().size = new Vector3(2, 2, 4);
                var body = root.AddComponent<Rigidbody>(); body.rotation = Quaternion.Euler(10, 0, 0);
                var buoyancy = root.AddComponent<OceanBuoyancy>(); buoyancy.enabled = false;
                typeof(OceanBuoyancy).GetMethod("Awake", Private).Invoke(buoyancy, null);
                typeof(OceanBuoyancy).GetField("hullGeometry", Private).SetValue(buoyancy, profile);
                var samples = new OceanBuoyancy.WaterSampleFilter[6];
                for (int i = 0; i < samples.Length; i++) samples[i].Update(0, 1, .02f, 0);
                typeof(OceanBuoyancy).GetField("waterSamples", Private).SetValue(buoyancy, samples);
                buoyancy.Draft = .75f;
                var loading = root.AddComponent<HullDraftLoading>(); loading.ConfigureReferencePose(body.rotation, true, .15f);
                Assert.IsTrue(HullDraftLoading.TryCalculateLoading(profile, Vector3.one, buoyancy.Draft, 1025, loading.ReferenceUp, out var expected));
                body.position = Vector3.up * -expected.Waterline;
                var forces = typeof(OceanBuoyancy).GetMethod("ApplyHullForces", Private);
                var physics = scene.GetPhysicsScene();
                void Settle()
                {
                    for (int i = 0; i < 800; i++) { forces.Invoke(buoyancy, new object[] { 9.81f }); physics.Simulate(.02f); }
                    Assert.That(Quaternion.Angle(body.rotation, loading.ReferenceRotation), Is.LessThan(.4f));
                }
                body.rotation = Quaternion.Euler(13, 0, 0); Settle();
                float shallowY = body.position.y;
                var previousPosition = body.position;
                buoyancy.Draft += .2f;
                typeof(HullDraftLoading).GetMethod("FixedUpdate", Private).Invoke(loading, null);
                Assert.That(Vector3.Distance(body.position, previousPosition), Is.LessThan(.00001f));
                Settle();
                Assert.That(shallowY - body.position.y, Is.EqualTo(.2f).Within(.02f));
                Assert.That(body.constraints, Is.EqualTo(RigidbodyConstraints.None));
            }
            finally
            {
                Object.DestroyImmediate(root); Object.DestroyImmediate(profile); SceneManager.UnloadSceneAsync(scene);
            }
        }

        [UnityTearDown]
        public IEnumerator LeaveSimulation()
        {
            if (Application.isPlaying) yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator ChangingDraftLowersTheDynamicHullWithoutTeleportingIt()
        {
            yield return new EnterPlayMode();
            var scene = SceneManager.CreateScene("Draft equilibrium", new CreateSceneParameters(LocalPhysicsMode.Physics3D));
            var root = new GameObject("Draft-controlled hull");
            SceneManager.MoveGameObjectToScene(root, scene);
            var profile = Box();
            try
            {
                root.AddComponent<BoxCollider>().size = new Vector3(2,2,4);
                var body = root.AddComponent<Rigidbody>();
                body.centerOfMass = Vector3.zero;
                body.position = Vector3.up * .75f;
                var buoyancy = root.AddComponent<OceanBuoyancy>();
                buoyancy.enabled = false;
                typeof(OceanBuoyancy).GetMethod("Awake", Private).Invoke(buoyancy, null);
                typeof(OceanBuoyancy).GetField("hullGeometry", Private).SetValue(buoyancy, profile);
                var samples = new OceanBuoyancy.WaterSampleFilter[6];
                for (int i = 0; i < samples.Length; i++) samples[i].Update(0f, 1f, .02f, 0f);
                typeof(OceanBuoyancy).GetField("waterSamples", Private).SetValue(buoyancy, samples);
                buoyancy.Draft = .25f;
                var loading = root.AddComponent<HullDraftLoading>();
                loading.ApplyDraftLoading();
                var physics = scene.GetPhysicsScene();
                var forces = typeof(OceanBuoyancy).GetMethod("ApplyHullForces", Private);
                void Settle()
                {
                    for (int i = 0; i < 500; i++)
                    {
                        forces.Invoke(buoyancy, new object[] { 9.81f });
                        physics.Simulate(.02f);
                    }
                }
                Settle();
                Assert.That(body.position.y, Is.EqualTo(.75f).Within(.01f));
                var oldPosition = body.position;
                var oldVelocity = body.linearVelocity;
                buoyancy.Draft = .75f;
                // Exercise the actual runtime update, without forcing its cache dirty.
                typeof(HullDraftLoading).GetMethod("FixedUpdate", Private).Invoke(loading, null);
                Assert.That(body.mass, Is.EqualTo(6150f).Within(.01f));
                Assert.That(body.position, Is.EqualTo(oldPosition));
                Assert.That(body.linearVelocity, Is.EqualTo(oldVelocity));
                Settle();
                Assert.That(body.position.y, Is.EqualTo(.25f).Within(.015f));
            }
            finally
            {
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(profile);
                SceneManager.UnloadSceneAsync(scene);
            }
        }
    }
}
