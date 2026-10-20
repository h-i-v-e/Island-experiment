using System;
using System.IO;
using Motu.Gameplay;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class RowingBoatDraftSetup
{
    public const float DefaultDraft = .45f;

    public static float Configure(GameObject root)
    {
        var buoyancy = root.GetComponentInChildren<OceanBuoyancy>(true);
        if (buoyancy == null || buoyancy.HullGeometry == null) throw new InvalidOperationException("Rowboat needs its baked buoyancy hull.");
        buoyancy.Draft = DefaultDraft;
        return ConfigureReferencePose(root, root.transform.rotation);
    }

    // Preserve the model's authored trim; the geometry, collider and exclusion
    // volume already live in this same root/body coordinate frame.
    public static float ConfigureReferencePose(GameObject root, Quaternion rotation)
    {
        var buoyancy = root.GetComponentInChildren<OceanBuoyancy>(true);
        var body = buoyancy.GetComponent<Rigidbody>();
        var up = Quaternion.Inverse(rotation) * Vector3.up;
        var scale = body.transform.lossyScale;
        if (!HullDraftLoading.TryCalculateLoading(buoyancy.HullGeometry, scale, buoyancy.Draft,
            buoyancy.WaterDensity, up, out var reference))
            throw new InvalidOperationException("Cannot fit loading at this draft and reference pose.");
        float centreHeight = Mathf.Max(.05f, Vector3.Dot(up, body.centerOfMass) - reference.MinimumHeight);
        root.transform.rotation = rotation;
        var loading = buoyancy.GetComponent<HullDraftLoading>();
        if (loading == null) loading = buoyancy.gameObject.AddComponent<HullDraftLoading>();
        loading.ConfigureReferencePose(rotation, true, centreHeight);
        var probes = new SerializedObject(buoyancy);
        var positions = probes.FindProperty("probes");
        for (int i = 0; i < positions.arraySize; i++)
        {
            var point = Vector3.Scale(positions.GetArrayElementAtIndex(i).vector3Value, scale);
            point += up * (reference.Waterline - .1f - Vector3.Dot(up, point));
            positions.GetArrayElementAtIndex(i).vector3Value = new Vector3(point.x / scale.x, point.y / scale.y, point.z / scale.z);
        }
        probes.ApplyModifiedPropertiesWithoutUndo();
        var floating = new SerializedObject(root.GetComponent<FloatingShip>());
        floating.FindProperty("restingRootY").floatValue = -reference.Waterline;
        floating.ApplyModifiedPropertiesWithoutUndo();
        var position = root.transform.position;
        position.y = -reference.Waterline;
        root.transform.position = position;
        // Reset the native pose as well as Transform for fast Enter Play Mode.
        body.position = body.transform.position;
        body.rotation = body.transform.rotation;
        Physics.SyncTransforms();
        return reference.Waterline;
    }

    [MenuItem("Island/Boat Tests/Set Up Rowing Boat Draft Loading")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode before setting up draft loading.");
        var root = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
        try
        {
            float waterline = Configure(root);
            var body = root.GetComponentInChildren<OceanBuoyancy>(true).GetComponent<Rigidbody>();
            PrefabUtility.SaveAsPrefabAsset(root, BoatTestSceneSetup.BoatPrefab);
            // Replace the old, ineffective draft experiment on an open test instance.
            // Leave other unsaved scene edits in place, and leave saving to the user.
            var scene = SceneManager.GetSceneByPath(BoatTestSceneSetup.RowingScene);
            if (scene.IsValid() && scene.isLoaded)
                foreach (var sceneRoot in scene.GetRootGameObjects())
                    foreach (var instance in sceneRoot.GetComponentsInChildren<OceanBuoyancy>(true))
                        if (instance.GetComponent<HullDraftLoading>() != null)
                        {
                            Undo.RecordObject(instance, "Set rowing boat draft");
                            instance.Draft = DefaultDraft;
                            PrefabUtility.RecordPrefabInstancePropertyModifications(instance);
                            EditorSceneManager.MarkSceneDirty(scene);
                        }
            string report = $"Rowboat now uses Hull Draft Loading: draft {DefaultDraft:F2} m, loading mass {body.mass:F1} kg, local waterline {waterline:F3} m.\nChanging Ocean Buoyancy > Draft now adjusts mass and the boat settles to the new draft.\n";
            File.WriteAllText("/tmp/rowing-boat-draft-setup.txt", report);
            Debug.Log(report);
        }
        finally { PrefabUtility.UnloadPrefabContents(root); }
    }
}
