using System;
using Motu.Rendering;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace Motu.Editor
{
    public static class ShipDeckWaveClampSetup
    {
        [MenuItem("Island/Set Up Selected Ship Texture Waves")]
        public static void ConfigureSelectedTextures()
        {
            ShipWaveTextureWindow.Open();
        }

        internal static OceanDeckWaveClamp ApplyTextures(GameObject ship, Texture2D hull, Texture2D wake)
        {
            if (EditorApplication.isPlaying) throw new InvalidOperationException("Set up ship textures outside Play Mode.");
            if (ship == null) throw new InvalidOperationException("Choose a ship first.");
            if (EditorUtility.IsPersistent(ship)) throw new InvalidOperationException("Open the ship prefab in Prefab Mode to edit its textures.");
            // Resolve both paths before reimporting: they may refer to the same asset.
            var hullPath = AssetDatabase.GetAssetPath(hull);
            var wakePath = AssetDatabase.GetAssetPath(wake);
            if (!string.IsNullOrEmpty(hullPath) && AssetImporter.GetAtPath(hullPath) is TextureImporter) hull = LoadLinearTexture(hullPath);
            if (!string.IsNullOrEmpty(wakePath) && AssetImporter.GetAtPath(wakePath) is TextureImporter)
                wake = wakePath == hullPath ? hull : LoadLinearTexture(wakePath);
            var component = ship.GetComponent<OceanDeckWaveClamp>() ?? Undo.AddComponent<OceanDeckWaveClamp>(ship);
            Undo.RecordObject(component, "Set ship wave textures");
            component.ConfigureTextures(hull, wake);
            EditorUtility.SetDirty(component);
            PrefabUtility.RecordPrefabInstancePropertyModifications(component);
            EditorSceneManager.MarkSceneDirty(ship.scene);
            return component;
        }

        internal static Texture2D LoadLinearTexture(string path)
        {
            var importer = AssetImporter.GetAtPath(path) as TextureImporter;
            if (importer == null) throw new InvalidOperationException("Missing ship wave texture: " + path);
            importer.textureType = TextureImporterType.Default;
            importer.textureShape = TextureImporterShape.Texture2D;
            importer.npotScale = TextureImporterNPOTScale.None;
            importer.sRGBTexture = false;
            importer.mipmapEnabled = false;
            importer.wrapMode = TextureWrapMode.Clamp;
            importer.textureCompression = TextureImporterCompression.Uncompressed;
            importer.SaveAndReimport();
            var texture = AssetDatabase.LoadAssetAtPath<Texture2D>(path);
            if (texture == null) throw new InvalidOperationException("Ship wave texture did not import as Texture2D: " + path);
            return texture;
        }

        [MenuItem("Island/Set Up Selected Ship Deck Wave Clamp")]
        public static void ConfigureSelectedShip()
        {
            if (EditorApplication.isPlaying) throw new InvalidOperationException("Set up the deck capsule outside Play Mode.");
            var body = Selection.activeGameObject != null ? Selection.activeGameObject.GetComponentInParent<Rigidbody>() : null;
            if (body == null) throw new InvalidOperationException("Select the ship's Rigidbody first.");
            Configure(body);
        }

        internal static OceanDeckWaveClamp Configure(Rigidbody body)
        {
            var capsule = body.GetComponent<OceanDeckWaveClamp>() ?? Undo.AddComponent<OceanDeckWaveClamp>(body.gameObject);
            Undo.RecordObject(capsule, "Fit deck wave capsule");
            // This pirate-ship import points along local +X. Cover the deck with
            // a small vertex margin, excluding the long bowsprit from the fit.
            var forward = body.transform.TransformDirection(Vector3.right).normalized;
            var centre = body.transform.position - forward * 2;
            capsule.Configure(body.transform.InverseTransformPoint(centre - forward * 10),
                body.transform.InverseTransformPoint(centre + forward * 10), 4.5f, 3f);
            PrefabUtility.RecordPrefabInstancePropertyModifications(capsule);
            EditorSceneManager.MarkSceneDirty(body.gameObject.scene);
            Selection.activeGameObject = body.gameObject;
            return capsule;
        }
    }
}
