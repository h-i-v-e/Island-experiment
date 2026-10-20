using System;
using System.IO;
using Motu.Gameplay;
using Motu.Settings;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class BoatPushTestSceneSetup
{
    public const string ScenePath = "Assets/Scenes/BoatPushTest.unity";
    const string Folder = "Assets/BoatTesting/PushTest";
    const float Slope = 8f, ShoreHeight = -.35f;

    static PhysicsMaterial SandFriction()
    {
        var material = AssetDatabase.LoadAssetAtPath<PhysicsMaterial>(Folder + "/TestSand.physicMaterial");
        if (material != null) return material;
        Directory.CreateDirectory(Folder); AssetDatabase.Refresh();
        material = new PhysicsMaterial("TestSand") { staticFriction = .15f, dynamicFriction = .1f,
            frictionCombine = PhysicsMaterialCombine.Minimum };
        AssetDatabase.CreateAsset(material, Folder + "/TestSand.physicMaterial");
        return material;
    }

    public static void ConfigureShore(Scene scene)
    {
        if (scene.path != ScenePath) return;
        foreach (var root in scene.GetRootGameObjects())
        {
            if (root.name != "Sloping beach - 8 degrees" || !root.TryGetComponent<Collider>(out var shore)) continue;
            Undo.RecordObject(shore, "Configure push test sand friction");
            shore.sharedMaterial = SandFriction(); EditorSceneManager.MarkSceneDirty(scene);
        }
    }

    public static void CreateBatch()
    {
        if (!Application.isBatchMode || !File.Exists(".boat-exit-isolated-project")) throw new InvalidOperationException("Isolated project only.");
        EditorSceneManager.OpenScene("Assets/Scenes/RowingBoatCrewTest.unity");
        Create();
    }

    [MenuItem("Island/Boat Tests/Create Calm Beach Push Test Scene")]
    public static void Create()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (File.Exists(ScenePath))
        { Selection.activeObject = AssetDatabase.LoadAssetAtPath<SceneAsset>(ScenePath); EditorGUIUtility.PingObject(Selection.activeObject); return; }
        Directory.CreateDirectory(Folder); AssetDatabase.Refresh();
        var waves = AssetDatabase.LoadAssetAtPath<OceanWaveProfile>(Folder + "/CalmWater.asset");
        if (waves == null)
        {
            waves = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<OceanWaveProfile>("Assets/BoatTesting/BoatTestWaves.asset"));
            var data = new SerializedObject(waves);
            for (int i = 0; i < 4; i++)
            {
                data.FindProperty("wave" + i).FindPropertyRelative("amplitudeMetres").floatValue = 0;
                data.FindProperty("wave" + i).FindPropertyRelative("choppiness").floatValue = 0;
            }
            data.FindProperty("onshoreWaveEnabled").boolValue = false;
            data.FindProperty("whitecapStrength").floatValue = 0;
            data.ApplyModifiedPropertiesWithoutUndo();
            AssetDatabase.CreateAsset(waves, Folder + "/CalmWater.asset");
        }
        var config = AssetDatabase.LoadAssetAtPath<WorldEnvironmentConfiguration>(Folder + "/CalmEnvironment.asset");
        if (config == null)
        {
            config = UnityEngine.Object.Instantiate(AssetDatabase.LoadAssetAtPath<WorldEnvironmentConfiguration>("Assets/BoatTesting/BoatTestEnvironment.asset"));
            var data = new SerializedObject(config);
            data.FindProperty("environment").FindPropertyRelative("oceanWaveProfile").objectReferenceValue = waves;
            data.FindProperty("environment").FindPropertyRelative("windSpeedMetresPerSecond").floatValue = 0;
            data.FindProperty("environment").FindPropertyRelative("startingSolarTimeHours").floatValue = 12;
            data.ApplyModifiedPropertiesWithoutUndo();
            AssetDatabase.CreateAsset(config, Folder + "/CalmEnvironment.asset");
        }
        var sand = AssetDatabase.LoadAssetAtPath<Material>(Folder + "/TestSand.mat");
        if (sand == null)
        {
            sand = new Material(Shader.Find("Universal Render Pipeline/Lit"));
            sand.SetColor("_BaseColor", new Color(.64f, .51f, .31f)); sand.SetFloat("_Smoothness", .12f);
            AssetDatabase.CreateAsset(sand, Folder + "/TestSand.mat");
        }
        var previous = SceneManager.GetActiveScene();
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Additive);
        try
        {
            SceneManager.SetActiveScene(scene);
            var beach = GameObject.CreatePrimitive(PrimitiveType.Cube); beach.name = "Sloping beach - 8 degrees";
            beach.transform.SetPositionAndRotation(new Vector3(0, ShoreHeight - .5f / Mathf.Cos(Slope * Mathf.Deg2Rad), 0), Quaternion.Euler(Slope, 0, 0));
            beach.transform.localScale = new Vector3(28, 1, 70);
            beach.GetComponent<Renderer>().sharedMaterial = sand;
            beach.GetComponent<Collider>().sharedMaterial = SandFriction();

            var boat = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/BoatTesting/RowingBoatTest.prefab"));
            boat.name = "Beached rowboat - bow towards sea";
            boat.transform.SetPositionAndRotation(new Vector3(0, -.28f, 0), Quaternion.Euler(Slope, 0, 0));
            var floating = new SerializedObject(boat.GetComponent<FloatingShip>());
            floating.FindProperty("restingRootY").floatValue = boat.transform.position.y; floating.ApplyModifiedPropertiesWithoutUndo();
            PrefabUtility.RecordPrefabInstancePropertyModifications(boat.GetComponent<FloatingShip>());
            var buoyancy = boat.GetComponent<FloatingShip>().Hull.GetComponent<OceanBuoyancy>();
            buoyancy.WaterCurrent = Vector3.zero; PrefabUtility.RecordPrefabInstancePropertyModifications(buoyancy);

            var player = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"));
            player.name = "Player - push from stern";
            const float playerZ = -4.5f;
            player.transform.SetPositionAndRotation(new Vector3(0, ShoreHeight - playerZ * Mathf.Tan(Slope * Mathf.Deg2Rad) + .10f, playerZ), Quaternion.identity);
            var camera = player.GetComponentInChildren<Camera>(); camera.farClipPlane = 3000;
            var environment = new GameObject("Calm sea and sky").AddComponent<BoatTestEnvironment>();
            var settings = new SerializedObject(environment);
            settings.FindProperty("configuration").objectReferenceValue = config;
            settings.FindProperty("playerCamera").objectReferenceValue = camera; settings.ApplyModifiedPropertiesWithoutUndo();
            var sun = new GameObject("Sun").AddComponent<Light>(); sun.type = LightType.Directional;
            sun.intensity = 1.25f; sun.shadows = LightShadows.Soft; sun.transform.rotation = Quaternion.Euler(60, -35, 0);
            RenderSettings.sun = sun; RenderSettings.ambientMode = UnityEngine.Rendering.AmbientMode.Flat;
            RenderSettings.ambientLight = new Color(.45f, .52f, .6f);
            Transform Spawn(string name, Transform source)
            {
                var marker = new GameObject(name).transform; marker.SetPositionAndRotation(source.position, source.rotation); return marker;
            }
            new GameObject("Push test controls - R to reset").AddComponent<BoatPushTestScenario>().Configure(
                boat.GetComponent<RowboatController>(), player.GetComponent<PlayerController>(), Spawn("Boat start", boat.transform), Spawn("Player start", player.transform));
            if (!EditorSceneManager.SaveScene(scene, ScenePath)) throw new IOException("Could not save boat push test scene.");
        }
        finally { SceneManager.SetActiveScene(previous); EditorSceneManager.CloseScene(scene, true); }
        Selection.activeObject = AssetDatabase.LoadAssetAtPath<SceneAsset>(ScenePath); EditorGUIUtility.PingObject(Selection.activeObject);
        Debug.Log("Created " + ScenePath + ". Calm sea, native 8-degree beach collider, beached rowboat and shore player. E pushes; R resets. Existing scenes retained.");
    }
}
