using static Motu.Editor.ViewRenderingValidation;
using static Motu.Editor.EnvironmentRenderingValidation;
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
    internal static class IslandSceneValidation
    {
        private const string SandboxScenePath = "Assets/Scenes/IslandRuntimeSandbox.unity";
        internal static void ValidateFactorySettingsContract()
        {
            var factoryObject = new GameObject("Factory settings contract validation");
            try
            {
                var factory = factoryObject.AddComponent<GridIslandGenerationRequestFactory>();
                if (factory.GenerationSettings == null
                    || factory.RiverSettings == null
                    || factory.ForestSettings == null
                    || factory.ReedSettings == null
                    || factory.FernSettings == null
                    || factory.RenderingSettings == null
                    || factory.DebugSettings == null)
                {
                    throw new InvalidOperationException(
                        "A new request factory is missing an island settings group.");
                }
                if (typeof(IslandGenerationRequestFactoryBase).GetProperty("Clouds") != null)
                {
                    throw new InvalidOperationException(
                        "World weather must not be stored in an island request factory.");
                }
                if (typeof(IslandRenderingSettings).GetProperty("GrassPatchNoise") != null)
                {
                    throw new InvalidOperationException(
                        "Ocean and vegetation weather noise must be owned by the global environment.");
                }
                if (typeof(IslandRenderingSettings).GetProperty("TreeWoodMaterial") != null)
                {
                    throw new InvalidOperationException(
                        "Runtime tree bark must not depend on a per-island wood material template.");
                }
                if (typeof(IslandRenderingSettings).GetMethod(
                        "SelectMaterialColours",
                        BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic) != null)
                {
                    throw new InvalidOperationException(
                        "Material palette selection must be owned by the island request factory.");
                }
                if (typeof(IslandGenerationRequestFactoryBase).GetProperty("Streaming") != null)
                {
                    throw new InvalidOperationException(
                        "Scene-specific streaming references must not be stored in an island request factory.");
                }

                var dirt = new Color(0.11f, 0.12f, 0.13f, 1f);
                var stone = new Color(0.21f, 0.22f, 0.23f, 1f);
                var sand = new Color(0.31f, 0.32f, 0.33f, 1f);
                factory.DirtColour = dirt;
                factory.StoneColour = stone;
                factory.SandColour = sand;
                factory.RandomizeMaterialColours = false;
                factory.Configure(731, false, 0f);
                factory.SetFixedIslands(
                    new GridIslandGenerationRequestFactory.FixedIsland(Vector2Int.zero));
                var request = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                if (request == null
                    || request.MaterialColours.Dirt != dirt
                    || request.MaterialColours.Stone != stone
                    || request.MaterialColours.Sand != sand)
                {
                    throw new InvalidOperationException(
                        "The request factory did not put its selected material palette on the request.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(factoryObject);
            }

            var environment = ScriptableObject.CreateInstance<WorldEnvironmentConfiguration>();
            try
            {
                if (environment.Environment == null || environment.Clouds == null)
                {
                    throw new InvalidOperationException(
                        "A new WorldEnvironmentConfiguration is missing sky, ocean, or weather settings.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(environment);
            }
        }
        internal static IslandGenerationRequest RequireSandboxRequest()
        {
            var managers = UnityEngine.Object.FindObjectsByType<IslandWorldManager>(
                FindObjectsInactive.Include);
            var factories =
                UnityEngine.Object.FindObjectsByType<GridIslandGenerationRequestFactory>(
                    FindObjectsInactive.Include);
            if (managers.Length != 1 || factories.Length != 1)
            {
                throw new InvalidOperationException(
                    "The sandbox must contain one world manager and one grid request factory.");
            }
            return factories[0].CreateIslandGenerationRequest(Vector2Int.zero)
                ?? throw new InvalidOperationException(
                    "The sandbox request factory leaves its origin cell as open sea.");
        }
        internal static void ValidateSandboxScene()
        {
            var scene = EditorSceneManager.OpenScene(SandboxScenePath);
            if (!scene.IsValid())
            {
                throw new InvalidOperationException("The island sandbox scene could not be opened.");
            }
            var managers = UnityEngine.Object.FindObjectsByType<IslandWorldManager>(
                FindObjectsInactive.Include);
            var factories =
                UnityEngine.Object.FindObjectsByType<GridIslandGenerationRequestFactory>(
                    FindObjectsInactive.Include);
            var islands = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
                FindObjectsInactive.Include);
            if (managers.Length != 1 || factories.Length != 1 || islands.Length != 0)
            {
                throw new InvalidOperationException(
                    "The sandbox scene must contain one world manager, one request factory, and no pre-placed IslandGenerator.");
            }
            var request = RequireSandboxRequest();
            if (request.DebugSettings.ToggleFrameRateKey == KeyCode.None)
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
            if (!demoControllers[0].ShowMinimap)
            {
                throw new InvalidOperationException(
                    "The sandbox runtime debug HUD does not have its island minimap enabled.");
            }
            var managerState = new SerializedObject(managers[0]);
            if (managerState.FindProperty("streamingTarget").objectReferenceValue == null
                || managerState.FindProperty("islandGenerationRequestFactoryComponent")
                    .objectReferenceValue != factories[0]
                || factories[0].FixedIslandCount != 1
                || factories[0].GeneratesUnlistedCells)
            {
                throw new InvalidOperationException(
                    "The sandbox manager is not configured for its single factory-owned island cell.");
            }
            var sunlight = RenderSettings.sun;
            if (sunlight == null
                || sunlight.type != LightType.Directional
                || sunlight.shadows != LightShadows.Soft)
            {
                throw new InvalidOperationException(
                    "The sandbox directional sunlight does not have soft shadows enabled.");
            }
            if (request.Rendering.SunCycleDurationMinutes <= 0.25f
                || request.Rendering.MidnightToNoonClockRateRatio < 1f
                || Mathf.Abs(request.Rendering.SunLatitudeDegrees) < 0.01f
                || request.Rendering.MiddaySunIntensity <= 0f
                || request.Rendering.MoonEquatorOffsetDegrees <= 0f
                || request.Rendering.FullMoonLightIntensity <= 0f)
            {
                throw new InvalidOperationException(
                    "The sandbox solar or lunar cycle settings are invalid.");
            }
            if (!request.Rendering.ShowDistanceHaze
                || request.Rendering.DistanceHazeDensity <= 0f)
            {
                throw new InvalidOperationException(
                    "The sandbox first-person distance haze is missing or has invalid density.");
            }
            ValidateFirstPersonDistanceHaze(request);
            if (request.Rendering.TerrainMaterial == null
                || request.Rendering.GrassMaterial == null
                || request.Rendering.RiverMaterial == null
                || request.Rendering.SeaMaterial == null
                || request.Rendering.RockMaterial == null)
            {
                throw new InvalidOperationException(
                    "The sandbox IslandGenerator is missing a default material template.");
            }
            if (request.Rendering.RiverMaterial.shader.name != "Motu/River Water"
                || request.Rendering.SeaMaterial.shader.name != "Motu/Sea Water")
            {
                throw new InvalidOperationException(
                    "The sandbox river and sea materials do not use their dedicated shaders.");
            }
            ValidateRealTimeAmbientOcclusion();
            ValidatePlanarWaterReflections();
            var sceneEnabled = EditorBuildSettings.scenes.Any(
                entry => entry.enabled && entry.path == SandboxScenePath);
            if (!sceneEnabled)
            {
                throw new InvalidOperationException(
                    "The island sandbox scene is not enabled in Build Settings.");
            }
        }
    }
}
