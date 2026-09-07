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
    internal static class EnvironmentRenderingValidation
    {
        private const string SandboxScenePath = "Assets/Scenes/IslandRuntimeSandbox.unity";
        internal static void ValidateFernShader()
        {
            var shader = Shader.Find("Motu/Forest Ferns");
            if (shader == null
                || !shader.isSupported
                || ShaderUtil.ShaderHasError(shader))
            {
                throw new InvalidOperationException(
                    "The forest-fern cutout shader is missing or invalid.");
            }
            var material = new Material(shader);
            try
            {
                if (material.GetTag("RenderType", false) != "MotuFernCutout"
                    || material.GetTag("MotuReflection", false) != "Ferns"
                    || material.HasProperty("_FernWindMultiplier")
                    || material.HasProperty("_GrassPatchNoise"))
                {
                    throw new InvalidOperationException(
                        "The forest-fern shader has an invalid reflection or global-wind contract.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(material);
            }
        }
        internal static void ValidateWaterfallMistShader()
        {
            var mistShader = Shader.Find("Motu/Waterfall Foot Mist");
            var sprayShader = Shader.Find("Motu/Waterfall Spray Particle");
            if (mistShader == null
                || sprayShader == null
                || !mistShader.isSupported
                || !sprayShader.isSupported
                || ShaderUtil.ShaderHasError(mistShader)
                || ShaderUtil.ShaderHasError(sprayShader))
            {
                throw new InvalidOperationException(
                    "A waterfall-foot mist or impact-spray shader is missing or invalid.");
            }
        }
        internal static void ValidateSkyDomeShader()
        {
            var shader = Shader.Find("Motu/Sky Dome");
            if (shader == null
                || !shader.isSupported
                || ShaderUtil.ShaderHasError(shader))
            {
                throw new InvalidOperationException(
                    "The generated sky-dome shader is missing or invalid.");
            }
            var material = new Material(shader);
            try
            {
                if (!material.HasProperty("_HorizonColor")
                    || !material.HasProperty("_ZenithColor")
                    || !material.HasProperty("_SunDirection")
                    || !material.HasProperty("_SunColor")
                    || !material.HasProperty("_SunDiscCosRadius")
                    || !material.HasProperty("_SunVisibility")
                    || !material.HasProperty("_SunHaloColor")
                    || !material.HasProperty("_SunHaloStrength")
                    || !material.HasProperty("_MoonDirection")
                    || !material.HasProperty("_MoonLightDirection")
                    || !material.HasProperty("_MoonColor")
                    || !material.HasProperty("_MoonDarkColor")
                    || !material.HasProperty("_MoonDiscCosRadius")
                    || !material.HasProperty("_MoonVisibility")
                    || !material.HasProperty("_SkyExposure")
                    || !material.HasProperty("_StarSettings")
                    || !material.HasProperty("_StarVisibility")
                    || !material.HasProperty("_StarRotation"))
                {
                    throw new InvalidOperationException(
                        "The sky-dome shader is missing its haze, celestial, or star contract.");
                }

                var settings = new IslandRenderingSettings
                {
                    StarDensity = 2f,
                    StarBrightness = -1f,
                    StarSize = 1f,
                };
                if (!Mathf.Approximately(settings.StarDensity, 1f)
                    || !Mathf.Approximately(settings.StarBrightness, 0f)
                    || !Mathf.Approximately(settings.StarSize, 0.12f))
                {
                    throw new InvalidOperationException(
                        "The procedural star settings are not clamping their live values.");
                }
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(material);
            }
        }
        internal static void ValidateCloudReceiverShaders()
        {
            var shaderNames = new[]
            {
                "Motu/Sky Dome",
                "Motu/Terrain Unified",
                "Motu/Terrain Grass",
                "Motu/Tree Wood",
                "Motu/Tree Foliage",
                "Motu/Tree Foliage Distant",
                "Motu/Rock Decoration",
                "Motu/Riverbank Reeds",
                "Motu/Forest Ferns",
                "Motu/River Water",
                "Motu/Sea Water",
                "Motu/Planar Reflection Simplified",
            };
            foreach (var shaderName in shaderNames)
            {
                var shader = Shader.Find(shaderName);
                if (shader == null
                    || !shader.isSupported
                    || ShaderUtil.ShaderHasError(shader))
                {
                    throw new InvalidOperationException(
                        $"Cloud receiver shader '{shaderName}' is missing or invalid.");
                }
            }

            var settings = new IslandCloudSettings
            {
                WeatherMapResolution = 70,
                Coverage = 2f,
                Density = -1f,
                VerticalThicknessMetres = 2000f,
                BroadNoiseScale = 100f,
                BroadNoiseStrength = 2f,
            };
            if (settings.WeatherMapResolution != 64
                || !Mathf.Approximately(settings.Coverage, 1f)
                || !Mathf.Approximately(settings.Density, 0f)
                || !Mathf.Approximately(settings.VerticalThicknessMetres, 1000f)
                || !Mathf.Approximately(settings.BroadNoiseScale, 16f)
                || !Mathf.Approximately(settings.BroadNoiseStrength, 1f))
            {
                throw new InvalidOperationException(
                    "Cloud runtime settings are not clamping their live values.");
            }
        }
        internal static void ValidateSolarLightingCycle()
        {
            const float midnightToNoonRateRatio = 10f;
            var noonClockRate = CelestialLighting.EvaluateClockRateMultiplier(
                12f,
                midnightToNoonRateRatio);
            var midnightClockRate = CelestialLighting.EvaluateClockRateMultiplier(
                0f,
                midnightToNoonRateRatio);
            var sunriseClockRate = CelestialLighting.EvaluateClockRateMultiplier(
                6f,
                midnightToNoonRateRatio);
            var sunsetClockRate = CelestialLighting.EvaluateClockRateMultiplier(
                18f,
                midnightToNoonRateRatio);
            var inverseRateIntegral = 0f;
            const int clockSamples = 4096;
            for (var sample = 0; sample < clockSamples; sample++)
            {
                inverseRateIntegral += 1f / CelestialLighting.EvaluateClockRateMultiplier(
                    24f * sample / clockSamples,
                    midnightToNoonRateRatio);
            }
            inverseRateIntegral /= clockSamples;
            if (!Mathf.Approximately(midnightClockRate / noonClockRate, 10f)
                || midnightClockRate <= sunriseClockRate
                || sunriseClockRate <= noonClockRate
                || !Mathf.Approximately(sunriseClockRate, sunsetClockRate)
                || Mathf.Abs(inverseRateIntegral - 1f) > 0.001f)
            {
                throw new InvalidOperationException(
                    "The solar clock does not slow at noon, accelerate tenfold at midnight, or preserve its configured period.");
            }
            if (!Mathf.Approximately(
                CelestialLighting.EvaluateClockRateMultiplier(3f, 1f),
                1f))
            {
                throw new InvalidOperationException(
                    "A one-to-one solar clock rate must remain uniform.");
            }
            var sunrise = CelestialLighting.EvaluateSun(6f, 45f, 1.25f);
            var noon = CelestialLighting.EvaluateSun(12f, 45f, 1.25f);
            var sunset = CelestialLighting.EvaluateSun(18f, 45f, 1.25f);
            var midnight = CelestialLighting.EvaluateSun(0f, 45f, 1.25f);
            var newMoon = CelestialLighting.EvaluateMoon(
                12f,
                45f,
                22f,
                0f,
                0.14f,
                noon.LocalDirection.y);
            var fullMoon = CelestialLighting.EvaluateMoon(
                0f,
                45f,
                22f,
                0.5f,
                0.14f,
                midnight.LocalDirection.y);
            if (sunrise.LocalDirection.x < 0.999f
                || Mathf.Abs(sunrise.LocalDirection.y) > 0.001f
                || sunset.LocalDirection.x > -0.999f
                || Mathf.Abs(sunset.LocalDirection.y) > 0.001f
                || noon.LocalDirection.y < 0.70f
                || noon.LocalDirection.z > -0.70f
                || midnight.LocalDirection.y > -0.70f)
            {
                throw new InvalidOperationException(
                    "The configured latitude does not produce an opposite sunrise and sunset path.");
            }
            if (sunrise.SunColour.r <= sunrise.SunColour.b
                || sunrise.AmbientColour.b <= sunrise.AmbientColour.r
                || noon.SunIntensity <= sunrise.SunIntensity
                || midnight.SunIntensity != 0f
                || midnight.AmbientColour.b <= midnight.AmbientColour.r
                || midnight.AmbientColour.maxColorComponent
                    >= noon.AmbientColour.maxColorComponent
                || midnight.SunVisibility != 0f
                || midnight.SkyExposure >= noon.SkyExposure
                || sunset.SunHaloStrength <= noon.SunHaloStrength
                || sunset.SunHaloStrength <= midnight.SunHaloStrength
                || midnight.NightStrength <= noon.NightStrength)
            {
                throw new InvalidOperationException(
                    "The solar cycle does not preserve a sunset sun halo and blue night ambience.");
            }
            if (!Mathf.Approximately(newMoon.OrbitLatitudeDegrees, 23f)
                || newMoon.Illumination != 0f
                || fullMoon.Illumination < 0.999f
                || fullMoon.LightIntensity <= 0f
                || Vector3.Dot(
                    fullMoon.LocalDirection,
                    fullMoon.LocalLightDirection) > -0.999f)
            {
                throw new InvalidOperationException(
                    "The lunar orbit, phase illumination, or full-moon light is invalid.");
            }
        }
        internal static void ValidateFirstPersonDistanceHaze(IslandGenerationRequest request)
        {
            var reflections = UnityEngine.Object.FindObjectsByType<PlanarWaterReflection>(FindObjectsInactive.Include);
            var planes = reflections.Select(value => value.ReflectionPlane).ToArray();
            var host = new GameObject("World haze validation");
            var environment = host.AddComponent<WorldEnvironmentController>();
            try
            {
                var settings = new WorldEnvironmentSettings();
                environment.Initialize(settings, new IslandCloudSettings(), 64f, 128f, null);
                environment.SetFirstPersonViewActive(true);
                if (!RenderSettings.fog || RenderSettings.fogMode != FogMode.ExponentialSquared
                    || !Mathf.Approximately(RenderSettings.fogDensity, settings.DistanceHazeDensity))
                    throw new InvalidOperationException("Entering first person did not apply world distance haze.");
                var islandHost = new GameObject("Non-owning island");
                islandHost.SetActive(false);
                var island = islandHost.AddComponent<IslandGenerator>();
                island.Configure(request);
                UnityEngine.Object.DestroyImmediate(islandHost);
                if (!RenderSettings.fog || !environment.IsInstalled)
                    throw new InvalidOperationException("Disposing an island reset the world environment.");
                environment.SetFirstPersonViewActive(false);
                if (RenderSettings.fog) throw new InvalidOperationException("Leaving first person did not disable distance haze.");
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(host);
                for (var i = 0; i < reflections.Length; i++) reflections[i].Configure(planes[i]);
            }
        }
    }
}
