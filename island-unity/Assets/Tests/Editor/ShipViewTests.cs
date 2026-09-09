using Motu.Gameplay;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    public sealed class ShipViewTests
    {
        private const string ShipFolder = "Assets/MeshyImports/Pirate Ship High Poly_20260908_110931/";

        [TestCase(true, false)]
        [TestCase(false, false)]
        [TestCase(false, true)]
        public void PlayerLeavesShipBehindAndRemovesItOutsideTheCurrentCell(bool teleportFromHelm, bool flyAcrossBoundary)
        {
            var root = new GameObject("Camera switching fixture");
            root.SetActive(false);
            var cursorLock = Cursor.lockState;
            var cursorVisible = Cursor.visible;
            try
            {
                GameObject Child(string name)
                {
                    var child = new GameObject(name);
                    child.transform.SetParent(root.transform, false);
                    return child;
                }
                var manager = root.AddComponent<IslandWorldManager>();
                var flight = Child("Flight camera");
                var flyingCamera = flight.AddComponent<Camera>();
                flyingCamera.enabled = false;
                var orbit = flight.AddComponent<OrbitCamera>();
                orbit.enabled = false;
                var player = flight.AddComponent<FirstPersonController>();
                var flyReflection = flight.AddComponent<PlanarWaterReflection>();
                flyReflection.enabled = false;
                var demo = flight.AddComponent<IslandDemoController>();
                demo.Configure(manager, flyingCamera, orbit, player);
                var hull = Child("Ship");
                var body = hull.AddComponent<Rigidbody>();
                var ship = hull.AddComponent<ShipController>();
                var eye = Child("Bridge eye").transform;
                eye.SetParent(hull.transform, false);
                eye.localPosition = new Vector3(2, 12, 0);
                var bridgeObject = Child("Bridge camera");
                var bridgeCamera = bridgeObject.AddComponent<Camera>();
                bridgeObject.tag = "MainCamera";
                var bridge = bridgeObject.AddComponent<ShipBridgeCamera>();
                var bridgeReflection = bridgeObject.AddComponent<PlanarWaterReflection>();
                bridge.Configure(eye, body);
                ship.Configure(eye, bridge, manager);
                demo.ConfigureShipStart(ship, bridgeCamera);
                var start = bridge.transform.position;
                body.linearVelocity = new Vector3(2,0,1);
                var velocity = body.linearVelocity;
                Assert.AreEqual(KeyCode.F, demo.SwitchCameraKey);
                Assert.IsTrue(demo.ToggleShipCamera());
                Assert.IsFalse(demo.IsAtHelm);
                Assert.IsTrue(player.IsActive && player.IsFlyMode && !player.FollowsTerrainInFlyMode);
                Assert.AreEqual(start, player.transform.position, "Keep the bridge eye height.");
                Assert.IsFalse(ship.enabled || bridge.enabled || bridgeCamera.enabled || orbit.enabled);
                Assert.IsTrue(hull.GetComponent<OceanBuoyancy>().enabled);
                Assert.IsFalse(body.isKinematic);
                Assert.AreEqual(velocity, body.linearVelocity);
                Assert.IsTrue(flyingCamera.enabled && flyingCamera.CompareTag("MainCamera"));
                Assert.IsTrue(flyReflection.enabled && !bridgeReflection.enabled);
                Assert.AreEqual(player.transform, new SerializedObject(manager).FindProperty("streamingTarget").objectReferenceValue);

                demo.minimapTexture = new Texture2D(1,1);
                demo.hasMinimapCentre = true;
                demo.minimapCentreCell = Vector2Int.zero;
                var click = IslandDemoController.MinimapMapRect().center;
                var shipPosition = body.position;
                demo.HandleMinimapPointer(click, true, false, true);
                demo.HandleMinimapPointer(click, false, true, true);
                Assert.AreEqual(shipPosition, body.position, "Flying minimap clicks must not teleport the ship.");
                Assert.That(player.transform.position.x, Is.EqualTo(0));
                Assert.IsTrue(ship != null, "Same-cell teleport must retain the ship.");
                Assert.IsTrue(player.IsCursorReleased);
                Assert.AreEqual(start.y, player.transform.position.y);
                Assert.IsFalse(player.FollowsTerrainInFlyMode);

                body.position = new Vector3(50,0,25);
                Assert.IsTrue(demo.ToggleShipCamera());
                Assert.IsTrue(demo.IsAtHelm && ship.enabled && bridge.enabled && bridgeCamera.enabled);
                Assert.IsFalse(player.IsActive || player.enabled || flyingCamera.enabled || orbit.enabled);
                Assert.AreEqual(body.position + eye.localPosition, bridge.transform.position);
                Assert.IsTrue(bridgeCamera.CompareTag("MainCamera") && !flyingCamera.CompareTag("MainCamera"));
                Assert.IsTrue(bridgeReflection.enabled && !flyReflection.enabled);
                Assert.AreEqual(bridge.transform, new SerializedObject(manager).FindProperty("streamingTarget").objectReferenceValue);
                Assert.IsTrue(demo.ToggleShipCamera(), "Repeated switching must remain usable.");
                Assert.IsTrue(demo.ToggleShipCamera());
                if (!teleportFromHelm) Assert.IsTrue(demo.ToggleShipCamera());
                if (flyAcrossBoundary)
                {
                    player.transform.position = IslandWorldManager.CellCentre(Vector2Int.right);
                    demo.RemoveShipOutsidePlayerCell();
                }
                else
                {
                    click = IslandDemoController.MinimapMapRect().center + Vector2.right * 7;
                    demo.HandleMinimapPointer(click, true, false, true);
                    demo.HandleMinimapPointer(click, false, true, true);
                    Assert.That(player.transform.position.x, Is.GreaterThan(1000));
                    Assert.IsTrue(player.IsCursorReleased, "Keep the minimap cursor available after leaving the helm.");
                }
                Assert.IsTrue(hull == null && bridgeObject == null, "Remove the ship and detached bridge camera from the hierarchy.");
                Assert.IsFalse(demo.IsAtHelm || demo.ToggleShipCamera(), "The removed ship must not remain a return target.");
                Assert.IsTrue(player.IsActive && flyingCamera.enabled && flyingCamera.CompareTag("MainCamera"));
                Assert.AreEqual(player.transform, new SerializedObject(manager).FindProperty("streamingTarget").objectReferenceValue);
                demo.RemoveShipOutsidePlayerCell(); // Repeated cleanup is harmless.
            }
            finally
            {
                Object.DestroyImmediate(root);
                Cursor.lockState = cursorLock;
                Cursor.visible = cursorVisible;
            }
        }

        [Test]
        public void ImportedShipUsesTheTexturedMatteMaterial()
        {
            AssetDatabase.ImportAsset(ShipFolder + "Meshy_AI_Pirate_Ship_High_Poly_0907230920_texture.fbx", ImportAssetOptions.ForceUpdate);
            var model = AssetDatabase.LoadAssetAtPath<GameObject>(ShipFolder + "Meshy_AI_Pirate_Ship_High_Poly_0907230920_texture.fbx");
            var material = AssetDatabase.LoadAssetAtPath<Material>(ShipFolder + "Material.001.mat");
            var texture = AssetDatabase.LoadAssetAtPath<Texture2D>(ShipFolder + "meshy_basecolor.png");
            Assert.IsNotNull(model);
            Assert.IsNotNull(texture);
            Assert.AreEqual(texture, material.mainTexture);
            var renderers = model.GetComponentsInChildren<Renderer>(true);
            Assert.IsNotEmpty(renderers);
            foreach (var renderer in renderers)
                foreach (var assigned in renderer.sharedMaterials)
                    Assert.AreEqual(material, assigned,
                        $"{renderer.name}: material {assigned?.name}, asset {AssetDatabase.GetAssetPath(assigned)}, texture {assigned?.mainTexture?.name}");
            Assert.IsNull(material.GetTexture("_MetallicGlossMap"));
            Assert.That(material.GetFloat("_Metallic"), Is.Zero);
            Assert.That(material.GetFloat("_Glossiness"), Is.Zero);
        }
    }
}
