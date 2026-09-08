using System.Collections;
using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;

namespace Motu.Editor
{
    public sealed class OceanShipWaveTests
    {
        [UnityTest]
        public IEnumerator CruisingShipRendersBowWavesAndATrailingWake()
        {
            EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            yield return new EnterPlayMode();
            var ship = Object.FindAnyObjectByType<Motu.Gameplay.ShipController>();
            var emitter = ship.GetComponent<OceanDeckWaveClamp>();
            ship.ReadPlayerInput = false;
            ship.SetInput(1, 0);
            var until = Time.time + 12;
            while (Time.time < until) yield return null;
            Assert.That(ship.SpeedMetresPerSecond, Is.GreaterThan(3));
            Assert.That(emitter.TrailCount, Is.GreaterThan(5), "Runtime LateUpdate must deposit a wake as the Rigidbody moves.");
            var cameraObject = new GameObject("Ship wake validation camera");
            var camera = cameraObject.AddComponent<Camera>();
            var target = new RenderTexture(1280, 720, 24);
            var snapshot = new Texture2D(1280, 720, TextureFormat.RGB24, false);
            var previousTarget = RenderTexture.active;
            try
            {
                emitter.GetFootprint(out var centre, out var forward);
                var right = Vector3.Cross(Vector3.up, forward);
                camera.transform.position = centre - forward * 45 + Vector3.up * 40 + right * 30;
                camera.transform.LookAt(centre - forward * 15);
                camera.farClipPlane = 16000;
                camera.depthTextureMode = DepthTextureMode.Depth;
                camera.targetTexture = target;
                camera.Render();
                RenderTexture.active = target;
                snapshot.ReadPixels(new Rect(0, 0, 1280, 720), 0, 0);
                snapshot.Apply();
                System.IO.File.WriteAllBytes(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "motu-ship-wake-preview.png"), snapshot.EncodeToPNG());
                Assert.IsFalse(ShaderUtil.ShaderHasError(Shader.Find("Motu/Sea Water")));
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(cameraObject);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(snapshot);
            }
            yield return new ExitPlayMode();
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
        }

        [UnityTest]
        public IEnumerator TextureFieldClampsRaisesAndFollowsTheShipOnTheGpu()
        {
            yield return new EnterPlayMode();
            var host = new GameObject("Texture wave ship");
            var body = host.AddComponent<Rigidbody>();
            body.useGravity = false;
            host.transform.SetPositionAndRotation(new Vector3(100, -20, 200), Quaternion.Euler(0, 90, 0));
            host.transform.localScale = Vector3.one * 1000;
            var ship = host.AddComponent<OceanDeckWaveClamp>();
            ship.Configure(Vector3.back * .01f, Vector3.forward * .01f, 4, 2);
            var texture = new Texture2D(1, 3, TextureFormat.RGBA32, false, true);
            texture.filterMode = FilterMode.Point;
            texture.wrapMode = TextureWrapMode.Clamp;
            texture.SetPixels(new[] { Color.black, new Color(128f/255,128f/255,128f/255), Color.white });
            texture.Apply();
            ship.ConfigureTextures(texture, Texture2D.whiteTexture);
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ship Wave Field"));
            var input = new Texture2D(4, 1, TextureFormat.RGBAFloat, false, true) { filterMode = FilterMode.Point };
            var target = new RenderTexture(4, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(4, 1, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            try
            {
                input.SetPixels(new[] { new Color(86, 200, 2, 7), new Color(100, 200, 2, 7),
                    new Color(114, 200, 2, 7), new Color(86, 200, -2, 7) });
                input.Apply();
                Color[] Read(float now = 0)
                {
                    OceanShipWaveField.Render(new[] { ship }, new Vector3(100, 0, 200), now);
                    Graphics.Blit(input, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 4, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                body.linearVelocity = Vector3.right * 8;
                var pixels = Read();
                Assert.That(pixels[0].r, Is.EqualTo(7).Within(.01), "Black holds crests at sea level even with the ship below it.");
                Assert.That(pixels[1].r, Is.EqualTo(9).Within(.01), "8-bit mid-grey must be exactly neutral.");
                Assert.That(pixels[2].r, Is.EqualTo(10.2).Within(.01), "White bow faces the rotated ship's forward direction.");
                Assert.That(pixels[2].a, Is.GreaterThan(.8), "Raised bow generates foam.");
                Assert.That(pixels[3].r, Is.EqualTo(5).Within(.01), "Black leaves troughs alone.");
                body.linearVelocity = Vector3.zero;
                Assert.That(Read()[2].r, Is.EqualTo(9).Within(.01), "Stopped ship has no bow wave.");
                body.linearVelocity = Vector3.left * 8;
                Assert.That(Read()[2].r, Is.EqualTo(9).Within(.01), "Reverse motion does not generate a wave ahead of the bow.");
                host.transform.position += Vector3.right * 80;
                Assert.That(Read()[0].r, Is.EqualTo(9).Within(.01), "Old hull footprint clears after moving.");
                // A deposited white section stays put and fades instead of following a turn.
                ship.AdvanceTrail(new Vector3(100, 0, 200), Vector3.right, 8, 0);
                ship.AdvanceTrail(new Vector3(106, 0, 200), Vector3.right, 8, 1);
                Assert.That(Read(1)[1].r, Is.GreaterThan(9.5));
                Assert.That(Read(11)[1].r, Is.InRange(9.1f, 9.3f));
                Assert.That(Read(22)[1].r, Is.EqualTo(9).Within(.01));
                Assert.IsFalse(ShaderUtil.ShaderHasError(Resources.Load<Shader>("OceanShipWaveStamp")));
                Assert.IsFalse(ShaderUtil.ShaderHasError(Shader.Find("Motu/Sea Water")));
            }
            finally
            {
                RenderTexture.active = oldTarget;
                Object.DestroyImmediate(host);
                Object.DestroyImmediate(texture);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(input);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
            Assert.That(Shader.GetGlobalFloat("_MotuShipWaveEnabled"), Is.Zero);
            yield return new ExitPlayMode();
        }

        [Test]
        public void WakeRetainsItsPathExpiresAndResetsOnTeleport()
        {
            var host = new GameObject("Wake trail fixture");
            try
            {
                var ship = host.AddComponent<OceanDeckWaveClamp>();
                ship.AdvanceTrail(Vector3.zero, Vector3.forward, 8, 0);
                ship.AdvanceTrail(Vector3.forward * 9, Vector3.forward, 8, 1);
                Assert.That(ship.TrailCount, Is.EqualTo(3));
                var first = ship.TrailAt(0);
                ship.AdvanceTrail(new Vector3(9, 0, 9), Vector3.right, 8, 2);
                Assert.That(ship.TrailAt(0).Position, Is.EqualTo(first.Position));
                Assert.That(ship.TrailAt(0).Forward, Is.EqualTo(Vector3.forward));
                Assert.That(ship.TrailAt(ship.TrailCount-1).Forward, Is.EqualTo(Vector3.right));
                ship.AdvanceTrail(new Vector3(1000, 0, 9), Vector3.right, 8, 3);
                Assert.That(ship.TrailCount, Is.Zero, "Teleport clears old trail without connecting destinations.");
                ship.AdvanceTrail(new Vector3(1009, 0, 9), Vector3.right, 8, 4);
                Assert.That(ship.TrailCount, Is.EqualTo(3));
                ship.AdvanceTrail(new Vector3(1009, 0, 9), Vector3.right, 0, 25);
                Assert.That(ship.TrailCount, Is.Zero, "Old wake expires even when stationary.");
            }
            finally { Object.DestroyImmediate(host); }
        }

        [Test]
        public void OpenSeaShipUsesLinearUncompressedWaveTextures()
        {
            EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            try
            {
                var ships = Object.FindObjectsByType<OceanDeckWaveClamp>(FindObjectsSortMode.None);
                Assert.That(ships, Is.Not.Empty);
                foreach (var ship in ships)
                foreach (var texture in new[] { ship.HullTexture, ship.WakeTexture })
                {
                    if (texture == null) continue;
                    var path = AssetDatabase.GetAssetPath(texture);
                    var importer = (TextureImporter)AssetImporter.GetAtPath(path);
                    Assert.AreEqual(TextureImporterShape.Texture2D, importer.textureShape);
                    Assert.IsFalse(importer.sRGBTexture, "Gamma conversion changes neutral grey into a negative displacement.");
                    Assert.AreEqual(TextureImporterCompression.Uncompressed, importer.textureCompression);
                }
            }
            finally { EditorSceneManager.NewScene(NewSceneSetup.EmptyScene); }
        }
    }
}
