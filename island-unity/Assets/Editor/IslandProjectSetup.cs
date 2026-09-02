using System;
using System.IO;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

public static class IslandProjectSetup
{
    private const string ScenePath = "Assets/Scenes/IslandSandbox.unity";
    private const string MultiIslandScenePath = "Assets/Scenes/IslandsSandbox.unity";
    private const string MaterialFolder = "Assets/Materials";
    private const string TreeWoodMaterialPath = "Assets/Materials/TreeWood.mat";
    private const string TreeFoliageMaterialPath = "Assets/Materials/TreeFoliage.mat";

    [MenuItem("Island/Create or Refresh Sandbox Level")]
    public static void CreateConventionalProjectAssets()
    {
        EnsureFolder("Assets", "Scenes");
        EnsureFolder("Assets", "Materials");

        var terrain = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandTerrain.mat",
            "Motu/Terrain Unified");
        var grass = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandGrass.mat",
            "Motu/Terrain Grass");
        var river = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandRiver.mat",
            "Motu/River Water");
        var sea = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandSea.mat",
            "Motu/Sea Water");
        var rock = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandRock.mat",
            "Motu/Rock Decoration");
        rock.enableInstancing = true;
        var treeWood = AssetDatabase.LoadAssetAtPath<Material>(TreeWoodMaterialPath);
        var treeFoliage = AssetDatabase.LoadAssetAtPath<Material>(TreeFoliageMaterialPath);
        if (treeWood == null || treeFoliage == null)
        {
            throw new InvalidOperationException(
                "Create or refresh the TreeSandbox materials before creating the island sandbox.");
        }

        var scene = EditorSceneManager.NewScene(
            NewSceneSetup.EmptyScene,
            NewSceneMode.Single);
        scene.name = "IslandSandbox";

        var islandObject = new GameObject("Island");
        var island = islandObject.AddComponent<IslandGenerator>();

        var sunObject = new GameObject("Sun");
        sunObject.transform.rotation = Quaternion.Euler(50f, -35f, 0f);
        var sun = sunObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.intensity = 1.25f;
        sun.color = new Color(1f, 0.94f, 0.82f);

        var cameraObject = new GameObject("Main Camera");
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.AddComponent<Camera>();
        camera.depthTextureMode |= DepthTextureMode.Depth;
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.49f, 0.68f, 0.82f);
        camera.nearClipPlane = 0.05f;
        camera.farClipPlane = island.WorldSizeMetres * 8f;
        var waterReflection = cameraObject.AddComponent<PlanarWaterReflection>();
        waterReflection.Configure(island.transform);
        cameraObject.AddComponent<RealTimeAmbientOcclusion>();
        cameraObject.AddComponent<AudioListener>();
        var orbit = cameraObject.AddComponent<OrbitCamera>();
        var firstPerson = cameraObject.AddComponent<FirstPersonController>();
        var demo = cameraObject.AddComponent<IslandDemoController>();
        demo.Configure(island, camera, orbit, firstPerson);

        island.ConfigureSceneReferences(
            cameraObject.transform,
            sun,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
        RenderSettings.fogMode = FogMode.ExponentialSquared;
        RenderSettings.fogColor = island.Rendering.DistanceHazeColour;
        RenderSettings.fogDensity = island.Rendering.DistanceHazeDensity;
        RenderSettings.sun = sun;

        EditorSceneManager.SaveScene(scene, ScenePath);
        EditorBuildSettings.scenes = new[]
        {
            new EditorBuildSettingsScene(ScenePath, true),
        };
        EditorUtility.SetDirty(terrain);
        EditorUtility.SetDirty(grass);
        EditorUtility.SetDirty(river);
        EditorUtility.SetDirty(sea);
        EditorUtility.SetDirty(rock);
        AssetDatabase.SaveAssets();
        AssetDatabase.Refresh();
        Debug.Log($"Created conventional island sandbox scene at {ScenePath}.");
    }

    [MenuItem("Island/Create or Refresh Multi-Island Sandbox")]
    public static void CreateMultiIslandSandbox()
    {
        EnsureFolder("Assets", "Scenes");
        EnsureFolder("Assets", "Materials");

        var terrain = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandTerrain.mat",
            "Motu/Terrain Unified");
        var grass = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandGrass.mat",
            "Motu/Terrain Grass");
        var river = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandRiver.mat",
            "Motu/River Water");
        var sea = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandSea.mat",
            "Motu/Sea Water");
        var rock = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandRock.mat",
            "Motu/Rock Decoration");
        rock.enableInstancing = true;
        var treeWood = AssetDatabase.LoadAssetAtPath<Material>(TreeWoodMaterialPath);
        var treeFoliage = AssetDatabase.LoadAssetAtPath<Material>(TreeFoliageMaterialPath);
        if (treeWood == null || treeFoliage == null)
        {
            throw new InvalidOperationException(
                "Create or refresh the TreeSandbox materials before creating the multi-island sandbox.");
        }

        var sceneExists = File.Exists(MultiIslandScenePath);
        var scene = sceneExists
            ? EditorSceneManager.OpenScene(MultiIslandScenePath, OpenSceneMode.Single)
            : EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
        if (!sceneExists)
        {
            scene.name = "IslandsSandbox";
        }
        foreach (var root in scene.GetRootGameObjects())
        {
            UnityEngine.Object.DestroyImmediate(root);
        }

        var worldObject = new GameObject("Open Sea World");
        worldObject.AddComponent<IslandWorldManager>();

        var sunObject = new GameObject("Sun");
        sunObject.transform.rotation = Quaternion.Euler(50f, -35f, 0f);
        var sun = sunObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.intensity = 1.25f;
        sun.color = new Color(1f, 0.94f, 0.82f);

        var cameraObject = new GameObject("Main Camera");
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.AddComponent<Camera>();
        camera.depthTextureMode |= DepthTextureMode.Depth;
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.49f, 0.68f, 0.82f);
        camera.nearClipPlane = 0.05f;
        camera.farClipPlane = 16000f;
        var waterReflection = cameraObject.AddComponent<PlanarWaterReflection>();
        cameraObject.AddComponent<RealTimeAmbientOcclusion>();
        cameraObject.AddComponent<AudioListener>();
        var orbit = cameraObject.AddComponent<OrbitCamera>();
        var firstPerson = cameraObject.AddComponent<FirstPersonController>();
        var demo = cameraObject.AddComponent<IslandDemoController>();

        var authority = CreateAuthoredIsland(
            "Island West (Environment Authority)",
            worldObject.transform,
            new Vector3(-2200f, 0f, 400f),
            666,
            -12f,
            cameraObject.transform,
            sun,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);
        CreateAuthoredIsland(
            "Island Central",
            worldObject.transform,
            new Vector3(0f, 0f, 700f),
            90210,
            8f,
            cameraObject.transform,
            sun,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);
        CreateAuthoredIsland(
            "Island East",
            worldObject.transform,
            new Vector3(2200f, 0f, 200f),
            271828,
            17f,
            cameraObject.transform,
            sun,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);

        waterReflection.Configure(authority.transform);
        demo.Configure(authority, camera, orbit, firstPerson);
        demo.ConfigureFlyStart(true, new Vector3(0f, 4f, -1500f), 0f, 0f);

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
        RenderSettings.fogMode = FogMode.ExponentialSquared;
        RenderSettings.fogColor = authority.Rendering.DistanceHazeColour;
        RenderSettings.fogDensity = authority.Rendering.DistanceHazeDensity;
        RenderSettings.sun = sun;

        EditorSceneManager.SaveScene(scene, MultiIslandScenePath);
        ValidateMultiIslandSandbox();
        AddSceneToBuildSettings(MultiIslandScenePath);
        EditorUtility.SetDirty(terrain);
        EditorUtility.SetDirty(grass);
        EditorUtility.SetDirty(river);
        EditorUtility.SetDirty(sea);
        EditorUtility.SetDirty(rock);
        AssetDatabase.SaveAssets();
        AssetDatabase.Refresh();
        Debug.Log($"Created multi-island flight sandbox at {MultiIslandScenePath}.");
    }

    public static void ValidateMultiIslandSandbox()
    {
        var scene = EditorSceneManager.OpenScene(
            MultiIslandScenePath,
            OpenSceneMode.Single);
        var managers = UnityEngine.Object.FindObjectsByType<IslandWorldManager>(
            FindObjectsInactive.Include,
            FindObjectsSortMode.None);
        var generators = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include,
            FindObjectsSortMode.None);
        var cameras = UnityEngine.Object.FindObjectsByType<Camera>(
            FindObjectsInactive.Include,
            FindObjectsSortMode.None);
        var demos = UnityEngine.Object.FindObjectsByType<IslandDemoController>(
            FindObjectsInactive.Include,
            FindObjectsSortMode.None);
        if (!scene.IsValid()
            || managers.Length != 1
            || generators.Length != 3
            || cameras.Length != 1
            || demos.Length != 1
            || cameras[0].GetComponent<FirstPersonController>() == null
            || cameras[0].GetComponent<OrbitCamera>() == null)
        {
            throw new InvalidOperationException(
                "The multi-island sandbox must contain one manager, three generators, and one fly-capable camera.");
        }
        var demoState = new SerializedObject(demos[0]);
        if (!demoState.FindProperty("startInFlyMode").boolValue)
        {
            throw new InvalidOperationException(
                "The multi-island sandbox camera must start in fly mode.");
        }
        foreach (var generator in generators)
        {
            if (!generator.transform.IsChildOf(managers[0].transform))
            {
                throw new InvalidOperationException(
                    "Every authored island must be parented below IslandWorldManager.");
            }
        }
    }

    private static IslandGenerator CreateAuthoredIsland(
        string name,
        Transform parent,
        Vector3 position,
        int seed,
        float yawDegrees,
        Transform streamingTarget,
        Light sunlight,
        Material terrain,
        Material grass,
        Material river,
        Material sea,
        Material rock,
        Material treeWood,
        Material treeFoliage)
    {
        var islandObject = new GameObject(name);
        islandObject.transform.SetParent(parent, false);
        islandObject.transform.SetPositionAndRotation(
            position,
            Quaternion.Euler(0f, yawDegrees, 0f));
        var island = islandObject.AddComponent<IslandGenerator>();
        island.Generation.Seed = seed;
        island.ConfigureSceneReferences(
            streamingTarget,
            sunlight,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);
        return island;
    }

    private static void AddSceneToBuildSettings(string path)
    {
        var scenes = new System.Collections.Generic.List<EditorBuildSettingsScene>(
            EditorBuildSettings.scenes);
        if (scenes.Exists(entry => entry.path == path))
        {
            return;
        }
        scenes.Add(new EditorBuildSettingsScene(path, true));
        EditorBuildSettings.scenes = scenes.ToArray();
    }

    private static Material CreateOrUpdateMaterial(string path, string shaderName)
    {
        var shader = Shader.Find(shaderName);
        if (shader == null)
        {
            throw new InvalidOperationException($"Could not find shader '{shaderName}'.");
        }
        var material = AssetDatabase.LoadAssetAtPath<Material>(path);
        if (material == null)
        {
            material = new Material(shader) { name = Path.GetFileNameWithoutExtension(path) };
            AssetDatabase.CreateAsset(material, path);
        }
        else
        {
            material.shader = shader;
        }
        return material;
    }

    private static void EnsureFolder(string parent, string name)
    {
        var path = $"{parent}/{name}";
        if (!AssetDatabase.IsValidFolder(path))
        {
            AssetDatabase.CreateFolder(parent, name);
        }
    }
}
