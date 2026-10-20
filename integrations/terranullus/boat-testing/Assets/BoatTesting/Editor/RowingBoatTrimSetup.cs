using System;
using System.IO;
using Motu.Gameplay;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class RowingBoatTrimSetup
{
    [MenuItem("Island/Boat Tests/Align Buoyancy To Corrected Rowboat Rotation")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var scene = SceneManager.GetSceneByPath(BoatTestSceneSetup.RowingScene);
        FloatingShip instance = null;
        foreach (var root in scene.GetRootGameObjects())
            foreach (var candidate in root.GetComponentsInChildren<FloatingShip>(true))
                if (PrefabUtility.GetPrefabAssetPathOfNearestInstanceRoot(candidate.gameObject) == BoatTestSceneSetup.BoatPrefab)
                    instance = candidate;
        if (instance == null) throw new InvalidOperationException("Open the corrected RowingBoatTest scene first.");
        var rotation = instance.transform.rotation;
        float draft = instance.GetComponentInChildren<OceanBuoyancy>().Draft;
        Undo.RegisterFullObjectHierarchyUndo(instance.gameObject, "Calibrate rowboat trim and buoyancy");
        var prefab = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
        try
        {
            // Carry the user's actual scene rotation and current draft into the
            // reusable boat, without applying unrelated scene/prefab overrides.
            prefab.GetComponentInChildren<OceanBuoyancy>().Draft = draft;
            RowingBoatDraftSetup.ConfigureReferencePose(prefab, rotation);
            PrefabUtility.SaveAsPrefabAsset(prefab, BoatTestSceneSetup.BoatPrefab);
        }
        finally { PrefabUtility.UnloadPrefabContents(prefab); }
        float waterline = RowingBoatDraftSetup.ConfigureReferencePose(instance.gameObject, rotation);
        foreach (var component in new UnityEngine.Object[] { instance, instance.transform,
            instance.GetComponentInChildren<OceanBuoyancy>(), instance.GetComponentInChildren<Rigidbody>(),
            instance.GetComponentInChildren<HullDraftLoading>() })
            PrefabUtility.RecordPrefabInstancePropertyModifications(component);
        EditorSceneManager.MarkSceneDirty(scene);
        var body = instance.GetComponentInChildren<Rigidbody>();
        string report = $"Corrected rowboat reference: {rotation.eulerAngles:F4}; target draft {draft:F3} m; reference waterline {waterline:F4} m; loading mass {body.mass:F1} kg; centre of mass {body.centerOfMass:F4}.\nScene remains unsaved; unrelated overrides preserved.\n";
        File.WriteAllText("/tmp/rowing-boat-trim-setup.txt", report);
        Debug.Log(report);
    }
}
