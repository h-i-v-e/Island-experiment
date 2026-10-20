using System;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEditor.Animations;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class BoatPushSetup
{
    const string ClipPath = "Assets/Small_Boat_Anim_Set/Animations/Boat_Push_out_to_Sea.fbx";
    const string BoatPrefab = "Assets/BoatTesting/RowingBoatTest.prefab";
    public const string ReportPath = "/tmp/boat-push-setup.txt";

    [MenuItem("Island/Boat Tests/Set Up Grounded Boat Pushing")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var report = new StringBuilder();
        var controller = AssetDatabase.LoadAssetAtPath<AnimatorController>("Assets/TheOne.controller");
        var layers = controller.layers;
        // Existing boarding profiles use this base-layer path. Preserve the new push state and trigger.
        layers[0].name = "Base Layer"; layers[0].iKPass = true;
        controller.layers = layers;
        var state = layers[0].stateMachine.states.Select(s => s.state).Single(s => s.name == "PushOutBoat");
        state.tag = "Boat";
        foreach (var transition in layers[0].stateMachine.states.SelectMany(s => s.state.transitions).Where(t => t.destinationState == state))
        { transition.hasExitTime = false; transition.duration = .12f; transition.canTransitionToSelf = false; EditorUtility.SetDirty(transition); }
        EditorUtility.SetDirty(state); EditorUtility.SetDirty(controller); AssetDatabase.SaveAssetIfDirty(controller);
        var importer = (ModelImporter)AssetImporter.GetAtPath(ClipPath);
        var clips = importer.clipAnimations;
        if (clips.Any(c => c.loopTime))
        {
            foreach (var clip in clips) clip.loopTime = false;
            importer.clipAnimations = clips; importer.SaveAndReimport();
        }

        var player = PrefabUtility.LoadPrefabContents("Assets/Prefabs/Player.prefab");
        try
        {
            var pushing = player.GetComponent<BoatPushing>() ?? player.AddComponent<BoatPushing>();
            var settings = new SerializedObject(pushing);
            settings.FindProperty("interactionRange").floatValue = Mathf.Min(settings.FindProperty("interactionRange").floatValue, 1.35f);
            settings.ApplyModifiedPropertiesWithoutUndo();
            PrefabUtility.SaveAsPrefabAsset(player, "Assets/Prefabs/Player.prefab");
        }
        finally { PrefabUtility.UnloadPrefabContents(player); }

        var boat = PrefabUtility.LoadPrefabContents(BoatPrefab);
        try { Configure(boat); PrefabUtility.SaveAsPrefabAsset(boat, BoatPrefab); }
        finally { PrefabUtility.UnloadPrefabContents(boat); }
        for (int i = 0; i < SceneManager.sceneCount; i++)
        {
            var scene = SceneManager.GetSceneAt(i);
            if (!scene.isLoaded || EditorSceneManager.IsPreviewScene(scene)) continue;
            BoatPushTestSceneSetup.ConfigureShore(scene);
            foreach (var root in scene.GetRootGameObjects())
                foreach (var rowboat in root.GetComponentsInChildren<RowboatController>(true))
                {
                    if (PrefabUtility.GetPrefabAssetPathOfNearestInstanceRoot(rowboat.gameObject) != BoatPrefab) continue;
                    Undo.RegisterFullObjectHierarchyUndo(rowboat.gameObject, "Use animated palm hull contacts");
                    Configure(rowboat.gameObject); EditorSceneManager.MarkSceneDirty(scene);
                }
        }
        report.AppendLine("Installed in-place E push: animated palms detect actual hull contacts; no standing spots or hand markers. Scenes left unsaved.");
        File.WriteAllText(ReportPath, report.ToString()); Debug.Log(report.ToString());
    }
    static void Configure(GameObject root)
    {
        var group = root.transform.Find("Push spots");
        if (group != null) UnityEngine.Object.DestroyImmediate(group.gameObject);
        var hull = root.GetComponentsInChildren<MeshCollider>(true).Where(c => !c.convex && !c.isTrigger && c.sharedMesh != null).Cast<Collider>().ToArray();
        if (hull.Length == 0) throw new InvalidOperationException("Rowboat needs a wooden hull surface collider.");
        var pushable = root.GetComponent<BoatPushable>() ?? root.AddComponent<BoatPushable>();
        pushable.Configure(hull);
        if (PrefabUtility.IsPartOfPrefabInstance(pushable)) PrefabUtility.RecordPrefabInstancePropertyModifications(pushable);
    }
}
