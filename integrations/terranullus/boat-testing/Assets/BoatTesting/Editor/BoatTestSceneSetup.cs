using System;
using System.IO;
using System.Linq;
using System.Text;
using Motu.Gameplay;
using Motu.Rendering;
using Motu.Samples;
using Motu.Settings;
using Motu.World;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class BoatTestSceneSetup
{
    public const string Folder = "Assets/BoatTesting";
    public const string BoatSource = "Assets/Synty/PolygonPirates/Prefabs/Vehicles/SM_Veh_Boat_Rowing_01_Hull_Attachments.prefab";
    public const string BoatPrefab = Folder + "/RowingBoatTest.prefab";
    public const string EmptyScene = "Assets/Scenes/BoatTestbed.unity";
    public const string RowingScene = "Assets/Scenes/RowingBoatTest.unity";
    private const string ReportPath = "/tmp/terranullus-boat-tests.txt";

    [MenuItem("Island/Boat Tests/Create Test Scenes")]
    public static void Create()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        if (File.Exists(EmptyScene) || File.Exists(RowingScene) || File.Exists(BoatPrefab))
            throw new InvalidOperationException("Test assets already exist. Keep scene edits; delete only the generated test assets to regenerate.");
        var previous = SceneManager.GetActiveScene();
        var working = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Additive);
        var report = new StringBuilder();
        try
        {
            SceneManager.SetActiveScene(working);
            var waves = ScriptableObject.CreateInstance<OceanWaveProfile>();
            var waveSettings = new SerializedObject(waves);
            waveSettings.FindProperty("onshoreWaveEnabled").boolValue = false;
            waveSettings.FindProperty("fineVertexSpacingMetres").floatValue = .5f;
            waveSettings.FindProperty("fineRadiusMetres").floatValue = 64f;
            waveSettings.FindProperty("displacementFadeStartMetres").floatValue = 384f;
            waveSettings.FindProperty("displacementFadeEndMetres").floatValue = 512f;
            waveSettings.FindProperty("domainWarpMetres").floatValue = 4f;
            waveSettings.FindProperty("amplitudeVariation").floatValue = .15f;
            for (int i = 0; i < 4; i++)
            {
                var wave = waveSettings.FindProperty("wave" + i);
                wave.FindPropertyRelative("amplitudeMetres").floatValue = new[] { .08f, .035f, .015f, .006f }[i];
                wave.FindPropertyRelative("wavelengthMetres").floatValue = new[] { 16f, 8f, 4f, 2f }[i];
                wave.FindPropertyRelative("speedMetresPerSecond").floatValue = new[] { 2f, 1.5f, 1f, .7f }[i];
            }
            waveSettings.ApplyModifiedPropertiesWithoutUndo();
            AssetDatabase.CreateAsset(waves, Folder + "/BoatTestWaves.asset");
            var config = ScriptableObject.CreateInstance<WorldEnvironmentConfiguration>();
            var settings = new SerializedObject(config);
            var env = settings.FindProperty("environment");
            env.FindPropertyRelative("oceanWaveProfile").objectReferenceValue = waves;
            env.FindPropertyRelative("startingSolarTimeHours").floatValue = 12f;
            env.FindPropertyRelative("sunLatitudeDegrees").floatValue = -25f;
            env.FindPropertyRelative("sunCycleDurationMinutes").floatValue = 240f;
            env.FindPropertyRelative("windSpeedMetresPerSecond").floatValue = 9f;
            env.FindPropertyRelative("seaMaterial").objectReferenceValue = AssetDatabase.LoadAssetAtPath<Material>(AssetDatabase.GUIDToAssetPath("f7e64c96dada84bf1bae7f46e0e94f50"));
            settings.ApplyModifiedPropertiesWithoutUndo();
            config.Clouds.Enabled = false;
            AssetDatabase.CreateAsset(config, Folder + "/BoatTestEnvironment.asset");
            var boat = BuildBoat(report, out var boardingPosition);
            var boatAsset = PrefabUtility.SaveAsPrefabAsset(boat, BoatPrefab);
            UnityEngine.Object.DestroyImmediate(boat);
            var light = new GameObject("Sun").AddComponent<Light>();
            light.type = LightType.Directional;
            light.intensity = 1.25f;
            light.shadows = LightShadows.Soft;
            light.transform.rotation = Quaternion.Euler(60f, -35f, 0f);
            RenderSettings.sun = light;
            RenderSettings.ambientMode = UnityEngine.Rendering.AmbientMode.Flat;
            RenderSettings.ambientLight = new Color(.45f, .52f, .6f);
            var player = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab"));
            player.name = "Player";
            player.transform.SetPositionAndRotation(new Vector3(0f, 2f, -6f), Quaternion.identity);
            var view = player.GetComponentInChildren<Camera>();
            view.farClipPlane = 5000f;
            var controls = player.AddComponent<BoatTestPlayer>();
            var environment = new GameObject("Sea and Sky").AddComponent<BoatTestEnvironment>();
            Set(environment, "configuration", config);
            Set(environment, "playerCamera", view);
            EditorSceneManager.SaveScene(working, EmptyScene);
            var placed = (GameObject)PrefabUtility.InstantiatePrefab(boatAsset);
            placed.name = "Rowing Boat with Attachments";
            player.transform.position = placed.transform.TransformPoint(boardingPosition);
            var controlSettings = new SerializedObject(controls);
            controlSettings.FindProperty("startFlying").boolValue = false;
            controlSettings.FindProperty("resetTarget").objectReferenceValue = placed.transform;
            controlSettings.FindProperty("resetLocalPosition").vector3Value = boardingPosition;
            controlSettings.ApplyModifiedPropertiesWithoutUndo();
            EditorSceneManager.SaveScene(working, RowingScene);
            AssetDatabase.SaveAssets();
            report.AppendLine("Created reusable sea/player template and populated first-test scene.");
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); Debug.LogException(error); }
        finally
        {
            SceneManager.SetActiveScene(previous);
            EditorSceneManager.CloseScene(working, true);
            File.WriteAllText(ReportPath, report.ToString());
        }
        Validate();
    }

    private static GameObject BuildBoat(StringBuilder report, out Vector3 boardingPosition)
    {
        var root = new GameObject("Rowing Boat Test");
        var model = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>(BoatSource), root.transform);
        var walk = model.GetComponent<MeshCollider>();
        var collisionMesh = walk.sharedMesh;
        var mesh = model.GetComponent<MeshFilter>().sharedMesh;
        walk.convex = false;
        walk.sharedMesh = mesh;
        var bounds = mesh.bounds;
        var physics = new GameObject("Boat Physics");
        physics.transform.SetParent(root.transform, false);
        var body = physics.AddComponent<Rigidbody>();
        // Hull Draft Loading derives mass from the requested draft after the geometry is baked.
        body.interpolation = RigidbodyInterpolation.Interpolate;
        body.collisionDetectionMode = CollisionDetectionMode.ContinuousDynamic;
        body.linearDamping = 0f;
        body.angularDamping = .1f;
        body.centerOfMass = new Vector3(bounds.center.x, bounds.min.y + .15f, bounds.center.z);
        var collision = physics.AddComponent<MeshCollider>();
        collision.sharedMesh = collisionMesh;
        collision.convex = true;
        var profile = ScriptableObject.CreateInstance<ShipHullGeometry>();
        var sections = ExplorerShipPhysicsSetup.Slice(mesh, Matrix4x4.identity, 32);
        var serialized = new SerializedObject(profile);
        var entries = serialized.FindProperty("sections");
        entries.arraySize = sections.Count;
        for (int i = 0; i < sections.Count; i++)
        {
            var entry = entries.GetArrayElementAtIndex(i);
            entry.FindPropertyRelative("station").floatValue = sections[i].station;
            entry.FindPropertyRelative("length").floatValue = sections[i].length;
            var points = entry.FindPropertyRelative("outline");
            points.arraySize = sections[i].outline.Length;
            for (int j = 0; j < points.arraySize; j++) points.GetArrayElementAtIndex(j).vector2Value = sections[i].outline[j];
        }
        serialized.ApplyModifiedPropertiesWithoutUndo();
        AssetDatabase.CreateAsset(profile, Folder + "/RowingBoatHull.asset");
        float waterline = bounds.min.y + RowingBoatDraftSetup.DefaultDraft;
        var buoyancy = physics.AddComponent<OceanBuoyancy>();
        Set(buoyancy, "hullGeometry", profile);
        var bs = new SerializedObject(buoyancy);
        var probes = bs.FindProperty("probes");
        probes.arraySize = 6;
        for (int i = 0; i < 6; i++)
            probes.GetArrayElementAtIndex(i).vector3Value = new Vector3(bounds.center.x + (i % 2 == 0 ? -.65f : .65f) * bounds.extents.x,
                waterline - .1f, Mathf.Lerp(bounds.min.z, bounds.max.z, .2f + (i / 2) * .3f));
        bs.ApplyModifiedPropertiesWithoutUndo();
        var floating = root.AddComponent<FloatingShip>();
        Set(floating, "hull", body);
        Set(floating, "buoyancy", buoyancy);
        var fs = new SerializedObject(floating);
        fs.FindProperty("restingRootY").floatValue = -waterline;
        fs.ApplyModifiedPropertiesWithoutUndo();
        waterline = RowingBoatDraftSetup.Configure(root);
        // Fit the main shell's interior; keel, rudder and attached oars must not enlarge it.
        var volumeAsset = ScriptableObject.CreateInstance<OceanHullVolume>();
        AssetDatabase.CreateAsset(volumeAsset, Folder + "/RowingBoatWaterExclusion.asset");
        RowingBoatWaterExclusionSetup.Fit(volumeAsset, mesh, Matrix4x4.identity);
        var clamp = root.AddComponent<OceanDeckWaveClamp>();
        clamp.ConfigureConvexHull(volumeAsset);
        Physics.SyncTransforms();
        var ray = new Ray(new Vector3(0f, bounds.max.y + 3f, bounds.center.z), Vector3.down);
        if (!walk.Raycast(ray, out var hit, 10f)) throw new InvalidOperationException("No boardable centre surface on rowboat.");
        boardingPosition = root.transform.InverseTransformPoint(hit.point) + Vector3.up * .08f;
        report.AppendLine($"Synty hull bounds: {bounds}; draft-controlled waterline: {waterline:F3} m; boarding local: {boardingPosition}");
        return root;
    }

    private static void Set(UnityEngine.Object target, string field, UnityEngine.Object value)
    {
        var data = new SerializedObject(target);
        data.FindProperty(field).objectReferenceValue = value;
        data.ApplyModifiedPropertiesWithoutUndo();
    }

    private static void Check(bool condition, string message, StringBuilder report)
    {
        if (!condition) throw new InvalidOperationException(message);
        report.AppendLine("PASS: " + message);
    }

    [MenuItem("Island/Boat Tests/Validate Test Scenes")]
    public static void Validate()
    {
        var report = new StringBuilder();
        var previous = SceneManager.GetActiveScene();
        try
        {
            foreach (string path in new[] { EmptyScene, RowingScene })
            {
                var scene = EditorSceneManager.OpenScene(path, OpenSceneMode.Additive);
                try
                {
                    var all = scene.GetRootGameObjects().SelectMany(root => root.GetComponentsInChildren<Transform>(true)).ToArray();
                    Check(all.Count(t => t.GetComponent<BoatTestEnvironment>() != null) == 1, path + ": one standalone sea/sky", report);
                    Check(all.Count(t => t.GetComponent<BoatTestPlayer>() != null) == 1, "one real player and test controls", report);
                    Check(all.Count(t => t.GetComponent<Camera>() != null) == 1 && all.Count(t => t.GetComponent<AudioListener>() != null) == 1, "one camera/listener", report);
                    Check(!all.Any(t => t.GetComponent<IslandWorldManager>() != null), "no island generation or terrain streaming", report);
                    Check(!all.Any(t => GameObjectUtility.GetMonoBehavioursWithMissingScriptCount(t.gameObject) > 0), "all scripts resolve", report);
                    var boat = all.Select(t => t.GetComponent<FloatingShip>()).FirstOrDefault(t => t != null);
                    if (path == EmptyScene) Check(boat == null, "empty template has no boat", report);
                    else
                    {
                        Check(boat != null, "populated scene has floating boat", report);
                        Check(boat.GetComponentsInChildren<Transform>().Count(t => t.name.StartsWith("SM_Prop_Oar")) == 4, "four Synty oar attachments retained", report);
                        var buoyancy = boat.GetComponentInChildren<OceanBuoyancy>();
                        Check(buoyancy != null && buoyancy.HullGeometry.Sections.Length == 32, "32 baked hull sections drive shared ocean buoyancy", report);
                        Check(boat.GetComponentsInChildren<Rigidbody>().Length == 1 && !buoyancy.GetComponent<Rigidbody>().isKinematic, "one dynamic hull body", report);
                        Check(boat.GetComponentsInChildren<MeshCollider>().Any(c => !c.convex), "detailed walkable interior separate from convex body", report);
                        Check(boat.GetComponent<OceanDeckWaveClamp>().ConvexHull.IsValid, "baked water exclusion follows the hull", report);
                    }
                }
                finally { SceneManager.SetActiveScene(previous); EditorSceneManager.CloseScene(scene, true); }
            }
            report.AppendLine("SUCCESS: Both boat test scenes passed.");
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); Debug.LogException(error); }
        File.AppendAllText(ReportPath, report.ToString());
        Debug.Log(report.ToString());
    }
}
