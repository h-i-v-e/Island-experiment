using Motu.Islands;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(IslandBirdPerches))]
    public sealed class IslandBirdPerchesEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            var island = (IslandBirdPerches)target;
            int trees = 0, cliffs = 0;
            foreach (var perch in island.Perches)
                if (perch.Kind == LandBirdPerchKind.Tree) trees++; else cliffs++;
            EditorGUILayout.LabelField("Generated land perches", island.Perches.Count.ToString());
            EditorGUILayout.LabelField("Treetops / cliff ledges", $"{trees} / {cliffs}");
            EditorGUILayout.LabelField("Cruise altitude", $"{island.CruiseHeight:F1} m");
            EditorGUILayout.HelpBox("Generated from the final native canopy and terrain, at most one per LOD0 tile. Selected gizmos show treetops in green and cliff ledges in cyan. Only occupied anchors create bird markers.", MessageType.Info);
        }
    }
}
