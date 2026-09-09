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
            EditorGUILayout.LabelField("Cave resources", EditorStyles.boldLabel);
            EditorGUILayout.LabelField("Meshes / rendered triangles", $"{caves.MeshCount:N0} / {caves.RenderTriangles:N0}");
            EditorGUILayout.LabelField("Collider triangles", caves.ColliderTriangles.ToString("N0"));
            EditorGUILayout.LabelField("Prepared array payload", $"{caves.PreparedBufferBytes / 1048576.0:0.00} MiB");
            EditorGUILayout.LabelField("Native mesh export", $"{caves.ExportMilliseconds:0.00} ms");
            EditorGUILayout.LabelField("Managed copy and validation", $"{caves.CopyMilliseconds:0.00} ms");
            EditorGUILayout.LabelField("Unity mesh creation", $"{caves.MeshCreationMilliseconds:0.00} ms");
            EditorGUILayout.LabelField("Collider creation / cooking", $"{caves.ColliderCookMilliseconds:0.00} ms");
            EditorGUILayout.LabelField("Longest collider creation", $"{caves.LongestColliderCookMilliseconds:0.00} ms");
            EditorGUILayout.HelpBox("Installation timings exclude frame-budget waits. Array payload excludes object overhead, GPU memory and physics allocations.", MessageType.None);
            EditorGUILayout.LabelField("Select this object to show entrance and passage markers.", EditorStyles.wordWrappedLabel);
            using (new EditorGUI.DisabledScope(!Application.isPlaying))
                for (var i = 0; i < caves.CaveCount; i++)
                {
                    EditorGUILayout.LabelField($"Cave {i + 1}", EditorStyles.boldLabel);
                    if (GUILayout.Button("Walk from entrance (debug)")) Visit(caves, i, false);
                    if (GUILayout.Button("Visit main passage end (debug)")) Visit(caves, i, true);
                    for (var branch = 0; branch < caves.GetBranchCount(i); branch++)
                        if (GUILayout.Button($"Visit branch {branch + 1} end (debug)")) Visit(caves, i, true, branch);
                }
        }
        private static void Visit(CaveStreamer caves, int index, bool chamber, int branch = -1)
        {
            var demo = Object.FindAnyObjectByType<IslandDemoController>();
            if (demo == null || !demo.VisitCave(caves, index, chamber, branch))
                Debug.LogWarning("Cave traversal requires an installed cave and the world first-person controller.", caves);
        }
    }
}
