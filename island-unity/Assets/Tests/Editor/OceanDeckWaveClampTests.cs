using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace Motu.Editor
{
    public sealed class OceanDeckWaveClampTests
    {
        [Test]
        public void GpuCapsCrestsAtSeaPlaneWithoutHidingSubmergedDeckWater()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var host = new GameObject("Moving deck capsule");
            var other = new GameObject("Overlapping capsule");
            var shader = Shader.Find("Hidden/Motu/Tests/Deck Wave Clamp");
            Assert.IsNotNull(shader);
            var material = new Material(shader);
            var input = new Texture2D(5, 1, TextureFormat.RGBAFloat, false, true);
            input.filterMode = FilterMode.Point;
            var target = new RenderTexture(5, 1, 0, RenderTextureFormat.ARGBFloat);
            var readback = new Texture2D(5, 1, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            try
            {
                // Imported-model scale and rotated bow must not scale the radius.
                host.transform.SetPositionAndRotation(new Vector3(100, -20, 200), Quaternion.Euler(0, 90, 0));
                host.transform.localScale = Vector3.one * 1000;
                var capsule = host.AddComponent<OceanDeckWaveClamp>();
                capsule.Configure(Vector3.back * .01f, Vector3.forward * .01f, 4, 2);
                var overlap = other.AddComponent<OceanDeckWaveClamp>();
                overlap.Configure(new Vector3(90, 0, 200), new Vector3(110, 0, 200), 4, 2);
                // XY: world XZ; Z: signed wave height; W: undisplaced sea height.
                input.SetPixels(new[] {
                    new Color(100, 200, 3, 7), new Color(100, 200, -2, 7),
                    new Color(100, 205, 3, 7), new Color(100, 207, 3, 7),
                    new Color(112, 200, 3, 7) });
                input.Apply();
                Assert.That(input.GetPixel(0, 0).r, Is.EqualTo(100f));
                Color[] Read()
                {
                    OceanDeckWaveClamp.BindGlobals();
                    Graphics.Blit(input, target, material);
                    RenderTexture.active = target;
                    readback.ReadPixels(new Rect(0, 0, 5, 1), 0, 0);
                    readback.Apply();
                    return readback.GetPixels();
                }
                var pixels = Read();
                Assert.That(pixels[0].r, Is.EqualTo(7).Within(.001), "Crest stops at sea plane, even when ship is below it.");
                Assert.That(pixels[0].a, Is.EqualTo(1), "Water is retained, never clipped out above submerged decks.");
                Assert.That(pixels[1].r, Is.EqualTo(5).Within(.001), "Troughs are unchanged.");
                Assert.That(pixels[2].r, Is.EqualTo(8.5f).Within(.001), "Smooth exterior blend; overlaps do not double the clamp.");
                Assert.That(pixels[3].r, Is.EqualTo(10).Within(.001), "Waves outside remain unchanged.");
                Assert.That(pixels[4].r, Is.EqualTo(7).Within(.001), "Capsule has rounded ends.");
                overlap.enabled = false;
                host.transform.position += Vector3.right * 100;
                Assert.That(Read()[0].r, Is.EqualTo(10).Within(.001), "Old footprint clears when ship moves.");
                host.transform.position -= Vector3.right * 100;
                Assert.That(Read()[0].r, Is.EqualTo(7).Within(.001));
                capsule.enabled = false;
                Assert.That(Read()[0].r, Is.EqualTo(10).Within(.001), "Disabling last capsule restores all waves.");
                Assert.That(Shader.GetGlobalInt("_MotuDeckCapsuleCount"), Is.Zero);
                Assert.IsFalse(ShaderUtil.ShaderHasError(Shader.Find("Motu/Sea Water")));
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(host);
                Object.DestroyImmediate(other);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(input);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(readback);
            }
        }

        [Test]
        public void OpenSeaShipHasAnActiveDeckCapsule()
        {
            EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            try
            {
                var ship = Object.FindFirstObjectByType<Motu.Gameplay.ShipController>();
                var capsule = ship.GetComponent<OceanDeckWaveClamp>();
                Assert.IsNotNull(capsule);
                Assert.IsTrue(capsule.isActiveAndEnabled);
            }
            finally { EditorSceneManager.NewScene(NewSceneSetup.EmptyScene); }
        }
    }
}
