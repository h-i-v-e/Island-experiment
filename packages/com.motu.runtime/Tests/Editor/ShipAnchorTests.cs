using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Motu.Gameplay;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ShipAnchorTests
    {
        [UnitySetUp]
        public IEnumerator EnterSimulation() { yield return new EnterPlayMode(); }

        [UnityTearDown]
        public IEnumerator ExitSimulation() { yield return new ExitPlayMode(); }

        private sealed class Anchorage : IDisposable
        {
            private readonly Scene scene = SceneManager.CreateScene("Isolated anchorage " + Guid.NewGuid(),
                new CreateSceneParameters(LocalPhysicsMode.Physics3D));
            internal readonly GameObject Ship = new GameObject("Anchored hull");
            internal readonly GameObject Ground = new GameObject("Terrain seabed");
            private readonly TerrainData terrain = new TerrainData { heightmapResolution = 33, size = new Vector3(200, 20, 200) };
            internal readonly Rigidbody Body;
            internal readonly ShipAnchor Anchor;
            internal readonly PhysicsScene Physics;

            internal Anchorage(float depth = 6f)
            {
                SceneManager.MoveGameObjectToScene(Ship, scene);
                SceneManager.MoveGameObjectToScene(Ground, scene);
                Ground.transform.position = new Vector3(-100, -depth, -100);
                Ground.AddComponent<TerrainCollider>().terrainData = terrain;
                Ship.AddComponent<BoxCollider>().size = new Vector3(2, 2, 8);
                Body = Ship.AddComponent<Rigidbody>();
                Body.mass = 300000f;
                Body.useGravity = false;
                Body.constraints = RigidbodyConstraints.FreezePositionY | RigidbodyConstraints.FreezeRotationX | RigidbodyConstraints.FreezeRotationZ;
                Body.inertiaTensor = new Vector3(2000000, 2000000, 2000000);
                Anchor = Ship.AddComponent<ShipAnchor>();
                Set(Anchor, "bowLocalPosition", new Vector3(0, 1, 4));
                Physics = scene.GetPhysicsScene();
                UnityEngine.Physics.SyncTransforms();
            }

            internal void Step(Vector3 wind, int count = 1)
            {
                for (int i = 0; i < count; i++) { Anchor.Simulate(wind); Physics.Simulate(.02f); }
            }

            public void Dispose()
            {
                Object.DestroyImmediate(Ship); Object.DestroyImmediate(Ground); Object.DestroyImmediate(terrain);
                SceneManager.UnloadSceneAsync(scene);
            }
        }

        [Test]
        public void FurlingDropsAtTheBowWithFortyFiveDegreeSlackAndPreservesMomentum()
        {
            using var sea = new Anchorage();
            sea.Body.linearVelocity = new Vector3(2, 0, 1);
            sea.Body.angularVelocity = new Vector3(0, .15f, 0);
            var bow = sea.Anchor.BowWorldPosition;
            sea.Anchor.SetDeployed(true);
            Assert.IsTrue(sea.Anchor.IsHolding);
            Assert.That(Vector3.Distance(sea.Anchor.DeploymentPosition, bow), Is.LessThan(.001f));
            Assert.That(sea.Anchor.AnchorPosition.x, Is.EqualTo(bow.x).Within(.001f));
            Assert.That(sea.Anchor.AnchorPosition.z, Is.EqualTo(bow.z).Within(.001f));
            Assert.That(sea.Anchor.AnchorPosition.y, Is.EqualTo(-6f).Within(.001f));
            Assert.That(sea.Anchor.RodeLengthMetres, Is.EqualTo(7f * Mathf.Sqrt(2f)).Within(.001f));
            Assert.That(sea.Body.linearVelocity, Is.EqualTo(new Vector3(2, 0, 1)));
            Assert.That(sea.Body.angularVelocity.y, Is.EqualTo(.15f).Within(.0001f));
            var joint = sea.Ship.GetComponent<ConfigurableJoint>();
            Assert.That(joint.anchor, Is.EqualTo(new Vector3(0, 1, 4)));
            Assert.That(joint.angularYMotion, Is.EqualTo(ConfigurableJointMotion.Free));
            Assert.That(joint.projectionMode, Is.EqualTo(JointProjectionMode.None));
        }

        [TestCase(6f, true)]
        [TestCase(10f, true)]
        [TestCase(10.1f, false)]
        [TestCase(15f, false)]
        public void CatchDepthIsMeasuredFromSeaLevelRatherThanTheBow(float depth, bool catches)
        {
            using var sea = new Anchorage(depth);
            sea.Body.position = Vector3.up * 5;
            sea.Anchor.SetDeployed(true);
            Assert.That(sea.Anchor.IsHolding, Is.EqualTo(catches));
        }

        [Test]
        public void DeepWaterDriftsThenCatchesAtTheCurrentBowWhenTerrainBecomesReachable()
        {
            using var sea = new Anchorage(15f);
            sea.Body.linearVelocity = new Vector3(2, 0, 1);
            sea.Anchor.SetDeployed(true);
            var original = sea.Anchor.DeploymentPosition;
            Assert.IsFalse(sea.Anchor.IsHolding);
            sea.Step(Vector3.right * 9, 10);
            Assert.That(sea.Body.linearVelocity.x, Is.GreaterThan(2f));
            Assert.That(sea.Body.linearVelocity.z, Is.GreaterThan(.9f));
            Assert.IsFalse(sea.Anchor.IsHolding);
            sea.Ground.transform.position += Vector3.up * 9;
            UnityEngine.Physics.SyncTransforms();
            var catchBow = sea.Anchor.BowWorldPosition;
            sea.Step(Vector3.right * 9);
            Assert.IsTrue(sea.Anchor.IsHolding);
            Assert.That(sea.Anchor.AnchorPosition.x, Is.EqualTo(catchBow.x).Within(.001f));
            Assert.That(sea.Anchor.AnchorPosition.x, Is.GreaterThan(original.x + .3f));
        }

        [Test]
        public void ShipAndOtherNonTerrainCollidersCannotCatchTheAnchor()
        {
            using var sea = new Anchorage(15f);
            var obstacle = new GameObject("Submerged non-terrain collider");
            SceneManager.MoveGameObjectToScene(obstacle, sea.Ship.scene);
            try
            {
                obstacle.transform.position = new Vector3(0, -4, 4);
                obstacle.AddComponent<BoxCollider>().size = new Vector3(30, 1, 30);
                UnityEngine.Physics.SyncTransforms();
                sea.Anchor.SetDeployed(true);
                Assert.IsFalse(sea.Anchor.IsHolding);
            }
            finally { Object.DestroyImmediate(obstacle); }
        }

        [Test]
        public void HoldingAnchorIsIdempotentAndUnfurlingImmediatelyReleasesIt()
        {
            using var sea = new Anchorage();
            sea.Anchor.SetDeployed(true);
            var original = sea.Anchor.AnchorPosition;
            sea.Body.position = Vector3.right;
            sea.Body.linearVelocity = Vector3.right * 3f;
            sea.Anchor.SetDeployed(true);
            Assert.That(sea.Anchor.AnchorPosition, Is.EqualTo(original));
            Assert.That(sea.Ship.GetComponents<ConfigurableJoint>().Length, Is.EqualTo(1));
            sea.Anchor.SetDeployed(false);
            Assert.IsFalse(sea.Anchor.IsDeployed);
            Assert.IsFalse(sea.Anchor.IsHolding);
            Assert.That(sea.Body.linearVelocity, Is.EqualTo(Vector3.right * 3f));
            sea.Step(Vector3.right * 9);
            Assert.That(sea.Body.linearVelocity.x, Is.EqualTo(3f).Within(.001f));
        }

        [Test]
        public void TeleportingRedeploysInsteadOfYankingTheShipBackToItsOldAnchor()
        {
            using var sea = new Anchorage();
            sea.Anchor.SetDeployed(true);
            sea.Body.position = Vector3.right * 30;
            sea.Anchor.ResetAfterTeleport();
            Assert.That(sea.Anchor.AnchorPosition.x, Is.EqualTo(30f).Within(.001f));
            Assert.IsTrue(sea.Anchor.IsHolding);
        }

        [Test]
        public void WindTakesUpSlackAndSwingsTheBowUpwindWithoutExceedingTheRode()
        {
            using var sea = new Anchorage();
            sea.Body.linearDamping = .08f;
            sea.Body.angularDamping = .6f;
            sea.Anchor.SetDeployed(true);
            sea.Step(Vector3.right * 9, 9000);
            Assert.That(sea.Body.position.x, Is.GreaterThan(2f), "Wind must carry the ship downwind through the slack.");
            Assert.That(Vector3.Dot(sea.Body.rotation * Vector3.forward, Vector3.left), Is.GreaterThan(.9f),
                "Bow tension and windage must turn the ship to face into the wind.");
            Assert.That(Vector3.Distance(sea.Anchor.BowWorldPosition, sea.Anchor.AnchorPosition),
                Is.LessThan(sea.Anchor.RodeLengthMetres + .1f));
        }

        [Test]
        public void LandCanObstructTheAnchoredSwing()
        {
            using var sea = new Anchorage();
            var land = new GameObject("Headland in the swing circle");
            SceneManager.MoveGameObjectToScene(land, sea.Ship.scene);
            try
            {
                land.transform.position = new Vector3(7, 0, 0);
                land.AddComponent<BoxCollider>().size = new Vector3(1, 20, 100);
                UnityEngine.Physics.SyncTransforms();
                sea.Body.linearDamping = .08f;
                sea.Body.angularDamping = .6f;
                sea.Anchor.SetDeployed(true);
                sea.Step(Vector3.right * 9, 6000);
                Assert.IsTrue(sea.Anchor.IsHolding);
                Assert.That(sea.Body.position.x, Is.InRange(1f, 6f), "The headland must physically stop the downwind swing.");
                Assert.That(Vector3.Distance(sea.Anchor.BowWorldPosition, sea.Anchor.AnchorPosition),
                    Is.LessThan(sea.Anchor.RodeLengthMetres + .1f));
            }
            finally { Object.DestroyImmediate(land); }
        }

        [TestCase(0f, 0f, 1f)]
        [TestCase(180f, 0f, 1f)]
        [TestCase(90f, 1f, 0f)]
        [TestCase(270f, 1f, 0f)]
        [TestCase(0f, 1f, 0f)]
        [TestCase(180f, 1f, 0f)]
        [TestCase(359f, 0f, 1f)]
        public void ExplorerMassAndBowTurnTowardsTheWindSourceFromEitherTackAndDirectlyDownwind(
            float heading, float windX, float windZ)
        {
            using var sea = new Anchorage();
            sea.Body.inertiaTensor = new Vector3(38694244, 38732496, 4011749);
            sea.Body.centerOfMass = new Vector3(0, 1.8f, -1.742657f);
            var rotation = Quaternion.Euler(0, heading, 0);
            sea.Ship.transform.SetPositionAndRotation(new Vector3(0, -3, 0), rotation);
            sea.Body.position = sea.Ship.transform.position; sea.Body.rotation = rotation;
            Set(sea.Anchor, "bowLocalPosition", new Vector3(0, 6, 14));
            UnityEngine.Physics.SyncTransforms();
            sea.Body.linearDamping = .08f;
            sea.Body.angularDamping = .6f;
            var flow = new Vector3(windX, 0, windZ) * 9f;
            sea.Anchor.SetDeployed(true);
            sea.Step(flow, 9000);
            var bow = Vector3.ProjectOnPlane(sea.Anchor.BowWorldPosition - sea.Body.worldCenterOfMass, Vector3.up).normalized;
            Assert.That(Vector3.Dot(bow, -flow.normalized), Is.GreaterThan(.95f), "Aim into the wind, opposite its travel direction.");
            Assert.That(Vector3.Distance(sea.Anchor.BowWorldPosition, sea.Anchor.AnchorPosition),
                Is.LessThan(sea.Anchor.RodeLengthMetres + .15f));
        }

        [Test]
        public void DeepWaterAlsoTurnsTheBowUpwindWhileTheAnchorDrags()
        {
            using var sea = new Anchorage(15);
            sea.Body.angularDamping = .6f;
            sea.Anchor.SetDeployed(true);
            sea.Step(Vector3.forward * 9f, 6000);
            Assert.IsFalse(sea.Anchor.IsHolding);
            Assert.That(Vector3.Dot(sea.Body.rotation * Vector3.forward, Vector3.back), Is.GreaterThan(.95f));
            Assert.That(sea.Body.position.z, Is.GreaterThan(1f), "Continue drifting downwind while facing the source.");
        }

        [Test]
        public void DropBottomContactAndFirstActualLoadAreDistinctOneShotEvents()
        {
            using var sea = new Anchorage();
            var events = new List<string>();
            sea.Anchor.Dropped += () => events.Add("drop");
            sea.Anchor.BottomContact += () => events.Add("bottom");
            sea.Anchor.Tensioned += () => events.Add("tension");
            sea.Anchor.SetDeployed(true);
            CollectionAssert.AreEqual(new[] { "drop", "bottom" }, events);
            Assert.IsFalse(sea.Anchor.IsTaut, "Touching the bottom is not yet a loaded rode.");
            sea.Step(Vector3.right * 9f, 6000);
            CollectionAssert.AreEqual(new[] { "drop", "bottom", "tension" }, events);
            sea.Anchor.SetDeployed(true);
            sea.Step(Vector3.left * 9f, 3000);
            Assert.That(events.Count, Is.EqualTo(3), "Wind turns and repeated furl commands must not replay first-load audio.");
            sea.Anchor.SetDeployed(false);
            sea.Anchor.SetDeployed(true);
            CollectionAssert.AreEqual(new[] { "drop", "bottom", "tension", "drop", "bottom" }, events);
        }

        [Test]
        public void DeepWaterOnlySignalsDropUntilTerrainActuallyCatches()
        {
            using var sea = new Anchorage(15);
            var events = new List<string>();
            sea.Anchor.Dropped += () => events.Add("drop");
            sea.Anchor.BottomContact += () => events.Add("bottom");
            sea.Anchor.Tensioned += () => events.Add("tension");
            sea.Anchor.SetDeployed(true);
            sea.Step(Vector3.right * 9, 100);
            CollectionAssert.AreEqual(new[] { "drop" }, events);
            sea.Ground.transform.position += Vector3.up * 9;
            UnityEngine.Physics.SyncTransforms();
            sea.Step(Vector3.right * 9);
            CollectionAssert.AreEqual(new[] { "drop", "bottom" }, events);
        }

        [Test]
        public void AnchorAudioUsesThreeIndependentSpatialVoicesAndStopsOnDisable()
        {
            using var sea = new Anchorage();
            var clips = new[] { AudioClip.Create("Drop test", 80000, 1, 8000, false),
                AudioClip.Create("Bottom test", 80000, 1, 8000, false), AudioClip.Create("Tension test", 80000, 1, 8000, false) };
            try
            {
                var audio = sea.Ship.AddComponent<ShipAnchorAudio>();
                Set(audio, "dropClip", clips[0]); Set(audio, "bottomContactClip", clips[1]); Set(audio, "tensionClip", clips[2]);
                sea.Anchor.SetDeployed(true);
                var sources = sea.Ship.GetComponentsInChildren<AudioSource>();
                Assert.That(sources.Length, Is.EqualTo(2), "Drop and seabed contact must not interrupt each other.");
                Assert.That(sources[0].isPlaying && sources[1].isPlaying, Is.True);
                Assert.That(Vector3.Distance(sources[1].transform.position, sea.Anchor.AnchorPosition), Is.LessThan(.001f));
                sea.Step(Vector3.right * 9, 6000);
                sources = sea.Ship.GetComponentsInChildren<AudioSource>();
                Assert.That(sources.Length, Is.EqualTo(3));
                Assert.That(sources[2].clip, Is.SameAs(clips[2]));
                foreach (var source in sources) Assert.That(source.spatialBlend, Is.EqualTo(1f));
                audio.enabled = false;
                foreach (var source in sources) Assert.IsFalse(source.isPlaying);
                sea.Anchor.SetDeployed(false); sea.Anchor.SetDeployed(true);
                foreach (var source in sources) Assert.IsFalse(source.isPlaying, "Disabled audio must unsubscribe from anchor events.");
            }
            finally { foreach (var clip in clips) Object.DestroyImmediate(clip); }
        }

        private static void Set(object target, string name, object value) => target.GetType()
            .GetField(name, BindingFlags.Instance | BindingFlags.NonPublic).SetValue(target, value);
    }
}
