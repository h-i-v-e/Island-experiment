using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

public static class TreeProjectSetup
{
    private const string ScenePath = "Assets/Scenes/TreeSandbox.unity";
    private const string WoodMaterialPath = "Assets/Materials/TreeWood.mat";
    private const string FoliageMaterialPath = "Assets/Materials/TreeFoliage.mat";

    [MenuItem("Island/Create or Refresh Tree Sandbox")]
    public static void CreateTreeSandbox()
    {
        EnsureFolder("Assets", "Scenes");
        EnsureFolder("Assets", "Materials");
        var wood = CreateOrUpdateMaterial(
            WoodMaterialPath,
            "Motu/Tree Wood",
            new Color(0.24f, 0.105f, 0.045f, 1f),
            new Color(0.43f, 0.22f, 0.09f, 1f));
        var foliage = CreateOrUpdateMaterial(
            FoliageMaterialPath,
            "Motu/Tree Foliage",
            new Color(0.08f, 0.28f, 0.055f, 1f),
            new Color(0.22f, 0.55f, 0.12f, 1f));

        var scene = EditorSceneManager.NewScene(
            NewSceneSetup.EmptyScene,
            NewSceneMode.Single);
        scene.name = "TreeSandbox";

        var cameraObject = new GameObject("Main Camera");
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.AddComponent<Camera>();
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.30f, 0.37f, 0.42f);
        cameraObject.AddComponent<AudioListener>();
        var orbit = cameraObject.AddComponent<OrbitCamera>();
        cameraObject.AddComponent<TreeMeshView>();

        var sunObject = new GameObject("Sun");
        sunObject.transform.rotation = Quaternion.Euler(48f, -32f, 0f);
        var sun = sunObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.intensity = 1.2f;
        sun.color = new Color(1f, 0.94f, 0.84f);

        var treeObject = new GameObject("Procedural Tree Preview");
        var preview = treeObject.AddComponent<ProceduralTreePreview>();
        preview.Configure(wood, foliage, camera, orbit);
        preview.Regenerate();

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.34f, 0.37f, 0.40f);
        RenderSettings.sun = sun;
        RenderSettings.fog = false;

        EditorSceneManager.SaveScene(scene, ScenePath);
        AddBuildSceneWithoutReplacingExistingScenes(ScenePath);
        EditorUtility.SetDirty(wood);
        EditorUtility.SetDirty(foliage);
        AssetDatabase.SaveAssets();
        AssetDatabase.Refresh();
        Debug.Log($"Created procedural tree sandbox scene at {ScenePath}.");
    }

    public static void ValidateTreeSandbox()
    {
        var scene = EditorSceneManager.OpenScene(ScenePath, OpenSceneMode.Single);
        var previews = scene.GetRootGameObjects()
            .SelectMany(root => root.GetComponentsInChildren<ProceduralTreePreview>(true))
            .ToArray();
        if (previews.Length != 1)
        {
            throw new InvalidOperationException(
                $"Expected one procedural tree preview, found {previews.Length}.");
        }

        previews[0].Regenerate();
        var wood = previews[0].transform.Find("Wood");
        var filter = wood != null ? wood.GetComponent<MeshFilter>() : null;
        if (filter == null
            || filter.sharedMesh == null
            || filter.sharedMesh.vertexCount == 0
            || filter.sharedMesh.triangles.Length == 0)
        {
            throw new InvalidOperationException("The tree sandbox has no generated wood mesh.");
        }
        ValidateTreeSurfaceMaterial(wood, "wood");
        var foliage = previews[0].transform.Find("Foliage");
        var foliageFilter = foliage != null ? foliage.GetComponent<MeshFilter>() : null;
        if (foliageFilter == null
            || foliageFilter.sharedMesh == null
            || foliageFilter.sharedMesh.vertexCount == 0
            || foliageFilter.sharedMesh.triangles.Length == 0)
        {
            throw new InvalidOperationException("The tree sandbox has no generated foliage mesh.");
        }
        ValidateTreeSurfaceMaterial(foliage, "foliage");

        var sun = scene.GetRootGameObjects()
            .SelectMany(root => root.GetComponentsInChildren<Light>(true))
            .SingleOrDefault(light => light.type == LightType.Directional);
        if (sun == null || sun.shadows != LightShadows.Soft)
        {
            throw new InvalidOperationException(
                "The tree sandbox directional light does not have soft shadows enabled.");
        }

        var orbit = scene.GetRootGameObjects()
            .SelectMany(root => root.GetComponentsInChildren<OrbitCamera>(true))
            .Single();
        var initialRotation = orbit.transform.rotation;
        orbit.OrbitByDegrees(20f);
        if (Quaternion.Angle(initialRotation, orbit.transform.rotation) < 19f)
        {
            throw new InvalidOperationException("The tree orbit control did not rotate the camera.");
        }
        orbit.ResetOrientation();

        var meshView = orbit.GetComponent<TreeMeshView>();
        if (meshView == null || meshView.IsVisible)
        {
            throw new InvalidOperationException("The tree mesh-view control was not initialized.");
        }
        meshView.Toggle();
        if (!meshView.IsVisible)
        {
            throw new InvalidOperationException("The tree mesh-view control did not toggle on.");
        }
        meshView.Toggle();

        var lod0WoodVertices = filter.sharedMesh.vertexCount;
        var lod0FoliageVertices = foliageFilter.sharedMesh.vertexCount;
        previews[0].ToggleLod();
        if (!previews[0].IsLod1Visible
            || filter.sharedMesh.vertexCount >= lod0WoodVertices
            || foliageFilter.sharedMesh.vertexCount >= lod0FoliageVertices)
        {
            throw new InvalidOperationException("The tree LOD control did not display LOD1.");
        }
        previews[0].ToggleLod();
        if (previews[0].IsLod1Visible
            || filter.sharedMesh.vertexCount != lod0WoodVertices
            || foliageFilter.sharedMesh.vertexCount != lod0FoliageVertices)
        {
            throw new InvalidOperationException("The tree LOD control did not restore LOD0.");
        }

        var initialSeed = previews[0].Seed;
        previews[0].GenerateNextSeed();
        if (previews[0].Seed != unchecked(initialSeed + 1))
        {
            throw new InvalidOperationException("The new-tree control did not advance the seed.");
        }

        Debug.Log(
            $"Validated procedural tree sandbox: wood {filter.sharedMesh.vertexCount} vertices/"
            + $"{filter.sharedMesh.triangles.Length / 3} triangles, foliage "
            + $"{foliageFilter.sharedMesh.vertexCount} vertices/"
            + $"{foliageFilter.sharedMesh.triangles.Length / 3} triangles, orbit, mesh view, LOD "
            + "and regenerate controls active.");
    }

    private static void ValidateTreeSurfaceMaterial(Transform meshTransform, string label)
    {
        var renderer = meshTransform != null ? meshTransform.GetComponent<MeshRenderer>() : null;
        var material = renderer != null ? renderer.sharedMaterial : null;
        if (material == null
            || !material.HasProperty("_CliffNoise3D")
            || !material.HasProperty("_TreeNoisePeriod")
            || !material.HasProperty("_TreeNoiseDetailScale")
            || !material.HasProperty("_TreeNoiseFineScale")
            || !material.HasProperty("_TreeNormalStrength")
            || !material.HasProperty("_TreeHueVariationDegrees")
            || (label == "foliage"
                && (!material.HasProperty("_CanopyCoverage")
                    || !material.HasProperty("_CanopyEdgeSoftness")
                    || !material.HasProperty("_AlphaCutoff")
                    || !material.HasProperty("_FoliageFurHeight")
                    || !material.HasProperty("_FoliageLeafWorldSize")
                    || !material.HasProperty("_FoliageLeafCoverage")
                    || !material.HasProperty("_FoliageLeafEdgeSoftness")
                    || !material.HasProperty("_GrassPlayerPosition")
                    || !material.HasProperty("_GrassRadius")
                    || !material.HasProperty("_GrassFadeWidth")))
            || material.GetTexture("_CliffNoise3D") == null)
        {
            throw new InvalidOperationException(
                $"The tree sandbox {label} material is missing layered surface noise.");
        }
        if (label == "foliage" && material.passCount != 10)
        {
            throw new InvalidOperationException(
                "The tree sandbox foliage material does not have exactly eight fur passes.");
        }
        if (label == "foliage"
            && (material.renderQueue != (int)RenderQueue.AlphaTest
                || material.FindPass("ShadowCaster") < 0))
        {
            throw new InvalidOperationException(
                "The tree sandbox foliage material has no alpha-tested shadow caster.");
        }
    }

    private static Material CreateOrUpdateMaterial(
        string path,
        string shaderName,
        Color baseColor,
        Color lightColor)
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
        material.SetColor("_BaseColor", baseColor);
        material.SetColor("_LightColor", lightColor);
        return material;
    }

    private static void AddBuildSceneWithoutReplacingExistingScenes(string path)
    {
        var scenes = EditorBuildSettings.scenes.ToList();
        if (scenes.All(scene => scene.path != path))
        {
            scenes.Add(new EditorBuildSettingsScene(path, true));
            EditorBuildSettings.scenes = scenes.ToArray();
        }
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
