using System;
using System.Linq;
using System.Reflection;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

public static class IslandGeneratorValidation
{
    private const string SandboxScenePath = "Assets/Scenes/IslandSandbox.unity";

    public static void BatchValidateNativeInterop()
    {
        IslandGenerator.BatchValidateNativeInterop();
        ValidateSandboxScene();
        ValidateRealtimeShadowRender();
        Debug.Log("IslandGenerator component, sandbox level, and native validation passed.");
    }

    public static void BatchValidateRealtimeShadows()
    {
        var scene = EditorSceneManager.OpenScene(SandboxScenePath);
        if (!scene.IsValid())
        {
            throw new InvalidOperationException("The island sandbox scene could not be opened.");
        }
        ValidateRealtimeShadowRender();
        Debug.Log("Tree and grass real-time shadow shader variants passed validation.");
    }

    public static void BatchValidateRealTimeAmbientOcclusion()
    {
        var scene = EditorSceneManager.OpenScene(SandboxScenePath);
        if (!scene.IsValid())
        {
            throw new InvalidOperationException("The island sandbox scene could not be opened.");
        }
        ValidateRealTimeAmbientOcclusion();
        Debug.Log("Real-time ambient occlusion shader and camera configuration passed.");
    }

    public static void BatchValidatePlanarWaterReflections()
    {
        var scene = EditorSceneManager.OpenScene(SandboxScenePath);
        if (!scene.IsValid())
        {
            throw new InvalidOperationException("The island sandbox scene could not be opened.");
        }
        ValidatePlanarWaterReflections();
        ValidatePlanarWaterReflectionRender();
        Debug.Log("Planar water reflection camera and shader configuration passed.");
    }

    public static void BatchValidateGrassWind()
    {
        var scene = EditorSceneManager.OpenScene(SandboxScenePath);
        if (!scene.IsValid())
        {
            throw new InvalidOperationException("The island sandbox scene could not be opened.");
        }
        var islands = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include);
        if (islands.Length != 1
            || islands[0].Rendering.GrassWindDirection.sqrMagnitude < 1.0e-4f
            || islands[0].Rendering.GrassWindStrengthMetres <= 0f
            || islands[0].Rendering.GrassWindSpeedMetresPerSecond <= 0f
            || islands[0].Rendering.GrassWindGustSizeMetres <= 0f
            || islands[0].Rendering.GrassWindNormalStrength <= 0f)
        {
            throw new InvalidOperationException(
                "The sandbox grass wind settings are missing or disabled.");
        }

        var shader = Shader.Find("Motu/Terrain Grass");
        var terrainShader = Shader.Find("Motu/Terrain Unified");
        if (shader == null
            || terrainShader == null
            || !shader.isSupported
            || !terrainShader.isSupported
            || ShaderUtil.ShaderHasError(shader)
            || ShaderUtil.ShaderHasError(terrainShader))
        {
            throw new InvalidOperationException(
                "A wind-enabled grass or terrain shader is missing or unsupported.");
        }
        var material = new Material(shader);
        var terrainMaterial = new Material(terrainShader);
        try
        {
            if (!material.HasProperty("_GrassWindDirection")
                || !material.HasProperty("_GrassWindStrength")
                || !material.HasProperty("_GrassWindSpeed")
                || !material.HasProperty("_GrassWindWorldSize")
                || !material.HasProperty("_GrassWindNormalStrength")
                || !terrainMaterial.HasProperty("_GrassWindDirection")
                || !terrainMaterial.HasProperty("_GrassWindStrength")
                || !terrainMaterial.HasProperty("_GrassWindSpeed")
                || !terrainMaterial.HasProperty("_GrassWindWorldSize")
                || !terrainMaterial.HasProperty("_GrassWindNormalStrength"))
            {
                throw new InvalidOperationException(
                    "The near or far grass shader is missing its wind controls.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(material);
            UnityEngine.Object.DestroyImmediate(terrainMaterial);
        }
        Debug.Log("Near and far grass wind-normal controls passed validation.");
    }

    private static void ValidateSandboxScene()
    {
        var scene = EditorSceneManager.OpenScene(SandboxScenePath);
        if (!scene.IsValid())
        {
            throw new InvalidOperationException("The island sandbox scene could not be opened.");
        }
        var islands = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include);
        if (islands.Length != 1)
        {
            throw new InvalidOperationException(
                $"The sandbox scene must contain exactly one IslandGenerator; found {islands.Length}.");
        }
        var island = islands[0];
        if (island.DebugSettings.ToggleFrameRateKey == KeyCode.None)
        {
            throw new InvalidOperationException(
                "The sandbox frame-rate display has no Play Mode toggle key.");
        }
        var demoControllers = UnityEngine.Object.FindObjectsByType<IslandDemoController>(
            FindObjectsInactive.Include);
        if (demoControllers.Length != 1)
        {
            throw new InvalidOperationException(
                $"The sandbox scene must contain one runtime debug HUD; found {demoControllers.Length}.");
        }
        if (island.Streaming.Target == null)
        {
            throw new InvalidOperationException(
                "The sandbox IslandGenerator has no streaming target.");
        }
        if (island.Rendering.Sunlight == null
            || island.Rendering.Sunlight.type != LightType.Directional
            || island.Rendering.Sunlight.shadows != LightShadows.Soft)
        {
            throw new InvalidOperationException(
                "The sandbox directional sunlight does not have soft shadows enabled.");
        }
        if (island.Rendering.TerrainMaterial == null
            || island.Rendering.GrassMaterial == null
            || island.Rendering.RiverMaterial == null
            || island.Rendering.SeaMaterial == null
            || island.Rendering.RockMaterial == null
            || island.Rendering.TreeWoodMaterial == null)
        {
            throw new InvalidOperationException(
                "The sandbox IslandGenerator is missing a default material template.");
        }
        if (island.Rendering.RiverMaterial.shader.name != "Motu/River Water"
            || island.Rendering.SeaMaterial.shader.name != "Motu/Sea Water")
        {
            throw new InvalidOperationException(
                "The sandbox river and sea materials do not use their dedicated shaders.");
        }
        var treeWood = island.Rendering.TreeWoodMaterial;
        if (treeWood.shader.name != "Motu/Tree Wood"
            || treeWood.GetTexture("_BarkAlbedoMap") == null
            || treeWood.GetTexture("_BarkHeightMap") == null
            || treeWood.GetTexture("_BarkNormalMap") == null
            || treeWood.GetTexture("_BarkOcclusionMap") == null)
        {
            throw new InvalidOperationException(
                "The sandbox tree wood template is missing its authored Bark recipe maps.");
        }
        ValidateRealTimeAmbientOcclusion();
        ValidatePlanarWaterReflections();
        if (island.Decorations.TreePrefabs == null
            || island.Decorations.PlantPrefabs == null)
        {
            throw new InvalidOperationException(
                "The sandbox decoration asset libraries are not serialized.");
        }
        var sceneEnabled = EditorBuildSettings.scenes.Any(
            entry => entry.enabled && entry.path == SandboxScenePath);
        if (!sceneEnabled)
        {
            throw new InvalidOperationException(
                "The island sandbox scene is not enabled in Build Settings.");
        }

        var originalPosition = island.transform.position;
        var originalRotation = island.transform.rotation;
        island.transform.SetPositionAndRotation(
            new Vector3(120f, 15f, -80f),
            Quaternion.Euler(0f, 37f, 0f));
        var local = new Vector3(31f, 7f, -19f);
        var roundTrip = island.transform.InverseTransformPoint(
            island.transform.TransformPoint(local));
        island.transform.SetPositionAndRotation(originalPosition, originalRotation);
        if ((roundTrip - local).sqrMagnitude > 1.0e-6f)
        {
            throw new InvalidOperationException(
                "Island local/world transform conversion failed validation.");
        }
    }

    private static void ValidateRealTimeAmbientOcclusion()
    {
        var ambientOcclusion = UnityEngine.Object.FindObjectsByType<RealTimeAmbientOcclusion>(
            FindObjectsInactive.Include);
        if (ambientOcclusion.Length != 1
            || !ambientOcclusion[0].enabled
            || ambientOcclusion[0].GetComponent<Camera>() == null)
        {
            throw new InvalidOperationException(
                "The sandbox camera must have one enabled real-time ambient occlusion effect.");
        }
        var ambientOcclusionShader = Shader.Find(RealTimeAmbientOcclusion.ShaderName);
        if (ambientOcclusionShader == null
            || !ambientOcclusionShader.isSupported
            || ShaderUtil.ShaderHasError(ambientOcclusionShader))
        {
            throw new InvalidOperationException(
                "The real-time ambient occlusion shader is missing or unsupported.");
        }
    }

    private static void ValidatePlanarWaterReflections()
    {
        var reflections = UnityEngine.Object.FindObjectsByType<PlanarWaterReflection>(
            FindObjectsInactive.Include);
        var islands = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include);
        if (reflections.Length != 1
            || !reflections[0].enabled
            || reflections[0].GetComponent<Camera>() == null
            || islands.Length != 1
            || reflections[0].ReflectionPlane != islands[0].transform)
        {
            throw new InvalidOperationException(
                "The sandbox camera must have one enabled planar reflection component linked to the island plane.");
        }
        if (LayerMask.NameToLayer("Water") < 0)
        {
            throw new InvalidOperationException(
                "The Water layer is required to keep water out of its own reflection pass.");
        }

        var riverShader = Shader.Find("Motu/River Water");
        var seaShader = Shader.Find("Motu/Sea Water");
        if (riverShader == null
            || seaShader == null
            || !riverShader.isSupported
            || !seaShader.isSupported
            || ShaderUtil.ShaderHasError(riverShader)
            || ShaderUtil.ShaderHasError(seaShader))
        {
            throw new InvalidOperationException(
                "The planar-reflection water shaders are missing or unsupported.");
        }

        var riverMaterial = new Material(riverShader);
        var seaMaterial = new Material(seaShader);
        try
        {
            if (!riverMaterial.HasProperty("_PlanarReflectionWeight")
                || !riverMaterial.HasProperty("_PlanarReflectionDistortion")
                || !seaMaterial.HasProperty("_PlanarReflectionWeight")
                || !seaMaterial.HasProperty("_PlanarReflectionDistortion"))
            {
                throw new InvalidOperationException(
                    "The water shaders do not expose planar-reflection controls.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(riverMaterial);
            UnityEngine.Object.DestroyImmediate(seaMaterial);
        }
    }

    private static void ValidatePlanarWaterReflectionRender()
    {
        var planeObject = new GameObject("Planar reflection validation plane");
        var cameraObject = new GameObject("Planar reflection validation camera");
        var sourceTarget = new RenderTexture(320, 180, 24);
        try
        {
            planeObject.transform.position = new Vector3(2f, 3f, -4f);
            cameraObject.transform.SetPositionAndRotation(
                new Vector3(10f, 12f, 20f),
                Quaternion.Euler(18f, 205f, 0f));
            var sourceCamera = cameraObject.AddComponent<Camera>();
            sourceCamera.enabled = false;
            sourceCamera.allowHDR = true;
            sourceCamera.targetTexture = sourceTarget;
            var reflection = cameraObject.AddComponent<PlanarWaterReflection>();
            reflection.Configure(planeObject.transform);

            sourceTarget.Create();
            InvokePrivateLifecycleMethod(reflection, "OnPreCull");

            var reflectedCamera = reflection.ReflectionCamera;
            var expectedPosition = new Vector3(10f, -6f, 20f);
            var waterLayer = LayerMask.NameToLayer("Water");
            var reflectedTarget = reflectedCamera != null
                ? reflectedCamera.targetTexture
                : null;
            var reflectedPosition = reflectedCamera != null
                ? reflectedCamera.transform.position
                : Vector3.zero;
            var reflectionAvailable = Shader.GetGlobalFloat(
                "_PlanarReflectionAvailable");
            var reflectionTexture = Shader.GetGlobalTexture(
                "_PlanarReflectionTexture");
            if (reflectedCamera == null
                || (reflectedPosition - expectedPosition).sqrMagnitude > 1.0e-5f
                || reflectedTarget == null
                || reflectedTarget.width != 160
                || reflectedTarget.height != 90
                || (reflectedCamera.cullingMask & (1 << waterLayer)) != 0
                || reflectionAvailable < 0.5f
                || reflectionTexture != reflectedTarget)
            {
                throw new InvalidOperationException(
                    "The planar reflection camera did not render the expected mirrored view. "
                    + $"camera={reflectedCamera != null}, "
                    + $"position={reflectedPosition}, "
                    + $"target={reflectedTarget}, "
                    + $"available={reflectionAvailable}, "
                    + $"texture={reflectionTexture}.");
            }

            reflection.enabled = false;
            InvokePrivateLifecycleMethod(reflection, "OnDisable");
            if (Shader.GetGlobalFloat("_PlanarReflectionAvailable") != 0f)
            {
                throw new InvalidOperationException(
                    "Disabling planar reflections did not restore the shader fallback.");
            }
        }
        finally
        {
            Shader.SetGlobalFloat("_PlanarReflectionAvailable", 0f);
            sourceTarget.Release();
            UnityEngine.Object.DestroyImmediate(sourceTarget);
            UnityEngine.Object.DestroyImmediate(cameraObject);
            UnityEngine.Object.DestroyImmediate(planeObject);
        }
    }

    private static void ValidateRealtimeShadowRender()
    {
        var woodShader = Shader.Find("Motu/Tree Wood");
        var foliageShader = Shader.Find("Motu/Tree Foliage");
        var grassShader = Shader.Find("Motu/Terrain Grass");
        if (woodShader == null || foliageShader == null || grassShader == null)
        {
            throw new InvalidOperationException(
                "A tree or grass shader required for shadow validation is missing.");
        }

        var root = new GameObject("Real-time shadow shader validation");
        var cameraObject = new GameObject("Real-time shadow validation camera");
        var lightObject = new GameObject("Real-time shadow validation light");
        var target = new RenderTexture(128, 128, 24);
        var materials = new[]
        {
            new Material(woodShader),
            new Material(foliageShader),
            new Material(grassShader),
        };
        if (materials[2].renderQueue != (int)UnityEngine.Rendering.RenderQueue.AlphaTest)
        {
            throw new InvalidOperationException(
                "The grass shader must be alpha-tested so its fur shells receive shadows.");
        }
        var originalShadows = QualitySettings.shadows;
        var originalShadowDistance = QualitySettings.shadowDistance;
        try
        {
            QualitySettings.shadows = ShadowQuality.All;
            QualitySettings.shadowDistance = 50f;

            var light = lightObject.AddComponent<Light>();
            light.type = LightType.Directional;
            light.shadows = LightShadows.Soft;
            lightObject.transform.rotation = Quaternion.Euler(50f, -35f, 0f);

            var camera = cameraObject.AddComponent<Camera>();
            camera.enabled = false;
            camera.renderingPath = RenderingPath.Forward;
            camera.depthTextureMode = DepthTextureMode.Depth;
            camera.targetTexture = target;
            cameraObject.transform.position = new Vector3(0f, 3f, -8f);
            cameraObject.transform.LookAt(new Vector3(0f, 1f, 0f));

            CreateShadowValidationPrimitive(
                PrimitiveType.Cube,
                "Wood",
                new Vector3(-2f, 1f, 0f),
                materials[0],
                root.transform);
            CreateShadowValidationPrimitive(
                PrimitiveType.Sphere,
                "Foliage",
                new Vector3(0f, 1f, 0f),
                materials[1],
                root.transform);
            materials[2].SetFloat("_GrassEnabled", 1f);
            materials[2].SetVector("_GrassPlayerPosition", Vector4.zero);
            materials[2].SetFloat("_GrassRadius", 50f);
            CreateShadowValidationPrimitive(
                PrimitiveType.Plane,
                "Grass",
                new Vector3(2f, 0f, 0f),
                materials[2],
                root.transform);

            var shadowVariants = new ShaderVariantCollection();
            foreach (var shader in new[] { woodShader, foliageShader, grassShader })
            {
                shadowVariants.Add(new ShaderVariantCollection.ShaderVariant(
                    shader,
                    UnityEngine.Rendering.PassType.ForwardBase,
                    "DIRECTIONAL",
                    "SHADOWS_SCREEN"));
            }
            shadowVariants.WarmUp();

            target.Create();
            camera.Render();

            if (ShaderUtil.ShaderHasError(woodShader)
                || ShaderUtil.ShaderHasError(foliageShader)
                || ShaderUtil.ShaderHasError(grassShader))
            {
                throw new InvalidOperationException(
                    "A tree or grass shader failed to compile with real-time shadows enabled.");
            }
        }
        finally
        {
            QualitySettings.shadows = originalShadows;
            QualitySettings.shadowDistance = originalShadowDistance;
            target.Release();
            UnityEngine.Object.DestroyImmediate(target);
            foreach (var material in materials)
            {
                UnityEngine.Object.DestroyImmediate(material);
            }
            UnityEngine.Object.DestroyImmediate(lightObject);
            UnityEngine.Object.DestroyImmediate(cameraObject);
            UnityEngine.Object.DestroyImmediate(root);
        }
    }

    private static void CreateShadowValidationPrimitive(
        PrimitiveType primitiveType,
        string name,
        Vector3 position,
        Material material,
        Transform parent)
    {
        var primitive = GameObject.CreatePrimitive(primitiveType);
        primitive.name = name;
        primitive.transform.SetParent(parent, false);
        primitive.transform.position = position;
        var renderer = primitive.GetComponent<Renderer>();
        renderer.sharedMaterial = material;
        renderer.receiveShadows = true;
    }

    private static void InvokePrivateLifecycleMethod(
        PlanarWaterReflection reflection,
        string methodName)
    {
        var method = typeof(PlanarWaterReflection).GetMethod(
            methodName,
            BindingFlags.Instance | BindingFlags.NonPublic);
        if (method == null)
        {
            throw new InvalidOperationException(
                $"PlanarWaterReflection.{methodName} was not found.");
        }
        method.Invoke(reflection, null);
    }
}
