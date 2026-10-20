using UnityEditor;
using UnityEngine;

[CustomEditor(typeof(BoatSeat))]
public sealed class BoatSeatEditor : Editor
{
    public override void OnInspectorGUI()
    {
        var seat = (BoatSeat)target;
        serializedObject.Update();
        var oldProfile = serializedObject.FindProperty("profile").objectReferenceValue;
        int oldEntry = serializedObject.FindProperty("entry").enumValueIndex;
        float oldClearance = serializedObject.FindProperty("approachClearance").floatValue;
        DrawDefaultInspector();
        serializedObject.Update();
        bool regenerate = oldProfile != serializedObject.FindProperty("profile").objectReferenceValue
            || oldEntry != serializedObject.FindProperty("entry").enumValueIndex
            || oldClearance != serializedObject.FindProperty("approachClearance").floatValue;
        EditorGUILayout.HelpBox("Place this marker at the seated hips and rotate its blue arrow to set facing. Green is the left approach; red is the right. Move approach children to adjust clearance.", MessageType.Info);
        if (seat.Role == BoatSeat.SeatRole.Rower)
            EditorGUILayout.HelpBox("Each hand follows the Hand Grip child on its assigned oar; its rotation describes the wrist bone. The Rowlock pin child defines the shaft contact and the rig transform is its fixed target on the boat. Increase the rig's Blade Dip Degrees to lower the blade. Pelvis and hand IK weights blend in only after boarding.", MessageType.Info);
        if (GUILayout.Button("Generate Left And Right Approach Points") || regenerate)
        {
            Undo.RegisterFullObjectHierarchyUndo(seat.gameObject, "Generate boat approach points");
            seat.GenerateApproachPoints();
            EditorUtility.SetDirty(seat);
            foreach (var item in new Object[] { seat, seat.LeftApproach, seat.RightApproach })
                if (item != null && PrefabUtility.IsPartOfPrefabInstance(item)) PrefabUtility.RecordPrefabInstancePropertyModifications(item);
        }
    }

    private void OnSceneGUI()
    {
        var seat = (BoatSeat)target;
        Handles.Label(seat.transform.position + Vector3.up * .15f, seat.name + " (" + seat.Role + ")");
        if (seat.LeftApproach != null) Handles.Label(seat.LeftApproach.position + Vector3.up * .15f, "Left approach");
        if (seat.RightApproach != null) Handles.Label(seat.RightApproach.position + Vector3.up * .15f, "Right approach");
        foreach (var oar in new[] { seat.LeftHandOar, seat.RightHandOar })
            if (oar != null && oar.HandGrip != null)
            {
                Handles.color = Color.yellow;
                Handles.DrawLine(seat.transform.position, oar.HandGrip.position);
                Handles.Label(oar.HandGrip.position + Vector3.up * .1f, oar == seat.LeftHandOar ? "Left hand grip" : "Right hand grip");
            }
    }
}
