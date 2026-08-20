using UnityEditor;
using UnityEngine;

[CustomEditor(typeof(IslandGenerator))]
public sealed class IslandGeneratorEditor : Editor
{
    public override void OnInspectorGUI()
    {
        DrawDefaultInspector();
        var island = (IslandGenerator)target;
        EditorGUILayout.Space();
        EditorGUILayout.HelpBox(
            "Tree and plant prefab libraries are reserved for the upcoming vegetation placement phase. Assigned assets are preserved but are not spawned yet.",
            MessageType.Info);
        using (new EditorGUI.DisabledScope(!Application.isPlaying || island.IsGenerating))
        {
            if (GUILayout.Button("Generate / Regenerate")) island.Generate();
            if (GUILayout.Button("Clear Generated Island")) island.Clear();
        }
        EditorGUILayout.LabelField("Runtime Status", island.Status);
    }
}
