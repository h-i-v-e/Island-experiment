using Motu.Gameplay;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Editor
{
    public sealed class PlayerTorchTests
    {
        private sealed class FlatSurface : IWorldSurfaceQuery
        {
            public void SetStreamingTarget(Transform target) { }
            public void PrepareStreamingAt(Vector3 position) { }
            public bool TrySnapToTerrain(Vector3 position, out Vector3 ground)
            {
                ground = new Vector3(position.x, 0, position.z);
                return true;
            }
            public float GetTerrainOrSeaHeight(Vector3 position) => 0;
            public void SetFirstPersonViewActive(bool active) { }
        }

        [Test]
        public void TorchFollowsTheViewAndRemembersItsToggleAcrossCameraSwitches()
        {
            var root = new GameObject("Torch lifecycle test");
            root.SetActive(false);
            var cursorLock = Cursor.lockState;
            var cursorVisible = Cursor.visible;
            try
            {
                root.AddComponent<Camera>();
                var orbit = root.AddComponent<OrbitCamera>();
                orbit.enabled = false;
                var player = root.AddComponent<FirstPersonController>();
                player.Configure(orbit, new FlatSurface());
                player.BeginFlying(new Vector3(0, 10, 0), followTerrain: false);
                root.SetActive(true);
                Assert.AreEqual(KeyCode.T, player.ToggleTorchKey);
                Assert.IsFalse(player.IsTorchOn);
                player.ToggleTorch();
                Assert.IsTrue(player.IsTorchOn);
                var light = root.GetComponentInChildren<Light>();
                Assert.AreEqual(LightType.Spot, light.type);
                root.transform.rotation = Quaternion.Euler(20, 70, 0);
                Assert.Less(Vector3.Distance(light.transform.forward, root.transform.forward), .0001f);
                player.Exit();
                Assert.IsFalse(player.IsTorchOn);
                player.EnterPreparedGround(Vector3.zero);
                Assert.IsTrue(player.IsTorchOn, "Restore the torch when returning to the player camera.");
                player.ToggleTorch();
                Assert.IsFalse(player.IsTorchOn);
                player.ToggleTorch();
                Assert.AreEqual(1, root.GetComponentsInChildren<Light>().Length);
                root.SetActive(false);
                Assert.IsFalse(player.IsTorchOn);
            }
            finally
            {
                Object.DestroyImmediate(root);
                Cursor.lockState = cursorLock;
                Cursor.visible = cursorVisible;
            }
        }

        [Test]
        public void TorchIlluminatesCaveStoneWithTheSunStillPresent()
        {
            var root = new GameObject("Torch rendering test");
            var material = new Material(Resources.Load<Shader>("CaveSurface"));
            var target = new RenderTexture(64, 64, 24);
            var pixels = new Texture2D(64, 64, TextureFormat.RGB24, false);
            var previousTarget = RenderTexture.active;
            var previousFog = RenderSettings.fog;
            var previousAmbientMode = RenderSettings.ambientMode;
            var previousAmbient = RenderSettings.ambientLight;
            var previousSun = RenderSettings.sun;
            try
            {
                var wall = GameObject.CreatePrimitive(PrimitiveType.Quad);
                wall.transform.SetParent(root.transform, false);
                wall.layer = 31;
                material.SetFloat("_UseTextures", 0);
                material.SetFloat("_CliffNormalStrength", 0);
                material.SetColor("_RockColour", new Color(.3f, .32f, .29f));
                wall.GetComponent<Renderer>().sharedMaterial = material;
                var eye = new GameObject("Eye");
                eye.transform.SetParent(root.transform, false);
                eye.transform.localPosition = new Vector3(0, 0, -3);
                var camera = eye.AddComponent<Camera>();
                camera.renderingPath = RenderingPath.Forward;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.targetTexture = target;
                var sunObject = new GameObject("Sun facing away from wall");
                sunObject.transform.SetParent(root.transform, false);
                sunObject.transform.rotation = Quaternion.Euler(0, 180, 0);
                var sun = sunObject.AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 1;
                sun.cullingMask = 1 << 31;
                RenderSettings.sun = sun;
                var lamp = new GameObject("Torch");
                lamp.transform.SetParent(eye.transform, false);
                var torch = lamp.AddComponent<Light>();
                torch.type = LightType.Spot;
                torch.renderMode = LightRenderMode.ForcePixel;
                torch.range = 35;
                torch.spotAngle = 75;
                torch.intensity = 4;
                torch.shadows = LightShadows.Soft;
                torch.cullingMask = 1 << 31;
                RenderSettings.fog = false;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = Color.black;
                float Brightness()
                {
                    camera.Render();
                    RenderTexture.active = target;
                    pixels.ReadPixels(new Rect(0, 0, 64, 64), 0, 0);
                    pixels.Apply();
                    return pixels.GetPixel(32, 32).grayscale;
                }
                torch.enabled = false;
                var dark = Brightness();
                torch.enabled = true;
                var lit = Brightness();
                Assert.Greater(lit, dark + .15f, "The spotlight must light the cave independently of the sun.");
                torch.transform.rotation = Quaternion.Euler(0, 180, 0);
                Assert.Less(Brightness(), lit - .15f, "The beam must follow the viewing direction.");
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = previousTarget;
                RenderSettings.fog = previousFog;
                RenderSettings.ambientMode = previousAmbientMode;
                RenderSettings.ambientLight = previousAmbient;
                RenderSettings.sun = previousSun;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(pixels);
            }
        }
    }
}
