using Motu.Rendering;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(OceanHullSpray))]
    public sealed class OceanHullSprayEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            DrawDefaultInspector();
            EditorGUILayout.HelpBox("The closed loop follows the authored resting waterline. Use Island > Waterline Texture Generator to regenerate it after changing the hull or sample spacing. Particles pass through the hull.", MessageType.Info);
        }

        private void OnSceneGUI()
        {
            serializedObject.Update();
            var points = serializedObject.FindProperty("waterline");
            var root = ((OceanHullSpray)target).transform;
            for (var i = 0; i < points.arraySize; i++)
            {
                var point = points.GetArrayElementAtIndex(i);
                var world = root.TransformPoint(point.vector3Value);
                EditorGUI.BeginChangeCheck();
                var moved = Handles.FreeMoveHandle(world, HandleUtility.GetHandleSize(world) * .035f, Vector3.zero, Handles.DotHandleCap);
                if (EditorGUI.EndChangeCheck()) point.vector3Value = root.InverseTransformPoint(moved);
            }
            serializedObject.ApplyModifiedProperties();
        }
    }
}
