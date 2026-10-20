using System;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

public static class NpcPrefabSetup
{
    public const string PrefabPath = "Assets/Prefabs/NPC.prefab";

    [MenuItem("Island/Characters/Create NPC Prefab")]
    public static void Create()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (AssetDatabase.LoadAssetAtPath<GameObject>(PrefabPath) != null)
        {
            Selection.activeObject = AssetDatabase.LoadAssetAtPath<GameObject>(PrefabPath);
            Debug.Log("NPC prefab already exists; kept existing edits.");
            return;
        }
        var preview = EditorSceneManager.NewPreviewScene();
        try
        {
            var source = AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab");
            var npc = (GameObject)PrefabUtility.InstantiatePrefab(source, preview);
            npc.name = "NPC";
            npc.tag = "Untagged";
            npc.transform.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
            foreach (var audio in npc.GetComponentsInChildren<ShoreWaveAudio>(true)) UnityEngine.Object.DestroyImmediate(audio);
            foreach (var inspection in npc.GetComponentsInChildren<BoatTestPlayer>(true)) UnityEngine.Object.DestroyImmediate(inspection);
            foreach (var camera in npc.GetComponentsInChildren<Camera>(true)) UnityEngine.Object.DestroyImmediate(camera.gameObject);
            foreach (var listener in npc.GetComponentsInChildren<AudioListener>(true)) UnityEngine.Object.DestroyImmediate(listener);
            var motor = npc.GetComponent<PlayerController>();
            var settings = new SerializedObject(motor);
            settings.FindProperty("controlSource").enumValueIndex = (int)PlayerController.ControlSource.External;
            settings.ApplyModifiedPropertiesWithoutUndo();
            if (npc.GetComponent<BoatBoarding>() == null) npc.AddComponent<BoatBoarding>();
            npc.AddComponent<NpcController>();
            var footsteps = npc.GetComponent<AudioSource>();
            footsteps.spatialBlend = 1f;
            PrefabUtility.RecordPrefabInstancePropertyModifications(footsteps);
            var animator = npc.GetComponent<Animator>();
            animator.runtimeAnimatorController = AssetDatabase.LoadAssetAtPath<RuntimeAnimatorController>("Assets/TheOne.controller");
            animator.applyRootMotion = true;
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            PrefabUtility.RecordPrefabInstancePropertyModifications(animator);
            var asset = PrefabUtility.SaveAsPrefabAsset(npc, PrefabPath);
            Selection.activeObject = asset;
            EditorGUIUtility.PingObject(asset);
            Debug.Log("Created " + PrefabPath + " as a Player prefab variant sharing TheOne, root motion, foot/seat/oar IK and boat boarding. No camera, listener or player input; open scene left untouched.");
        }
        finally { EditorSceneManager.ClosePreviewScene(preview); }
    }
}
