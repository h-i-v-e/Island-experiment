using System.Collections;
using Motu.Gameplay;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;

namespace Motu.Editor
{
    public sealed class ShipControllerTests
    {
        [Test]
        public void ThrottleCoastsReversesAndLimitsAcceleration()
        {
            Assert.That(ShipController.DriveAcceleration(0, 5, 8, 3, 1.5f, 3), Is.Zero);
            Assert.That(ShipController.DriveAcceleration(1, 0, 8, 3, 1.5f, 3), Is.EqualTo(1.5f));
            Assert.That(ShipController.DriveAcceleration(-1, 5, 8, 3, 1.5f, 3), Is.EqualTo(-1.5f));
            Assert.That(ShipController.DriveAcceleration(1, 8, 8, 3, 1.5f, 3), Is.Zero);
        }

        [UnityTest]
        public IEnumerator HelmDrivesTheImportedAxesWithoutOverridingBuoyancy()
        {
            yield return new EnterPlayMode();
            var scene = SceneManager.CreateScene("Ship drive test", new CreateSceneParameters(LocalPhysicsMode.Physics3D));
            var host = new GameObject("Rotated imported hull");
            SceneManager.MoveGameObjectToScene(host, scene);
            try
            {
                host.transform.rotation = Quaternion.Euler(-90, 0, 0);
                host.transform.localScale = Vector3.one * 1578.6524f;
                host.AddComponent<BoxCollider>().size = new Vector3(.019f, .0045f, .0026f);
                var body = host.AddComponent<Rigidbody>();
                body.mass = 300000;
                body.useGravity = false;
                var helm = new GameObject("Helm").transform;
                helm.SetParent(host.transform, false);
                helm.rotation = Quaternion.LookRotation(Vector3.right, Vector3.up);
                var controller = host.AddComponent<ShipController>();
                controller.Configure(helm, null, null);
                controller.ReadPlayerInput = false;
                controller.enabled = false;
                controller.SetInput(1, 0);
                var physics = scene.GetPhysicsScene();
                controller.ApplyControls(0, .02f);
                physics.Simulate(.02f);
                Assert.That(body.linearVelocity.sqrMagnitude, Is.Zero, "No airborne propulsion.");
                for (var i = 0; i < 400; i++) { controller.ApplyControls(1, .02f); physics.Simulate(.02f); }
                Assert.That(body.position.x, Is.GreaterThan(20f), "Drive towards the model's bow (+X).");
                Assert.That(controller.SpeedMetresPerSecond, Is.InRange(5f, 8f));
                Assert.That(Mathf.Abs(body.position.y), Is.LessThan(.001f), "Thrust must leave heave to buoyancy.");
                controller.SetInput(0, 1);
                for (var i = 0; i < 100; i++) { controller.ApplyControls(1, .02f); physics.Simulate(.02f); }
                Assert.That(body.angularVelocity.y, Is.GreaterThan(.04f), "Starboard rudder turns right when moving ahead.");
                body.angularVelocity = Vector3.zero;
                body.linearVelocity = body.rotation * Quaternion.Inverse(Quaternion.Euler(-90, 0, 0)) * Vector3.left * 3;
                controller.ApplyControls(1, .02f); physics.Simulate(.02f);
                Assert.That(body.angularVelocity.y, Is.LessThan(0), "Rudder response reverses while going astern.");
                var beforeBrake = body.linearVelocity.magnitude;
                controller.SetInput(0, 0, true);
                for (var i = 0; i < 100; i++) { controller.ApplyControls(1, .02f); physics.Simulate(.02f); }
                Assert.That(body.linearVelocity.magnitude, Is.LessThan(beforeBrake * .05f));
            }
            finally { Object.DestroyImmediate(host); SceneManager.UnloadSceneAsync(scene); }
            yield return new ExitPlayMode();
        }

        [Test]
        public void MinimapTeleportsTheShipAndClearsMotion()
        {
            var hull = new GameObject("Ship teleport fixture");
            var hud = new GameObject("Ship HUD fixture");
            try
            {
                var body = hull.AddComponent<Rigidbody>();
                var ship = hull.AddComponent<ShipController>();
                body.position = new Vector3(0, 5, 0);
                body.linearVelocity = new Vector3(3, 0, 1);
                body.angularVelocity = Vector3.up;
                var demo = hud.AddComponent<IslandDemoController>();
                demo.ConfigureShipStart(ship, null);
                demo.minimapTexture = new Texture2D(1, 1);
                demo.hasMinimapCentre = true;
                demo.minimapCentreCell = Vector2Int.zero;
                var click = IslandDemoController.MinimapMapRect().center + Vector2.right * 7;
                Assert.IsTrue(demo.TryGetMinimapCell(click, out var cell));
                Assert.IsTrue(demo.HandleMinimapPointer(click, true, false, true));
                Assert.IsTrue(demo.HandleMinimapPointer(click, false, true, true));
                var destination = Motu.World.IslandWorldManager.CellCentre(cell);
                Assert.That(body.position.x, Is.EqualTo(destination.x));
                Assert.That(body.position.z, Is.EqualTo(destination.z));
                Assert.That(body.position.y, Is.EqualTo(5));
                Assert.AreEqual(Vector3.zero, body.linearVelocity);
                Assert.AreEqual(Vector3.zero, body.angularVelocity);
            }
            finally { Object.DestroyImmediate(hud); Object.DestroyImmediate(hull); }
        }

        [Test]
        public void OpenSeaSceneStartsAtTheBridge()
        {
            var scene = EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            var controller = Object.FindFirstObjectByType<ShipController>();
            Assert.IsNotNull(controller);
            var camera = controller.BridgeCamera;
            Assert.IsTrue(controller.enabled && camera.enabled && camera.GetComponent<Camera>().enabled);
            Assert.AreEqual(camera.GetComponent<Camera>(), Camera.main);
            var activeCameras = Object.FindObjectsByType<Camera>(FindObjectsSortMode.None);
            Assert.That(System.Array.FindAll(activeCameras, c => c.enabled).Length, Is.EqualTo(1));
            Assert.IsFalse(Object.FindFirstObjectByType<OrbitCamera>(FindObjectsInactive.Include).enabled);
            Assert.IsFalse(Object.FindFirstObjectByType<FirstPersonController>(FindObjectsInactive.Include).enabled);
            var manager = Object.FindFirstObjectByType<Motu.World.IslandWorldManager>();
            Assert.AreEqual(camera.transform, new SerializedObject(manager).FindProperty("streamingTarget").objectReferenceValue);
            var eye = camera.EyePosition;
            Assert.AreEqual(controller.transform, eye.parent);
            Assert.That(Vector3.Dot(eye.forward, Vector3.right), Is.GreaterThan(.999f));
            Assert.That(Vector3.Distance(camera.transform.position, eye.position), Is.LessThan(.001f));
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
        }
    }
}
