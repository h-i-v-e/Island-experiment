using System;
using System.Linq;
using System.Reflection;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Motu.Gameplay;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;

namespace Motu.Editor
{
    internal static class ViewRenderingValidation
    {
        private const string SandboxScenePath = "Assets/Scenes/IslandRuntimeSandbox.unity";
        internal static void ValidateRealTimeAmbientOcclusion()
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
        internal static void ValidatePlanarWaterReflections()
        {
            var reflections = UnityEngine.Object.FindObjectsByType<PlanarWaterReflection>(
                FindObjectsInactive.Include);
            var managers = UnityEngine.Object.FindObjectsByType<IslandWorldManager>(
                FindObjectsInactive.Include);
            if (reflections.Length != 1
                || !reflections[0].enabled
                || reflections[0].GetComponent<Camera>() == null
                || !reflections[0].UseSimplifiedShader
                || reflections[0].SimplifiedReflectionShader == null
                || managers.Length != 1
                || reflections[0].ReflectionPlane != managers[0].transform)
            {
                throw new InvalidOperationException(
                    "The sandbox camera must have one enabled planar reflection component linked to the world sea plane.");
            }
            if (LayerMask.NameToLayer("Water") < 0)
            {
                throw new InvalidOperationException(
                    "The Water layer is required to keep water out of its own reflection pass.");
            }

            var riverShader = Shader.Find("Motu/River Water");
            var seaShader = Shader.Find("Motu/Sea Water");
            var simplifiedReflectionShader = Shader.Find(
                PlanarWaterReflection.SimplifiedShaderName);
            if (riverShader == null
                || seaShader == null
                || simplifiedReflectionShader == null
                || !riverShader.isSupported
                || !seaShader.isSupported
                || !simplifiedReflectionShader.isSupported
                || ShaderUtil.ShaderHasError(riverShader)
                || ShaderUtil.ShaderHasError(seaShader)
                || ShaderUtil.ShaderHasError(simplifiedReflectionShader))
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

            foreach (var (shaderName, reflectionType) in new[]
            {
                ("Motu/Terrain Unified", "Terrain"),
                ("Motu/Terrain Grass", "Grass"),
                ("Motu/Tree Wood", "Wood"),
                ("Motu/Tree Foliage", "Foliage"),
                ("Motu/Tree Foliage Distant", "Foliage"),
                ("Motu/Rock Decoration", "Rock"),
            })
            {
                var sourceShader = Shader.Find(shaderName);
                var sourceMaterial = sourceShader != null
                    ? new Material(sourceShader)
                    : null;
                try
                {
                    if (sourceMaterial == null
                        || sourceMaterial.GetTag(
                            PlanarWaterReflection.ReplacementTag,
                            false,
                            string.Empty) != reflectionType)
                    {
                        throw new InvalidOperationException(
                            $"Shader '{shaderName}' is not classified as the '{reflectionType}' simplified reflection type.");
                    }
                }
                finally
                {
                    if (sourceMaterial != null)
                    {
                        UnityEngine.Object.DestroyImmediate(sourceMaterial);
                    }
                }
            }
        }
        internal static void ValidatePlanarWaterReflectionRender()
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
                reflection.PrepareReflection();

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
                var reflectionViewerPosition = Shader.GetGlobalVector(
                    PlanarWaterReflection.ViewerPositionName);
                if (reflectedCamera == null
                    || (reflectedPosition - expectedPosition).sqrMagnitude > 1.0e-5f
                    || reflectedTarget == null
                    || reflectedTarget.width != 160
                    || reflectedTarget.height != 90
                    || (sourceCamera.depthTextureMode & DepthTextureMode.Depth) == 0
                    || (reflectedCamera.cullingMask & (1 << waterLayer)) != 0
                    || reflectedCamera.depthTextureMode != DepthTextureMode.None
                    || !reflection.LastRenderUsedSimplifiedShader
                    || reflection.FrameInterval != 2
                    || reflection.ReflectionRenderCount != 1
                    || reflectionAvailable < 0.5f
                    || reflectionTexture != reflectedTarget
                    || (new Vector3(
                            reflectionViewerPosition.x,
                            reflectionViewerPosition.y,
                            reflectionViewerPosition.z)
                        - sourceCamera.transform.position).sqrMagnitude > 1.0e-5f)
                {
                    throw new InvalidOperationException(
                        "The planar reflection camera did not render the expected mirrored view. "
                        + $"camera={reflectedCamera != null}, "
                        + $"position={reflectedPosition}, "
                        + $"target={reflectedTarget}, "
                        + $"available={reflectionAvailable}, "
                        + $"texture={reflectionTexture}.");
                }

                reflection.PrepareReflection();
                if (reflection.ReflectionRenderCount != 1)
                {
                    throw new InvalidOperationException(
                        "The planar reflection did not reuse its render on the skipped frame.");
                }
                reflection.PrepareReflection();
                if (reflection.ReflectionRenderCount != 2)
                {
                    throw new InvalidOperationException(
                        "The planar reflection did not render again at its configured interval.");
                }

                reflection.enabled = false;
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
        internal static void ValidateRealtimeShadowRender()
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
        internal static void CreateShadowValidationPrimitive(
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
    }
}
