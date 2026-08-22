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
    private const string MaterialFolder = "Assets/Materials";

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
            rock);

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
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
