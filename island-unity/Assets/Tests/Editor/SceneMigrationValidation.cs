using System;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Motu.Settings;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public static class SceneMigrationValidation
    {
        internal static readonly string[] Scenes = {
            "Assets/Scenes/OpenSeaWorld.unity", "Assets/Scenes/IslandRuntimeSandbox.unity",
            "Assets/Scenes/OceanRuntimeSandbox.unity", "Assets/Scenes/TreeSandbox.unity"
        };

        public static void ValidateScenes()
        {
            foreach (var path in Scenes)
            {
                var scene = EditorSceneManager.OpenScene(path);
                foreach (var root in scene.GetRootGameObjects())
                foreach (var transform in root.GetComponentsInChildren<Transform>(true))
                {
                    if (GameObjectUtility.GetMonoBehavioursWithMissingScriptCount(transform.gameObject) != 0)
                        throw new InvalidOperationException($"Missing script in {path}: {transform.name}");
                    foreach (var component in transform.GetComponents<MonoBehaviour>())
                    {
                        if (component.GetType().Assembly.GetName().Name != "Motu.Runtime")
                            throw new InvalidOperationException($"Unmigrated component: {component.GetType()}");
                        var script = MonoScript.FromMonoBehaviour(component);
                        if (script == null || script.GetClass() != component.GetType())
                            throw new InvalidOperationException($"Script binding mismatch: {component.name}");
                    }
                }
            }
            if (AssetDatabase.LoadAssetAtPath<OceanWaveProfile>("Assets/Settings/OceanWaveProfile.asset") == null)
                throw new InvalidOperationException("The serialized ocean profile did not migrate.");
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            Debug.Log("All four supported scenes and the ocean profile retain their script bindings.");
        }

        public static void Reserialize()
        {
            ValidateScenes();
            AssetDatabase.ForceReserializeAssets(Scenes);
            AssetDatabase.ForceReserializeAssets(new[] { "Assets/Settings/OceanWaveProfile.asset" });
            AssetDatabase.SaveAssets();
            ValidateScenes();
        }
    }
}
