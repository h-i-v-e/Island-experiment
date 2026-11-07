using System;
using System.Collections;
using Motu.Gameplay;
using Motu.Islands;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class CoherentWorldStartupTests
    {
        [UnityTest]
        public IEnumerator ShipStartupPreservesHullPoseAndMovesTheBridgeAndAnchorToOpenSea()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            using (var fixture = new StartupFixture(true))
            {
                var hull = fixture.Child("Authored ship");
                hull.transform.SetPositionAndRotation(new Vector3(123f, 5f, 245f),
                    Quaternion.Euler(-90f, 25f, 0f));
                hull.transform.localScale = Vector3.one * 1578.6524f;
                var body = hull.AddComponent<Rigidbody>();
                body.useGravity = false;
                var ship = hull.AddComponent<ShipController>();
                hull.GetComponent<OceanBuoyancy>().enabled = false;
                var anchor = hull.AddComponent<ShipAnchor>();
                anchor.enabled = false;
                anchor.SetDeployed(true);
                var originalDrop = anchor.DeploymentPosition;
                var eye = fixture.Child("Bridge eye").transform;
                eye.SetParent(hull.transform, false);
                eye.localPosition = new Vector3(.002f, 0f, .008f);
                var bridgeObject = fixture.Child("Detached bridge camera");
                var bridgeCamera = bridgeObject.AddComponent<Camera>();
                bridgeCamera.enabled = false;
                var bridge = bridgeObject.AddComponent<ShipBridgeCamera>();
                bridge.Configure(eye, body);
                ship.Configure(eye, bridge, fixture.Manager);
                fixture.Demo.ConfigureShipStart(ship, bridgeCamera);
                var authoredRotation = hull.transform.rotation;
                var authoredScale = hull.transform.localScale;
                body.linearVelocity = new Vector3(3f, 0f, 1f);
                body.angularVelocity = Vector3.up;

                fixture.Activate();

                var expected = fixture.Factory.StartingPosition(5f);
                Assert.That(Vector3.Distance(body.position, expected), Is.LessThan(.001f));
                Assert.That(Quaternion.Angle(body.rotation, authoredRotation), Is.LessThan(.001f));
                Assert.That(hull.transform.localScale, Is.EqualTo(authoredScale));
                Assert.That(body.linearVelocity, Is.EqualTo(Vector3.zero));
                Assert.That(body.angularVelocity, Is.EqualTo(Vector3.zero));
                Assert.That(anchor.IsDeployed, Is.True);
                Assert.That(Vector3.Distance(anchor.DeploymentPosition, originalDrop), Is.GreaterThan(1000f));
                Assert.That(Vector3.Distance(anchor.DeploymentPosition, anchor.BowWorldPosition), Is.LessThan(.001f));
                var expectedEye = body.position + body.rotation * Vector3.Scale(eye.localPosition, hull.transform.lossyScale);
                Assert.That(Vector3.Distance(bridge.transform.position, expectedEye), Is.LessThan(.01f));
                fixture.AssertReservedCell(body.position);
                fixture.AssertStreamingTarget(bridge.transform);
                Assert.That(fixture.Demo.IsAtHelm, Is.True);
                Assert.That(fixture.Orbit.enabled || fixture.Player.enabled, Is.False);

                yield return null; // Exercise the real ShipController and bridge Start callbacks too.

                fixture.AssertReservedCell(body.position);
                fixture.AssertStreamingTarget(bridge.transform);
                Assert.That(fixture.Demo.IsAtHelm, Is.True);
            }
            yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator CoherentWorldUsesReservedCellForBothFlyAndOverviewStartup()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            foreach (var requestFlyMode in new[] { false, true })
            {
                using var fixture = new StartupFixture(true);
                fixture.Demo.ConfigureFlyStart(requestFlyMode, new Vector3(421f, 27f, -791f), 70f, 12f);

                fixture.Activate();

                Assert.That(fixture.Orbit.Target, Is.EqualTo(fixture.Factory.StartingPosition(60f)));
                yield return null; // Demo.Start must activate the player even without the fly-mode flag.

                Assert.That(fixture.Player.IsActive && fixture.Player.IsFlyMode, Is.True);
                Assert.That(fixture.Player.FollowsTerrainInFlyMode, Is.False);
                Assert.That(fixture.Orbit.enabled, Is.False);
                Assert.That(Vector3.Distance(fixture.Player.transform.position,
                    fixture.Factory.StartingPosition(27f)), Is.LessThan(.001f));
                Assert.That(Quaternion.Angle(fixture.Player.transform.rotation, Quaternion.Euler(12f, 70f, 0f)),
                    Is.LessThan(.001f));
                fixture.AssertReservedCell(fixture.Player.transform.position);
                fixture.AssertStreamingTarget(fixture.Player.transform);
            }
            yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator OtherFactoriesKeepTheirAuthoredFlyStartAndTerrainFollowing()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            using (var fixture = new StartupFixture(false))
            {
                var authoredStart = new Vector3(421f, 4f, -791f);
                fixture.Demo.ConfigureFlyStart(true, authoredStart);
                fixture.Activate();
                Assert.That(fixture.Orbit.Target, Is.EqualTo(new Vector3(0f, 60f, 0f)));

                yield return null;

                Assert.That(fixture.Player.IsActive && fixture.Player.IsFlyMode, Is.True);
                Assert.That(fixture.Player.FollowsTerrainInFlyMode, Is.True);
                Assert.That(Vector3.Distance(fixture.Player.transform.position, authoredStart), Is.LessThan(.001f));
                fixture.AssertStreamingTarget(fixture.Player.transform);
            }
            yield return new ExitPlayMode();
        }

        private sealed class StartupFixture : IDisposable
        {
            private readonly GameObject root = new GameObject("World startup fixture");
            private readonly GameObject world = new GameObject("Inactive world queries");
            private readonly CursorLockMode cursorLock = Cursor.lockState;
            private readonly bool cursorVisible = Cursor.visible;
            public IslandWorldManager Manager { get; }
            public CoherentIslandFactory Factory { get; }
            public IslandDemoController Demo { get; }
            public OrbitCamera Orbit { get; }
            public FirstPersonController Player { get; }

            public StartupFixture(bool coherent)
            {
                root.SetActive(false);
                // Keep discovery and environment initialization out of this startup test.
                // The configured real query object is sufficient for the player callbacks.
                world.SetActive(false);
                Manager = world.AddComponent<IslandWorldManager>();
                if (coherent)
                {
                    Factory = world.AddComponent<CoherentIslandFactory>();
                    JsonUtility.FromJsonOverwrite("{\"archipelegoRadius\":4,\"seed\":1840}", Factory);
                    Manager.ConfigureIslandGenerationRequestFactory(Factory);
                }
                else
                {
                    var factory = world.AddComponent<GridIslandGenerationRequestFactory>();
                    factory.Configure(42, false, 0f);
                    Manager.ConfigureIslandGenerationRequestFactory(factory);
                }
                var flight = Child("Flight camera");
                var camera = flight.AddComponent<Camera>();
                camera.enabled = false;
                Orbit = flight.AddComponent<OrbitCamera>();
                Player = flight.AddComponent<FirstPersonController>();
                Demo = flight.AddComponent<IslandDemoController>();
                Demo.Configure(Manager, camera, Orbit, Player);
                Demo.ShowMinimap = false;
            }

            public GameObject Child(string name)
            {
                var child = new GameObject(name);
                child.transform.SetParent(root.transform, false);
                return child;
            }

            public void Activate() => root.SetActive(true);

            public void AssertReservedCell(Vector3 position)
            {
                Assert.That(IslandWorldManager.WorldToCell(position), Is.EqualTo(Factory.StartingCell));
                Assert.That(Factory.HasIsland(Factory.StartingCell), Is.False);
            }

            public void AssertStreamingTarget(Transform target) => Assert.That(
                new SerializedObject(Manager).FindProperty("streamingTarget").objectReferenceValue, Is.EqualTo(target));

            public void Dispose()
            {
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(world);
                Cursor.lockState = cursorLock;
                Cursor.visible = cursorVisible;
            }
        }
    }
}
