using Motu.Islands;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(SingleIsland))]
    public sealed class SingleIslandEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            DrawDefaultInspector();
            var island = (SingleIsland)target;
            EditorGUILayout.HelpBox("Assign your streaming target. This component uses your scene lighting and does not create cameras or an ocean. Generate in Play mode.", MessageType.Info);
            using (new EditorGUI.DisabledScope(!Application.isPlaying || island.Generator.IsGenerating))
                if (GUILayout.Button("Generate Island")) _ = island.GenerateAsync();
            if (Application.isPlaying && GUILayout.Button("Clear Island")) island.Clear();
            EditorGUILayout.LabelField("Status", island.Generator.Status);
        }
    }
}
