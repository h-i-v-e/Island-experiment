using static Motu.Editor.ViewRenderingValidation;
using static Motu.Editor.EnvironmentRenderingValidation;
using static Motu.Editor.OceanRenderingValidation;
using static Motu.Editor.IslandSceneValidation;
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
    public static class IslandGeneratorValidation
    {
        private const string SandboxScenePath =
            "Assets/Scenes/IslandRuntimeSandbox.unity";

        public static void BatchValidateNativeInterop()
        {
            //IslandNativeValidation.BatchValidateInitialSoil();
            IslandNativeValidation.BatchValidateInitialSoilNative();
            IslandNativeValidation.BatchValidateNativeInterop();
            IslandRenderingValidation.ValidateMaterialTextureCacheRoundTrip();
            ValidateFactorySettingsContract();
            ValidateWaterfallMistShader();
            ValidateFernShader();
            ValidateSkyDomeShader();
            ValidateCloudReceiverShaders();
            ValidateSolarLightingCycle();
            ValidateOpenSeaEnvironmentAnchoring();
            ValidateOceanWaveSystem();
            WorldRoutingValidation.ValidateRoutingPolicy();
            IslandProjectSetup.ValidateMultiIslandSandbox();
            ValidateSandboxScene();
            ValidateRealtimeShadowRender();
            Debug.Log("IslandGenerator component, sandbox level, and native validation passed.");
        }

        public static void BatchValidateIslandWorldArchitecture()
        {
            ValidateFactorySettingsContract();
            WorldRoutingValidation.ValidateRoutingPolicy();
            IslandProjectSetup.ValidateMultiIslandSandbox();
            ValidateSandboxScene();
            Debug.Log(
                "Factory-owned island requests, world routing, and replacement scenes passed validation.");
        }

        public static void BatchValidateOceanWaves()
        {
            ValidateOceanWaveSystem();
            IslandProjectSetup.ValidateOceanWaveSandbox();
            Debug.Log("Ocean clipmap, geometric-wave shaders, and sea-only sandbox passed validation.");
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
            var managers = UnityEngine.Object.FindObjectsByType<IslandWorldManager>(
                FindObjectsInactive.Include);
            var environment = managers.Length == 1
                ? managers[0].GlobalEnvironmentSettings
                : null;
            if (environment == null
                || environment.WindDirection.sqrMagnitude < 1.0e-4f
                || environment.WindSpeedMetresPerSecond <= 0f
                || environment.WindGustSizeMetres <= 0f
                || environment.TreeWindFullBendHeightMetres
                    <= environment.TreeWindBasePinHeightMetres)
            {
                throw new InvalidOperationException(
                    "The sandbox global wind or vegetation response settings are missing or disabled.");
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
            var vegetationShaderNames = new[]
            {
                "Motu/Tree Wood",
                "Motu/Tree Foliage",
                "Motu/Tree Foliage Distant",
                "Motu/Riverbank Reeds",
                "Motu/Forest Ferns",
                "Motu/Planar Reflection Simplified",
            };
            foreach (var shaderName in vegetationShaderNames)
            {
                var vegetationShader = Shader.Find(shaderName);
                if (vegetationShader == null
                    || !vegetationShader.isSupported
                    || ShaderUtil.ShaderHasError(vegetationShader))
                {
                    throw new InvalidOperationException(
                        $"Global wind receiver shader '{shaderName}' is missing or unsupported.");
                }
            }
            var material = new Material(shader);
            var terrainMaterial = new Material(terrainShader);
            try
            {
                if (material.HasProperty("_GrassWindStrength")
                    || material.HasProperty("_GrassWindWorldSize")
                    || material.HasProperty("_GrassWindNormalStrength")
                    || terrainMaterial.HasProperty("_GrassWindStrength")
                    || terrainMaterial.HasProperty("_GrassWindWorldSize")
                    || terrainMaterial.HasProperty("_GrassWindNormalStrength")
                    || material.HasProperty("_GrassWindDirection")
                    || material.HasProperty("_GrassWindSpeed")
                    || terrainMaterial.HasProperty("_GrassWindDirection")
                    || terrainMaterial.HasProperty("_GrassWindSpeed"))
                {
                    throw new InvalidOperationException(
                        "Vegetation wind controls must be owned entirely by global weather.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(material);
                UnityEngine.Object.DestroyImmediate(terrainMaterial);
            }
            Debug.Log("Unified global weather wind passed validation.");
        }

    }
}
