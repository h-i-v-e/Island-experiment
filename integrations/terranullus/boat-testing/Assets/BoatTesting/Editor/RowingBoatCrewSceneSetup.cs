using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class RowingBoatCrewSceneSetup
{
    public const string ScenePath = "Assets/Scenes/RowingBoatCrewTest.unity";

    [MenuItem("Island/Boat Tests/Create Crewed Rowboat Test Scene")]
    public static void Create()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (File.Exists(ScenePath))
        {
            Selection.activeObject = AssetDatabase.LoadAssetAtPath<SceneAsset>(ScenePath);
            EditorGUIUtility.PingObject(Selection.activeObject);
            return;
        }
        var previous = SceneManager.GetActiveScene();
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Additive);
        try
        {
            SceneManager.SetActiveScene(scene);
            var boat = Place(BoatTestSceneSetup.BoatPrefab, "Rowing Boat").GetComponent<RowboatController>();
            var seats = boat.GetComponentsInChildren<BoatSeat>().Where(s => s.Role == BoatSeat.SeatRole.Rower)
                .OrderBy(s => s.transform.localPosition.z).ToArray();
            if (seats.Length != 2 || boat.CommandSeat == null) throw new InvalidOperationException("Expected two rower seats and an aft command seat.");
            var player = Place("Assets/Prefabs/Player.prefab", "Player - aft passenger").GetComponent<PlayerController>();
            PlaceAtSeat(player.gameObject, boat.CommandSeat);
            var crew = new NpcController[2];
            for (int i = 0; i < crew.Length; i++)
            {
                crew[i] = Place(NpcPrefabSetup.PrefabPath, i == 0 ? "Rower - aft" : "Rower - forward").GetComponent<NpcController>();
                PlaceAtSeat(crew[i].gameObject, seats[i]);
            }
            var camera = player.GetComponentInChildren<Camera>();
            camera.farClipPlane = 5000f;
            var environment = new GameObject("Sea and Sky").AddComponent<BoatTestEnvironment>();
            var data = new SerializedObject(environment);
            data.FindProperty("configuration").objectReferenceValue = AssetDatabase.LoadAssetAtPath<UnityEngine.Object>(BoatTestSceneSetup.Folder + "/BoatTestEnvironment.asset");
            data.FindProperty("playerCamera").objectReferenceValue = camera;
            data.ApplyModifiedPropertiesWithoutUndo();
            var sun = new GameObject("Sun").AddComponent<Light>();
            sun.type = LightType.Directional;
            sun.intensity = 1.25f;
            sun.shadows = LightShadows.Soft;
            sun.transform.rotation = Quaternion.Euler(60f, -35f, 0f);
            RenderSettings.sun = sun;
            RenderSettings.ambientMode = UnityEngine.Rendering.AmbientMode.Flat;
            RenderSettings.ambientLight = new Color(.45f, .52f, .6f);
            new GameObject("Crewed Boat Test").AddComponent<RowingBoatTestScenario>().Configure(boat, player, crew, seats);
            if (!EditorSceneManager.SaveScene(scene, ScenePath)) throw new IOException("Could not save crewed rowboat test scene.");
        }
        finally
        {
            SceneManager.SetActiveScene(previous);
            EditorSceneManager.CloseScene(scene, true);
        }
        Selection.activeObject = AssetDatabase.LoadAssetAtPath<SceneAsset>(ScenePath);
        EditorGUIUtility.PingObject(Selection.activeObject);
        Debug.Log("Created " + ScenePath + ". Open it and press Play; W rows, A/D steer, E exits, R returns aboard. Existing editing scenes retained.");
    }

    private static GameObject Place(string path, string name)
    {
        var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(path);
        if (prefab == null) throw new InvalidOperationException("Missing prefab: " + path);
        var instance = (GameObject)PrefabUtility.InstantiatePrefab(prefab);
        instance.name = name;
        return instance;
    }

    private static void PlaceAtSeat(GameObject character, BoatSeat seat)
    {
        var pose = seat.SeatedRoot(character.GetComponent<Animator>().humanScale);
        character.transform.SetPositionAndRotation(pose.position, pose.rotation);
    }
}
