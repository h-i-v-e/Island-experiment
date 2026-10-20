using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class RowboatControlSetup
{
    [MenuItem("Island/Boat Tests/Set Up Rowboat Controls And Rowlock Pins")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var prefab = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
        try { Configure(prefab); PrefabUtility.SaveAsPrefabAsset(prefab, BoatTestSceneSetup.BoatPrefab); }
        finally { PrefabUtility.UnloadPrefabContents(prefab); }
        for (int i = 0; i < SceneManager.sceneCount; i++)
        {
            var scene = SceneManager.GetSceneAt(i);
            if (!scene.isLoaded || EditorSceneManager.IsPreviewScene(scene)) continue;
            foreach (var root in scene.GetRootGameObjects())
                foreach (var floating in root.GetComponentsInChildren<FloatingShip>(true))
                    if (PrefabUtility.GetPrefabAssetPathOfNearestInstanceRoot(floating.gameObject) == BoatTestSceneSetup.BoatPrefab)
                    {
                        Undo.RegisterFullObjectHierarchyUndo(floating.gameObject, "Add rowing controls and pinned oars");
                        Configure(floating.gameObject);
                        EditorSceneManager.MarkSceneDirty(scene);
                    }
        }
        const string report = "Rowboat Controller installed: Passenger aft commands the two NPC rowers; forward thrust and left/right yaw use the floating Rigidbody. Four shaft pins fixed to rowlocks. Authored seats, grips, pivots and oar poses preserved. Open scenes left unsaved.\n";
        File.WriteAllText("/tmp/rowboat-controls-setup.txt", report);
        Debug.Log(report);
    }

    static void Configure(GameObject root)
    {
        var seats = root.GetComponentsInChildren<BoatSeat>(true);
        var captain = seats.Single(s => s.name == "Passenger aft" && s.Role == BoatSeat.SeatRole.Passenger);
        var rowers = seats.Where(s => s.Role == BoatSeat.SeatRole.Rower).ToArray();
        if (rowers.Length != 2) throw new InvalidOperationException("Expected two rower seats.");
        var controls = root.GetComponent<RowboatController>();
        if (controls == null) controls = root.AddComponent<RowboatController>();
        controls.Configure(captain, rowers);
        if (PrefabUtility.IsPartOfPrefabInstance(controls)) PrefabUtility.RecordPrefabInstancePropertyModifications(controls);
        foreach (var rig in root.GetComponentsInChildren<BoatOar>(true)) BoatSeatIKSetup.EnsureRowlockPin(rig);
    }
}
