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
        [Test]
        public void DisplacementFoamTracksActualHeightChangeIncludingHullClamping()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ship Wave Field"));
            var field = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var input = new Texture2D(4, 1, TextureFormat.RGBAFloat, false, true) { filterMode = FilterMode.Point };
            var target = new RenderTexture(4, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(4, 1, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            var oldField = Shader.GetGlobalTexture("_MotuShipWaveField");
            var oldRect = Shader.GetGlobalVector("_MotuShipWaveRect");
            var oldEnabled = Shader.GetGlobalFloat("_MotuShipWaveEnabled");
            try
            {
                Shader.SetGlobalTexture("_MotuShipWaveField", field);
                Shader.SetGlobalVector("_MotuShipWaveRect", new Vector4(0, 0, 1, 1));
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 1);
                material.SetFloat("_ProbeDisplacementFoam", 1);
                material.SetFloat("_BoatDisplacementFoamStrength", 1);
                input.SetPixels(new[] { new Color(.5f, .5f, .25f, 0), new Color(.5f, .5f, .5f, 0),
                    new Color(.5f, .5f, -.5f, 0), new Color(.5f, .5f, 2, 0) });
                input.Apply();
                Color[] Read(Color fieldValue)
                {
                    field.SetPixel(0, 0, fieldValue);
                    field.Apply();
                    Graphics.Blit(input, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 4, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                material.SetFloat("_ProbeMaximumWaveHeight", 2);
                var rim = Read(new Color(.5f, 0, 0, 1)); // 1 metre ceiling.
                Assert.That(rim[0].r, Is.EqualTo(.25f).Within(.0001f));
                Assert.That(rim[1].r, Is.EqualTo(.5f).Within(.0001f));
                Assert.That(rim[2].r, Is.EqualTo(-.5f).Within(.0001f));
                Assert.That(rim[3].r, Is.EqualTo(1).Within(.0001f), "Only crests above the ceiling should be trimmed.");
                for (var i = 0; i < 3; i++)
                    Assert.That(rim[i].b, Is.Zero, "Waves below the ceiling should not generate clamp foam.");
                Assert.That(rim[3].b, Is.EqualTo(1).Within(.0001f));
                var lowerRim = Read(new Color(.75f, 0, 0, 1)); // 0.5 metre ceiling.
                Assert.That(lowerRim[0].r, Is.EqualTo(.25f).Within(.0001f));
                Assert.That(lowerRim[1].b, Is.Zero, "A wave exactly at the ceiling is unchanged.");
                Assert.That(lowerRim[3].r, Is.EqualTo(.5f).Within(.0001f));
                var hull = Read(new Color(1, 0, 0, 1));
                Assert.That(hull[0].b, Is.EqualTo(.25f).Within(.0001f));
                Assert.That(hull[1].b, Is.EqualTo(hull[0].b * 2).Within(.0001f));
                Assert.That(hull[2].b, Is.Zero, "A trough unchanged by the hull should produce no displacement foam.");
                Assert.That(hull[3].b, Is.EqualTo(1).Within(.0001f));
                var bow = Read(new Color(0, .5f, 0, 1));
                foreach (var pixel in bow)
                    Assert.That(pixel.b, Is.EqualTo(.5f).Within(.0001f), "Raised bow waves should foam too.");
                foreach (var pixel in Read(Color.clear)) Assert.That(pixel.b, Is.Zero);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                foreach (var pixel in Read(new Color(1, .5f, 0, 1))) Assert.That(pixel.b, Is.Zero);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 1);
                material.SetFloat("_BoatDisplacementFoamStrength", 0);
                foreach (var pixel in Read(new Color(1, .5f, 0, 1))) Assert.That(pixel.b, Is.Zero);
            }
            finally
            {
                RenderTexture.active = oldTarget;
                Shader.SetGlobalTexture("_MotuShipWaveField", oldField);
                Shader.SetGlobalVector("_MotuShipWaveRect", oldRect);
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", oldEnabled);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(field);
                Object.DestroyImmediate(input);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
        }

        [Test]
        public void HullClampShrinksAndFeathersBeforeReachingItsFlatCore()
        {
            var host = new GameObject("Soft hull clamp test");
            var ship = host.AddComponent<OceanDeckWaveClamp>();
            var texture = new Texture2D(128, 1, TextureFormat.RGBA32, false, true)
                { filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp };
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ship Wave Field"));
            var input = new Texture2D(65, 1, TextureFormat.RGBAFloat, false, true) { filterMode = FilterMode.Point };
            var target = new RenderTexture(65, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(65, 1, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            try
            {
                var colours = new Color[128];
                for (var i = 0; i < colours.Length; i++)
                    colours[i] = Mathf.Abs((i + .5f) / 128 - .5f) < .3f ? Color.black : new Color(128f / 255, 128f / 255, 128f / 255);
                texture.SetPixels(colours);
                texture.Apply();
                ship.FitGeneratedHullTexture(texture, Vector3.zero, Vector3.forward, new Vector2(12, 12), new Vector2(20, 20));
                var positions = new Color[65];
                for (var i = 0; i < positions.Length; i++) positions[i] = new Color(i * 10f / 64, 0, 2, 0);
                input.SetPixels(positions);
                input.Apply();
                Color[] Read(float scale, float feather)
                {
                    ship.HullClampFootprintScale = scale;
                    ship.HullClampFeatherMetres = feather;
                    OceanShipWaveField.Render(new[] { ship }, Vector3.zero, 0);
                    Graphics.Blit(input, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 65, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                var original = Read(1, 0);
                var shrunken = Read(.9f, 0);
                var softened = Read(.9f, 3);
                int Edge(Color[] values)
                {
                    var last = 0;
                    for (var i = 0; i < values.Length; i++) if (values[i].g > .5f) last = i;
                    return last;
                }
                Assert.That(Edge(shrunken), Is.LessThan(Edge(original) - 2), "The full clamp footprint should move inward.");
                Assert.That(softened[0].r, Is.EqualTo(0).Within(.001f), "The deep interior still flattens crests to the mean plane.");
                Assert.That(softened[26].g, Is.InRange(.05f, .95f), "The waterline rim should blend down gradually.");
                Assert.That(softened[64].g, Is.EqualTo(0).Within(.001f));
                var originalStep = 0f;
                var softenedStep = 0f;
                for (var i = 1; i < softened.Length; i++)
                {
                    originalStep = Mathf.Max(originalStep, Mathf.Abs(original[i].g - original[i-1].g));
                    softenedStep = Mathf.Max(softenedStep, Mathf.Abs(softened[i].g - softened[i-1].g));
                }
                Assert.That(softenedStep, Is.LessThan(originalStep * .8f), "Feathering must reduce the steepest clamp gradient.");
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(host);
                Object.DestroyImmediate(texture);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(input);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
        }

        [UnityTest]
        public IEnumerator BowWavePullsBackAndRisesGentlyFromItsTip()
        {
            yield return new EnterPlayMode();
            var host = new GameObject("Bow rise test");
            var body = host.AddComponent<Rigidbody>();
            body.useGravity = false;
            body.linearVelocity = Vector3.forward * 8;
            var ship = host.AddComponent<OceanDeckWaveClamp>();
            var texture = new Texture2D(1, 256, TextureFormat.RGBA32, false, true)
                { filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp };
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ship Wave Field"));
            var input = new Texture2D(6, 1, TextureFormat.RGBAFloat, false, true) { filterMode = FilterMode.Point };
            var target = new RenderTexture(6, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(6, 1, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            try
            {
                var colours = new Color[256];
                for (var i = 0; i < colours.Length; i++)
                {
                    var z = ((i + .5f) / 256 - .5f) * 40;
                    var ridge = Mathf.Exp(-Mathf.Pow((z - 14) / 2, 2));
                    var grey = (128 + ridge * 127) / 255;
                    colours[i] = new Color(grey, grey, grey);
                }
                texture.SetPixels(colours);
                texture.Apply();
                ship.FitGeneratedHullTexture(texture, Vector3.zero, Vector3.forward, new Vector2(8, 20), new Vector2(20, 40));
                input.SetPixels(new[] { new Color(0, 8, 0, 0), new Color(0, 10, 0, 0), new Color(0, 11, 0, 0),
                    new Color(0, 12, 0, 0), new Color(0, 14, 0, 0), new Color(0, 16, 0, 0) });
                input.Apply();
                Color[] Read(float pullback, float tipBlend)
                {
                    ship.BowWavePullbackMetres = pullback;
                    ship.BowWaveTipBlendMetres = tipBlend;
                    OceanShipWaveField.Render(new[] { ship }, Vector3.zero, 0);
                    Graphics.Blit(input, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 6, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                var original = Read(0, 0);
                var pulled = Read(3.5f, 0);
                var shaped = Read(3.5f, 2);
                Assert.That(original[4].r, Is.GreaterThan(1));
                Assert.That(pulled[2].r, Is.GreaterThan(original[2].r * 3), "The ridge should move back towards the bow.");
                Assert.That(shaped[1].r, Is.LessThan(.08f), "The raised wave should blend down at the bow tip.");
                Assert.That(shaped[2].r, Is.GreaterThan(shaped[1].r + .2f), "Water should rise smoothly out from the tip.");
                Assert.That(shaped[3].r, Is.GreaterThan(shaped[2].r));
                Assert.That(shaped[4].r, Is.LessThan(original[4].r * .2f));
                Assert.That(shaped[3].r, Is.EqualTo(pulled[3].r).Within(.06f), "Tip taper should preserve the rise outside its blend radius.");
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(host);
                Object.DestroyImmediate(texture);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(input);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
            yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator CruisingShipKeepsHullWavesWithoutAnAftWake()
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
            Assert.IsNotNull(Camera.main.GetComponent<OceanUnderwaterView>(), "The active world camera must receive underwater rendering.");
            Assert.IsNotNull(emitter.HullTexture, "Hull waves must remain assigned.");
            Assert.IsNull(emitter.WakeTexture, "The scene ship must not emit aft wake stamps.");
            Assert.That(emitter.TrailCount, Is.Zero, "Cruising must not deposit an aft wake.");
            Assert.IsTrue(ship.GetComponent<OceanHullSpray>().enabled, "Hull spray must remain enabled.");
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
                System.IO.File.WriteAllBytes(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "motu-ship-no-aft-wake-preview.png"), snapshot.EncodeToPNG());
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
            // Exercise the authored texture channels without the optional bow reshaping.
            ship.BowWavePullbackMetres = 0;
            ship.BowWaveTipBlendMetres = 0;
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
                // Generated maps can place a bow rise inside the broad clamp band.
                texture.SetPixels(new[] { new Color(0,0,0,0), new Color(128f/255,128f/255,128f/255,0), new Color(.4f,.4f,.4f,1) });
                texture.Apply();
                ship.BowWaveInAlpha = true;
                var independentBow = Read();
                Assert.That(independentBow[2].r, Is.InRange(9.5f,9.65f), "Alpha bow rise survives overlapping pull-down data while respecting its ceiling.");
                Assert.That(independentBow[2].g, Is.GreaterThan(.1), "The overlapping hull mask is still present.");
                Assert.That(independentBow[1].r, Is.EqualTo(9).Within(.01), "Zero alpha must not raise the rest of the footprint.");
                texture.SetPixels(new[] { Color.black, new Color(128f/255,128f/255,128f/255), Color.white });
                texture.Apply();
                ship.BowWaveInAlpha = false;
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
