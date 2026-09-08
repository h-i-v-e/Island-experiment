using System;
using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class ShipWaveTextureSetupTests
    {
        [Test]
        public void DifferentPrefabTypesKeepTheirTexturesAndInstanceOverrides()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var folder = "Assets/__ShipTextureTests_" + Guid.NewGuid().ToString("N");
            AssetDatabase.CreateFolder("Assets", folder.Substring("Assets/".Length));
            var first = new GameObject("First ship type");
            var second = new GameObject("Second ship type");
            GameObject instanceA = null, instanceB = null;
            ShipWaveTextureWindow window = null;
            try
            {
                Texture2D Texture(string name, Color colour)
                {
                    var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false, true);
                    texture.SetPixels(new[] { colour, colour, colour, colour });
                    texture.Apply();
                    AssetDatabase.CreateAsset(texture, folder + "/" + name + ".asset");
                    return texture;
                }
                var hullA = Texture("Hull A", Color.black);
                var wakeA = Texture("Wake A", Color.white);
                var hullB = Texture("Hull B", Color.gray);
                var wakeB = Texture("Wake B", new Color(.75f,.75f,.75f));
                var a = ShipDeckWaveClampSetup.ApplyTextures(first, hullA, wakeA);
                var b = ShipDeckWaveClampSetup.ApplyTextures(second, hullB, wakeB);
                a.Configure(Vector3.left*4, Vector3.right*8, 2, 3);
                var prefabA = PrefabUtility.SaveAsPrefabAsset(first, folder + "/A.prefab");
                var prefabB = PrefabUtility.SaveAsPrefabAsset(second, folder + "/B.prefab");
                Object.DestroyImmediate(first);
                Object.DestroyImmediate(second);
                instanceA = (GameObject)PrefabUtility.InstantiatePrefab(prefabA);
                instanceB = (GameObject)PrefabUtility.InstantiatePrefab(prefabB);
                a = instanceA.GetComponent<OceanDeckWaveClamp>();
                b = instanceB.GetComponent<OceanDeckWaveClamp>();
                Assert.AreEqual(hullA, a.HullTexture);
                Assert.AreEqual(wakeA, a.WakeTexture);
                Assert.AreEqual(hullB, b.HullTexture);
                Assert.AreEqual(wakeB, b.WakeTexture);
                a.GetFootprint(out var centre, out var forward);
                // Switching the picker target loads that ship's pair and performs no mutation.
                window = ScriptableObject.CreateInstance<ShipWaveTextureWindow>();
                window.LoadShip(instanceA);
                var picker = new SerializedObject(window);
                Assert.AreEqual(hullA, picker.FindProperty("hullTexture").objectReferenceValue);
                window.LoadShip(instanceB);
                picker.Update();
                Assert.AreEqual(hullB, picker.FindProperty("hullTexture").objectReferenceValue);
                Assert.AreEqual(wakeB, picker.FindProperty("wakeTexture").objectReferenceValue);
                ShipDeckWaveClampSetup.ApplyTextures(instanceA, hullB, null);
                Assert.AreEqual(hullB, a.HullTexture);
                Assert.IsNull(a.WakeTexture);
                Assert.AreEqual(wakeB, b.WakeTexture);
                Assert.AreEqual(hullA, prefabA.GetComponent<OceanDeckWaveClamp>().HullTexture);
                a.GetFootprint(out var afterCentre, out var afterForward);
                Assert.AreEqual(centre, afterCentre);
                Assert.AreEqual(forward, afterForward);
                var modifications = PrefabUtility.GetPropertyModifications(instanceA);
                Assert.That(Array.Exists(modifications, modification => modification.propertyPath == "hullTexture"));
            }
            finally
            {
                if (window != null) Object.DestroyImmediate(window);
                if (first != null) Object.DestroyImmediate(first);
                if (second != null) Object.DestroyImmediate(second);
                if (instanceA != null) Object.DestroyImmediate(instanceA);
                if (instanceB != null) Object.DestroyImmediate(instanceB);
                AssetDatabase.DeleteAsset(folder);
            }
        }

        [Test]
        public void GpuComposesDifferentHullAndWakeTexturesForTwoShips()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var first = new GameObject("Black hull with white wake");
            var second = new GameObject("Neutral hull and wake");
            var neutral = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            neutral.SetPixel(0,0,new Color(128f/255,128f/255,128f/255)); neutral.Apply();
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Ship Wave Field"));
            var input = new Texture2D(4, 1, TextureFormat.RGBAFloat, false, true) { filterMode = FilterMode.Point };
            var target = new RenderTexture(4, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var output = new Texture2D(4, 1, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            try
            {
                second.transform.position = Vector3.right*50;
                var a = first.AddComponent<OceanDeckWaveClamp>();
                var b = second.AddComponent<OceanDeckWaveClamp>();
                a.ConfigureTextures(Texture2D.blackTexture, Texture2D.whiteTexture);
                b.ConfigureTextures(neutral, neutral);
                foreach (var ship in new[] { a, b })
                {
                    var position = ship.transform.position;
                    ship.AdvanceTrail(position + Vector3.back*33, Vector3.forward, 8, 0);
                    ship.AdvanceTrail(position + Vector3.back*30, Vector3.forward, 8, 1);
                }
                OceanShipWaveField.Render(new[] { a, b }, Vector3.zero, 1);
                input.SetPixels(new[] { new Color(0,0,2,7), new Color(50,0,2,7), new Color(0,-30,2,7), new Color(50,-30,2,7) });
                input.Apply();
                Graphics.Blit(input, target, material);
                RenderTexture.active = target;
                output.ReadPixels(new Rect(0,0,4,1),0,0); output.Apply();
                var pixels = output.GetPixels();
                Assert.That(pixels[0].r, Is.EqualTo(7).Within(.01), "First ship's black hull clamps the crest.");
                Assert.That(pixels[1].r, Is.EqualTo(9).Within(.01), "Second ship's neutral hull leaves the same crest alone.");
                Assert.That(pixels[2].r, Is.EqualTo(9.8).Within(.01), "First ship leaves its own white wake.");
                Assert.That(pixels[3].r, Is.EqualTo(9).Within(.01), "Second ship samples its own neutral wake texture.");
            }
            finally
            {
                RenderTexture.active = oldTarget;
                Object.DestroyImmediate(first); Object.DestroyImmediate(second);
                Object.DestroyImmediate(neutral); Object.DestroyImmediate(material);
                Object.DestroyImmediate(input); Object.DestroyImmediate(target); Object.DestroyImmediate(output);
            }
        }
    }
}
