using System;
using System.IO;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

public static class IslandProjectSetup
{
    private const string ScenePath = "Assets/Scenes/IslandRuntimeSandbox.unity";
    private const string MultiIslandScenePath = "Assets/Scenes/OpenSeaWorld.unity";
    private const string OceanWaveScenePath = "Assets/Scenes/OceanRuntimeSandbox.unity";
    private const string MaterialFolder = "Assets/Materials";
    private const string IslandConfigurationPath =
        "Assets/Settings/IslandRuntimeConfiguration.asset";
    private const string OpenSeaConfigurationPath =
        "Assets/Settings/OpenSeaIslandConfiguration.asset";
    private const string OceanWaveProfilePath = "Assets/Settings/OceanWaveProfile.asset";
    private const string TreeWoodMaterialPath = "Assets/Materials/TreeWood.mat";
    private const string TreeFoliageMaterialPath = "Assets/Materials/TreeFoliage.mat";

    public static void CreateReplacementScenes()
    {
        CreateConventionalProjectAssets();
        CreateMultiIslandSandbox();
        CreateOceanWaveSandbox();
    }

    [MenuItem("Island/Create or Refresh Sandbox Level")]
    public static void CreateConventionalProjectAssets()
    {
        EnsureFolder("Assets", "Scenes");
        EnsureFolder("Assets", "Materials");
        EnsureFolder("Assets", "Settings");

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
        scene.name = "IslandRuntimeSandbox";

        var worldObject = new GameObject("Island World");
        var worldManager = worldObject.AddComponent<IslandWorldManager>();
        var requestFactory = worldObject.AddComponent<GridIslandGenerationRequestFactory>();

        var sunObject = new GameObject("Sun");
        sunObject.transform.rotation = Quaternion.Euler(50f, -35f, 0f);
        var sun = sunObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.intensity = 1.25f;
        sun.color = new Color(1f, 0.94f, 0.82f);
        worldManager.ConfigureWorldEnvironment(sun, sea);

        var cameraObject = new GameObject("Main Camera");
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.AddComponent<Camera>();
        camera.depthTextureMode |= DepthTextureMode.Depth;
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.49f, 0.68f, 0.82f);
        camera.nearClipPlane = 0.05f;
        camera.farClipPlane = 16000f;
        var waterReflection = cameraObject.AddComponent<PlanarWaterReflection>();
        waterReflection.Configure(worldObject.transform);
        cameraObject.AddComponent<RealTimeAmbientOcclusion>();
        cameraObject.AddComponent<AudioListener>();
        var orbit = cameraObject.AddComponent<OrbitCamera>();
        var firstPerson = cameraObject.AddComponent<FirstPersonController>();
        var demo = cameraObject.AddComponent<IslandDemoController>();
        demo.Configure(worldManager, camera, orbit, firstPerson);

        var configuration = CreateOrUpdateIslandConfiguration(
            IslandConfigurationPath,
            666,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);
        requestFactory.Configure(configuration, false, 0f);
        requestFactory.SetFixedIslands(
            new GridIslandGenerationRequestFactory.FixedIsland(
                Vector2Int.zero,
                configuration,
                "sandbox-origin"));
        worldManager.ConfigureIslandGenerationRequestFactory(requestFactory);
        worldManager.ConfigureWorldSeed(666);
        worldManager.SetStreamingTarget(cameraObject.transform);

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
        RenderSettings.fogMode = FogMode.ExponentialSquared;
        RenderSettings.fogColor = configuration.Rendering.DistanceHazeColour;
        RenderSettings.fogDensity = configuration.Rendering.DistanceHazeDensity;
        RenderSettings.sun = sun;

        EditorSceneManager.SaveScene(scene, ScenePath);
        AddSceneToBuildSettings(ScenePath);
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
        EnsureFolder("Assets", "Settings");

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
            scene.name = "OpenSeaWorld";
        }
        foreach (var root in scene.GetRootGameObjects())
        {
            UnityEngine.Object.DestroyImmediate(root);
        }

        var worldObject = new GameObject("Open Sea World");
        var worldManager = worldObject.AddComponent<IslandWorldManager>();
        var requestFactory = worldObject.AddComponent<GridIslandGenerationRequestFactory>();

        var sunObject = new GameObject("Sun");
        sunObject.transform.rotation = Quaternion.Euler(50f, -35f, 0f);
        var sun = sunObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.intensity = 1.25f;
        sun.color = new Color(1f, 0.94f, 0.82f);
        worldManager.ConfigureWorldEnvironment(sun, sea);

        var configuration = CreateOrUpdateIslandConfiguration(
            OpenSeaConfigurationPath,
            666,
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);

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

        requestFactory.Configure(configuration, true, 0.32f);
        requestFactory.SetFixedIslands(
            new GridIslandGenerationRequestFactory.FixedIsland(
                new Vector2Int(-1, 0),
                configuration,
                "open-sea-west"),
            new GridIslandGenerationRequestFactory.FixedIsland(
                Vector2Int.zero,
                configuration,
                "open-sea-central"),
            new GridIslandGenerationRequestFactory.FixedIsland(
                new Vector2Int(1, 0),
                configuration,
                "open-sea-east"));
        worldManager.ConfigureIslandGenerationRequestFactory(requestFactory);
        worldManager.ConfigureWorldSeed(8675309);
        worldManager.SetStreamingTarget(cameraObject.transform);

        waterReflection.Configure(worldObject.transform);
        demo.Configure(worldManager, camera, orbit, firstPerson);
        demo.ConfigureFlyStart(true, new Vector3(0f, 4f, -1500f), 0f, 0f);

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
        RenderSettings.fogMode = FogMode.ExponentialSquared;
        RenderSettings.fogColor = configuration.Rendering.DistanceHazeColour;
        RenderSettings.fogDensity = configuration.Rendering.DistanceHazeDensity;
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

    [MenuItem("Island/Create or Refresh Ocean Wave Sandbox")]
    public static void CreateOceanWaveSandbox()
    {
        EnsureFolder("Assets", "Scenes");
        EnsureFolder("Assets", "Materials");
        EnsureFolder("Assets", "Settings");
        var sea = CreateOrUpdateMaterial(
            $"{MaterialFolder}/IslandSea.mat",
            "Motu/Sea Water");
        var scene = EditorSceneManager.NewScene(
            NewSceneSetup.EmptyScene,
            NewSceneMode.Single);
        scene.name = "OceanRuntimeSandbox";
        var profile = AssetDatabase.LoadAssetAtPath<OceanWaveProfile>(
            OceanWaveProfilePath);
        if (profile == null)
        {
            profile = ScriptableObject.CreateInstance<OceanWaveProfile>();
            AssetDatabase.CreateAsset(profile, OceanWaveProfilePath);
            AssetDatabase.SaveAssets();
            profile = AssetDatabase.LoadAssetAtPath<OceanWaveProfile>(
                OceanWaveProfilePath);
        }

        var oceanObject = new GameObject("Ocean Wave Test Environment");
        oceanObject.AddComponent<OceanSurfaceController>();
        var sandbox = oceanObject.AddComponent<OceanWaveSandboxController>();

        var sunObject = new GameObject("Sun");
        sunObject.transform.rotation = Quaternion.Euler(42f, -28f, 0f);
        var sun = sunObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.intensity = 1.2f;
        sun.color = new Color(1f, 0.94f, 0.82f);

        var cameraObject = new GameObject("Main Camera");
        cameraObject.tag = "MainCamera";
        cameraObject.transform.SetPositionAndRotation(
            new Vector3(0f, 5f, -12f),
            Quaternion.Euler(8f, 0f, 0f));
        var camera = cameraObject.AddComponent<Camera>();
        camera.depthTextureMode |= DepthTextureMode.Depth;
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.49f, 0.68f, 0.82f);
        camera.nearClipPlane = 0.05f;
        camera.farClipPlane = 12000f;
        cameraObject.AddComponent<PlanarWaterReflection>();
        cameraObject.AddComponent<AudioListener>();
        sandbox.Configure(profile, sea, cameraObject.transform, 20000f);

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
        RenderSettings.sun = sun;

        EditorSceneManager.SaveScene(scene, OceanWaveScenePath);
        AddSceneToBuildSettings(OceanWaveScenePath);
        EditorUtility.SetDirty(profile);
        EditorUtility.SetDirty(sea);
        AssetDatabase.SaveAssets();
        AssetDatabase.Refresh();
        ValidateOceanWaveSandbox();
        Debug.Log($"Created ocean-wave sandbox scene at {OceanWaveScenePath}.");
    }

    public static void ValidateOceanWaveSandbox()
    {
        var scene = EditorSceneManager.OpenScene(
            OceanWaveScenePath,
            OpenSceneMode.Single);
        var sandboxes = UnityEngine.Object.FindObjectsByType<OceanWaveSandboxController>(
            FindObjectsInactive.Include);
        var islands = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include);
        var cameras = UnityEngine.Object.FindObjectsByType<Camera>(
            FindObjectsInactive.Include);
        if (!scene.IsValid()
            || sandboxes.Length != 1
            || islands.Length != 0
            || cameras.Length != 1
            || sandboxes[0].Profile == null
            || sandboxes[0].FollowTarget != cameras[0].transform
            || sandboxes[0].OceanDiameterMetres < 1000f)
        {
            throw new InvalidOperationException(
                "The ocean-wave sandbox must contain one configured sea, one camera, and no island generator.");
        }
    }

    public static void ValidateMultiIslandSandbox()
    {
        var scene = EditorSceneManager.OpenScene(
            MultiIslandScenePath,
            OpenSceneMode.Single);
        var managers = UnityEngine.Object.FindObjectsByType<IslandWorldManager>(
            FindObjectsInactive.Include);
        var requestFactories =
            UnityEngine.Object.FindObjectsByType<GridIslandGenerationRequestFactory>(
                FindObjectsInactive.Include);
        var generators = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include);
        var cameras = UnityEngine.Object.FindObjectsByType<Camera>(
            FindObjectsInactive.Include);
        var demos = UnityEngine.Object.FindObjectsByType<IslandDemoController>(
            FindObjectsInactive.Include);
        if (!scene.IsValid()
            || managers.Length != 1
            || requestFactories.Length != 1
            || generators.Length != 0
            || cameras.Length != 1
            || demos.Length != 1
            || cameras[0].GetComponent<FirstPersonController>() == null
            || cameras[0].GetComponent<OrbitCamera>() == null)
        {
            throw new InvalidOperationException(
                "The multi-island sandbox must contain one manager, one request factory, no pre-placed generators, and one fly-capable camera.");
        }
        var demoState = new SerializedObject(demos[0]);
        if (!demoState.FindProperty("startInFlyMode").boolValue)
        {
            throw new InvalidOperationException(
                "The multi-island sandbox camera must start in fly mode.");
        }
        var managerState = new SerializedObject(managers[0]);
        var environmentSettings = managerState.FindProperty("worldEnvironmentSettings");
        var factoryState = new SerializedObject(requestFactories[0]);
        var variationSettings = factoryState.FindProperty("parameterVariation");
        if (managerState.FindProperty("islandGenerationRequestFactoryComponent")
                .objectReferenceValue != requestFactories[0]
            || environmentSettings == null
            || environmentSettings.FindPropertyRelative("sunlight").objectReferenceValue == null
            || environmentSettings.FindPropertyRelative("seaMaterial").objectReferenceValue == null
            || variationSettings == null
            || !variationSettings.FindPropertyRelative("enabled").boolValue
            || factoryState.FindProperty("defaultConfiguration").objectReferenceValue == null
            || factoryState.FindProperty("fixedIslands").arraySize != 3
            || !factoryState.FindProperty("generateUnlistedCells").boolValue
            || managerState.FindProperty("generationRadiusMetres").floatValue
                >= managerState.FindProperty("discoveryRadiusMetres").floatValue
            || managerState.FindProperty("unloadRadiusMetres").floatValue
                >= managerState.FindProperty("discoveryRadiusMetres").floatValue
            || managerState.FindProperty("maximumLoadedIslandCount").intValue != 3)
        {
            throw new InvalidOperationException(
                "The multi-island sandbox factory must own its fixed and generated cells with correctly ordered generation, unload, and discovery radii.");
        }
    }

    private static IslandConfiguration CreateOrUpdateIslandConfiguration(
        string path,
        int seed,
        Material terrain,
        Material grass,
        Material river,
        Material sea,
        Material rock,
        Material treeWood,
        Material treeFoliage)
    {
        var configuration = AssetDatabase.LoadAssetAtPath<IslandConfiguration>(path);
        if (configuration == null)
        {
            configuration = ScriptableObject.CreateInstance<IslandConfiguration>();
            AssetDatabase.CreateAsset(configuration, path);
        }
        configuration.Generation.Seed = seed;
        configuration.ConfigureRenderingReferences(
            terrain,
            grass,
            river,
            sea,
            rock,
            treeWood,
            treeFoliage);
        EditorUtility.SetDirty(configuration);
        return configuration;
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
        if (shaderName == "Motu/Sea Water")
        {
            material.DisableKeyword("_GEOMETRICWAVES_ON");
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
