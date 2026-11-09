using Motu.World;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(TerrainFoundation))]
    public sealed class TerrainFoundationEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            DrawDefaultInspector();
            EditorGUILayout.Space();
            var foundation = (TerrainFoundation)target;
            EditorGUILayout.HelpBox("Add to a building or a group root. The foundation includes active child mesh bounds, "
                + "rounds dimensions up to whole texture repeats, sets its top to the highest terrain sample "
                + "and puts extra depth underground. "
                + "The bundled stonework was baked with the Rust Material Studio generator.", MessageType.Info);
            EditorGUILayout.LabelField("Build status", foundation.Status.ToString());
            if (foundation.IsBuilt)
                EditorGUILayout.LabelField("Terrain range", $"{foundation.GroundSample.MinimumHeight:F2} to {foundation.GroundSample.MaximumHeight:F2} m");
            using (new EditorGUI.DisabledScope(!Application.isPlaying))
                if (GUILayout.Button("Build / Refit Foundation")) foundation.TryBuild();
        }
    }
}
