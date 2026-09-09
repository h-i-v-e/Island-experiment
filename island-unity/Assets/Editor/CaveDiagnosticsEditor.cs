using Motu.Streaming;
using Motu.Gameplay;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(CaveStreamer))]
    internal sealed class CaveDiagnosticsEditor : UnityEditor.Editor
    {
        [MenuItem("Motu/Caves/Visit first available entrance")]
        private static void VisitFirstEntrance()
        {
            if (!Application.isPlaying) { Debug.Log("Enter Play Mode to visit a generated cave."); return; }
            foreach (var caves in Object.FindObjectsByType<CaveStreamer>())
            {
                if (caves.CaveCount == 0) continue;
                Selection.activeGameObject = caves.gameObject;
                Visit(caves, 0, false);
                return;
            }
            Debug.Log("No caves on the currently loaded islands. Cave settings apply when an island is generated; unsuitable islands may have none.");
        }

        public override void OnInspectorGUI()
        {
            var caves = (CaveStreamer)target;
            EditorGUILayout.HelpBox(caves.Diagnostics, MessageType.Info);
            EditorGUILayout.LabelField("Select this object to show entrance and chamber markers.", EditorStyles.wordWrappedLabel);
            using (new EditorGUI.DisabledScope(!Application.isPlaying))
                for (var i = 0; i < caves.CaveCount; i++)
                {
                    EditorGUILayout.LabelField($"Cave {i + 1}", EditorStyles.boldLabel);
                    if (GUILayout.Button("Walk from entrance (debug)")) Visit(caves, i, false);
                    if (GUILayout.Button("Visit chamber (debug)")) Visit(caves, i, true);
                }
        }
        private static void Visit(CaveStreamer caves, int index, bool chamber)
        {
            var demo = Object.FindAnyObjectByType<IslandDemoController>();
            if (demo == null || !demo.VisitCave(caves, index, chamber))
                Debug.LogWarning("Cave traversal requires an installed cave and the world first-person controller.", caves);
        }
    }
}
