using Motu.Gameplay;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    [CustomEditor(typeof(OceanBuoyancy))]
    public sealed class OceanBuoyancyEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            DrawDefaultInspector();
            EditorGUILayout.HelpBox("Place probes around the lower hull. Draft is their equilibrium depth below the waves in world metres. The Rigidbody needs gravity, free Y/rotation, and a centre of mass above the probe plane but low in the hull. Add hull colliders for land collisions.", MessageType.Info);
            var buoyancy = (OceanBuoyancy)target;
            if (buoyancy.GetComponentInChildren<Collider>() == null)
            {
                EditorGUILayout.HelpBox("This body has no hull collider. Add one to give the Rigidbody a useful mass distribution and collisions with land. The box below is an initial approximation; resize it to the hull.", MessageType.Warning);
                if (GUILayout.Button("Add Approximate Hull Box Collider")) AddHullCollider(buoyancy);
            }
            if (GUILayout.Button("Fit Probes To Hull Bounds"))
            {
                Undo.RecordObject(target, "Fit buoyancy probes");
                ((OceanBuoyancy)target).FitProbesToHullBounds();
                EditorUtility.SetDirty(target);
                PrefabUtility.RecordPrefabInstancePropertyModifications(target);
            }
        }

        private void AddHullCollider(OceanBuoyancy buoyancy)
        {
            serializedObject.Update();
            var probes = serializedObject.FindProperty("probes");
            if (probes.arraySize == 0) return;
            var bounds = new Bounds(buoyancy.transform.TransformPoint(probes.GetArrayElementAtIndex(0).vector3Value), Vector3.zero);
            for (var i = 1; i < probes.arraySize; i++)
                bounds.Encapsulate(buoyancy.transform.TransformPoint(probes.GetArrayElementAtIndex(i).vector3Value));
            bounds.center += Vector3.up * buoyancy.Draft * .5f;
            bounds.size = new Vector3(Mathf.Max(bounds.size.x / .65f, .01f), buoyancy.Draft * 2f,
                Mathf.Max(bounds.size.z / .65f, .01f));
            var localBounds = new Bounds(buoyancy.transform.InverseTransformPoint(bounds.center), Vector3.zero);
            for (var i = 0; i < 8; i++)
            {
                var corner = bounds.center + Vector3.Scale(bounds.extents,
                    new Vector3((i & 1) == 0 ? -1 : 1, (i & 2) == 0 ? -1 : 1, (i & 4) == 0 ? -1 : 1));
                localBounds.Encapsulate(buoyancy.transform.InverseTransformPoint(corner));
            }
            var collider = Undo.AddComponent<BoxCollider>(buoyancy.gameObject);
            collider.center = localBounds.center;
            collider.size = localBounds.size;
            PrefabUtility.RecordPrefabInstancePropertyModifications(collider);
        }

        private void OnSceneGUI()
        {
            serializedObject.Update();
            var probes = serializedObject.FindProperty("probes");
            var buoyancy = (OceanBuoyancy)target;
            for (var i = 0; i < probes.arraySize; i++)
            {
                var probe = probes.GetArrayElementAtIndex(i);
                var world = buoyancy.transform.TransformPoint(probe.vector3Value);
                Handles.Label(world, $"Probe {i + 1}");
                EditorGUI.BeginChangeCheck();
                var moved = Handles.PositionHandle(world, Quaternion.identity);
                if (EditorGUI.EndChangeCheck()) probe.vector3Value = buoyancy.transform.InverseTransformPoint(moved);
            }
            serializedObject.ApplyModifiedProperties();
        }
    }
}
