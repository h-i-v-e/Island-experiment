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
                var hullB = Texture("Hull B", Color.gray);
                var a = ShipDeckWaveClampSetup.ApplyHullTexture(first, hullA);
                var b = ShipDeckWaveClampSetup.ApplyHullTexture(second, hullB);
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
                Assert.AreEqual(hullB, b.HullTexture);
                a.GetFootprint(out var centre, out var forward);
                // Switching the picker target loads that ship's texture and performs no mutation.
                window = ScriptableObject.CreateInstance<ShipWaveTextureWindow>();
                window.LoadShip(instanceA);
                var picker = new SerializedObject(window);
                Assert.AreEqual(hullA, picker.FindProperty("hullTexture").objectReferenceValue);
                window.LoadShip(instanceB);
                picker.Update();
                Assert.AreEqual(hullB, picker.FindProperty("hullTexture").objectReferenceValue);
                ShipDeckWaveClampSetup.ApplyHullTexture(instanceA, hullB);
                Assert.AreEqual(hullB, a.HullTexture);
                Assert.AreEqual(hullB, b.HullTexture);
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
        public void GpuComposesDifferentHullTexturesForTwoShips()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var first = new GameObject("Black hull");
            var second = new GameObject("Neutral hull");
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
                a.ConfigureHullTexture(Texture2D.blackTexture);
                b.ConfigureHullTexture(neutral);
                OceanShipWaveField.Render(new[] { a, b }, Vector3.zero);
                input.SetPixels(new[] { new Color(0,0,2,7), new Color(50,0,2,7), new Color(0,-30,2,7), new Color(50,-30,2,7) });
                input.Apply();
                Graphics.Blit(input, target, material);
                RenderTexture.active = target;
                output.ReadPixels(new Rect(0,0,4,1),0,0); output.Apply();
                var pixels = output.GetPixels();
                Assert.That(pixels[0].r, Is.EqualTo(7).Within(.01), "First ship's black hull clamps the crest.");
                Assert.That(pixels[1].r, Is.EqualTo(9).Within(.01), "Second ship's neutral hull leaves the same crest alone.");
                Assert.That(pixels[2].r, Is.EqualTo(9).Within(.01), "Water behind the first ship remains unchanged.");
                Assert.That(pixels[3].r, Is.EqualTo(9).Within(.01), "Water behind the second ship remains unchanged.");
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
