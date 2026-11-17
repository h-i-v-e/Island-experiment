using Motu.Towns;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(TownRecipeCatalogue))]
    public sealed class TownRecipeCatalogueEditor : UnityEditor.Editor
    {
        private int previewSeed = 42;
        public override void OnInspectorGUI()
        {
            EditorGUILayout.HelpBox("Each coastal section uses one deterministic prototype per recipe. Centre entries occupy the town centre; Other entries fill nearby dry sites. Native slope, river and foundation limits can reduce requested counts.", MessageType.Info);
            DrawDefaultInspector();
            previewSeed = EditorGUILayout.IntField("Prefab Preview Seed", previewSeed);
            if (GUILayout.Button("Bake Preview Building Prefabs"))
            {
                try
                {
                    var paths = TownPrefabBaker.Bake((TownRecipeCatalogue)target, previewSeed);
                    Selection.activeObject = AssetDatabase.LoadAssetAtPath<GameObject>(paths[0]);
                    EditorGUIUtility.PingObject(Selection.activeObject);
                    Debug.Log($"Baked {paths.Length} town building prefabs under {System.IO.Path.GetDirectoryName(paths[0])}.");
                }
                catch (System.Exception error) { Debug.LogException(error); }
            }
            if (GUILayout.Button("Check Catalogue References"))
            {
                try
                {
                    ((TownRecipeCatalogue)target).CaptureJson();
                    EditorUtility.DisplayDialog("Town catalogue", "Recipe references, IDs and count ranges are valid.", "OK");
                }
                catch (System.ArgumentException error)
                {
                    EditorUtility.DisplayDialog("Town catalogue", error.Message, "OK");
                }
            }
        }
    }
}
