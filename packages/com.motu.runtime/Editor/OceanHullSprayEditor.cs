using Motu.Rendering;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(OceanHullSpray))]
    public sealed class OceanHullSprayEditor : UnityEditor.Editor
    {
        private bool showHeights;
        public override void OnInspectorGUI()
        {
            serializedObject.Update();
            DrawPropertiesExcluding(serializedObject, "m_Script", "gunwaleHeights");
            var points = serializedObject.FindProperty("waterline");
            var heights = serializedObject.FindProperty("gunwaleHeights");
            if (points.arraySize > 0)
            {
                if (heights.arraySize != points.arraySize)
                {
                    EditorGUILayout.HelpBox("Individual gunwale heights are not configured for this waterline. Default Gunwale Height is used until you create the profile.", MessageType.Info);
                    if (GUILayout.Button("Create individual gunwale heights")) { InitializeHeights(points, heights); showHeights = true; }
                }
                else
                {
                    showHeights = EditorGUILayout.Foldout(showHeights, "Individual gunwale heights (" + points.arraySize + ")", true);
                    if (showHeights)
                    {
                        EditorGUI.indentLevel++;
                        for (int i = 0; i < points.arraySize; i++)
                        {
                            var point = points.GetArrayElementAtIndex(i).vector3Value;
                            var height = heights.GetArrayElementAtIndex(i);
                            height.floatValue = Mathf.Max(0, EditorGUILayout.FloatField(
                                new GUIContent("Point " + i + "  (x " + point.x.ToString("0.0") + ", z " + point.z.ToString("0.0") + ")",
                                    "Rise above this waterline point along the ship's hull-up axis."), height.floatValue));
                        }
                        EditorGUI.indentLevel--;
                    }
                }
            }
            serializedObject.ApplyModifiedProperties();
            EditorGUILayout.HelpBox("Orange numbered handles mark the individual gunwale switch heights. Drag each handle along hull-up to match the local rail, or edit its value above. Heights interpolate between neighbouring points, including across the closing edge. Blue handles edit the resting waterline. Particles pass through the hull.", MessageType.Info);
        }

        private void InitializeHeights(SerializedProperty points, SerializedProperty heights)
        {
            float fallback = Mathf.Max(0, serializedObject.FindProperty("gunwaleHeight").floatValue);
            heights.arraySize = points.arraySize;
            for (int i = 0; i < heights.arraySize; i++) heights.GetArrayElementAtIndex(i).floatValue = fallback;
        }

        private void OnSceneGUI()
        {
            serializedObject.Update();
            var points = serializedObject.FindProperty("waterline");
            var heights = serializedObject.FindProperty("gunwaleHeights");
            var root = ((OceanHullSpray)target).transform;
            var up = serializedObject.FindProperty("waterlineUp").vector3Value.normalized;
            bool overflow = serializedObject.FindProperty("gunwaleOverflow").boolValue;
            float fallback = serializedObject.FindProperty("gunwaleHeight").floatValue;
            for (var i = 0; i < points.arraySize; i++)
            {
                var point = points.GetArrayElementAtIndex(i);
                var world = root.TransformPoint(point.vector3Value);
                Handles.color = Color.cyan;
                EditorGUI.BeginChangeCheck();
                var moved = Handles.FreeMoveHandle(world, HandleUtility.GetHandleSize(world) * .035f, Vector3.zero, Handles.DotHandleCap);
                if (EditorGUI.EndChangeCheck()) point.vector3Value = root.InverseTransformPoint(moved);
                if (!overflow) continue;
                float height = heights.arraySize == points.arraySize ? heights.GetArrayElementAtIndex(i).floatValue : fallback;
                var rail = root.TransformPoint(point.vector3Value + up * Mathf.Max(0, height));
                Handles.color = new Color(1, .65f, .15f);
                Handles.Label(rail, i.ToString());
                EditorGUI.BeginChangeCheck();
                var raised = Handles.Slider(rail, root.TransformVector(up).normalized,
                    HandleUtility.GetHandleSize(rail) * .055f, Handles.ConeHandleCap, 0);
                if (!EditorGUI.EndChangeCheck()) continue;
                if (heights.arraySize != points.arraySize) InitializeHeights(points, heights);
                heights.GetArrayElementAtIndex(i).floatValue = Mathf.Max(0,
                    Vector3.Dot(root.InverseTransformPoint(raised) - point.vector3Value, up));
            }
            serializedObject.ApplyModifiedProperties();
        }
    }
}
