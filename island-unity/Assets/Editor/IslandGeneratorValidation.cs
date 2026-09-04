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
        IslandGenerator.ValidateMaterialTextureCacheRoundTrip();
        ValidateConfigurationAssetContract();
        ValidateWaterfallMistShader();
        ValidateFernShader();
        ValidateSkyDomeShader();
        ValidateCloudReceiverShaders();
        ValidateSolarLightingCycle();
        ValidateOpenSeaEnvironmentAnchoring();
        ValidateOceanWaveSystem();
        IslandWorldManager.ValidateRoutingPolicy();
        IslandProjectSetup.ValidateMultiIslandSandbox();
        ValidateSandboxScene();
        ValidateRealtimeShadowRender();
        Debug.Log("IslandGenerator component, sandbox level, and native validation passed.");
    }

    private static void ValidateConfigurationAssetContract()
    {
        var configuration = ScriptableObject.CreateInstance<IslandConfiguration>();
        try
        {
            if (configuration.Generation == null
                || configuration.Rivers == null
                || configuration.Forest == null
                || configuration.Reeds == null
                || configuration.Ferns == null
                || configuration.Clouds == null
                || configuration.Rendering == null
                || configuration.Decorations == null
                || configuration.DebugSettings == null)
            {
                throw new InvalidOperationException(
                    "A new IslandConfiguration is missing a reusable settings group.");
            }
            if (typeof(IslandConfiguration).GetProperty("Streaming") != null)
            {
                throw new InvalidOperationException(
                    "Scene-specific streaming references must not be stored in IslandConfiguration assets.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(configuration);
        }
    }

    private static void ValidateOpenSeaEnvironmentAnchoring()
    {
        var anchor = WorldEnvironmentController.SnapAnchor(
            new Vector3(37f, 912f, -38f),
            4.5f,
            25f);
        if (anchor != new Vector3(25f, 4.5f, -50f))
        {
            throw new InvalidOperationException(
                "The open-sea environment anchor is not snapped in XZ at sea level.");
        }
        var nearbyAnchor = WorldEnvironmentController.SnapAnchor(
            new Vector3(37.4f, -120f, -38.2f),
            4.5f,
            25f);
        if (nearbyAnchor != anchor)
        {
            throw new InvalidOperationException(
                "Small player movements should not move the open-sea environment.");
        }

        ValidateOpenSeaEnvironmentOwnership();
        ValidateCoastalOverlayIsolation();
    }

    public static void BatchValidateOceanWaves()
    {
        ValidateOceanWaveSystem();
        IslandProjectSetup.ValidateOceanWaveSandbox();
        Debug.Log("Ocean clipmap, geometric-wave shaders, and sea-only sandbox passed validation.");
    }

    private static void ValidateOceanWaveSystem()
    {
        var settings = OceanWaveRuntimeSettings.Default;
        var coordinates = OceanClipmapMeshBuilder.BuildAxisCoordinates(
            10000f,
            settings);
        var mesh = OceanClipmapMeshBuilder.Build(
            20000f,
            settings,
            markNoLongerReadable: false);
        try
        {
            var expectedVertices = coordinates.Count * coordinates.Count;
            var expectedTriangles = (coordinates.Count - 1)
                * (coordinates.Count - 1)
                * 2;
            var snapSteps = settings.MaskAnchorSnapMetres
                / settings.FineVertexSpacingMetres;
            if (!settings.Enabled
                || Mathf.Abs(settings.FineVertexSpacingMetres - 1f) > 1.0e-5f
                || settings.DisplacementFadeStartMetres
                    >= settings.DisplacementFadeEndMetres
                || settings.DisplacementFadeEndMetres
                    > settings.FineRadiusMetres
                || Mathf.Abs(snapSteps - Mathf.Round(snapSteps)) > 1.0e-5f
                || !float.IsFinite(settings.MaximumVerticalDisplacement)
                || settings.MaximumVerticalDisplacement <= 0f
                || expectedVertices > OceanClipmapMeshBuilder.MaximumVertexCount
                || expectedTriangles > OceanClipmapMeshBuilder.MaximumTriangleCount
                || mesh.vertexCount != expectedVertices
                || mesh.GetIndexCount(0) != (uint)(expectedTriangles * 3)
                || mesh.bounds.extents.x < 10000f
                || mesh.bounds.extents.z < 10000f
                || mesh.bounds.extents.y
                    < settings.MaximumVerticalDisplacement)
            {
                throw new InvalidOperationException(
                    "The ocean clipmap does not satisfy its deterministic topology or bounds contract.");
            }
            if (Mathf.Abs(coordinates[0] + 10000f) > 1.0e-5f
                || Mathf.Abs(coordinates[coordinates.Count - 1] - 10000f)
                    > 1.0e-5f)
            {
                throw new InvalidOperationException(
                    "The ocean clipmap does not reach its configured outer extent exactly.");
            }
            for (var index = 1; index < coordinates.Count; index++)
            {
                if (!float.IsFinite(coordinates[index])
                    || coordinates[index] <= coordinates[index - 1])
                {
                    throw new InvalidOperationException(
                        "The ocean clipmap contains a non-finite or non-increasing axis coordinate.");
                }
            }
            var vertices = mesh.vertices;
            var normals = mesh.normals;
            var uv = mesh.uv;
            var triangles = mesh.triangles;
            if (vertices.Length != expectedVertices
                || normals.Length != expectedVertices
                || uv.Length != expectedVertices
                || triangles.Length != expectedTriangles * 3)
            {
                throw new InvalidOperationException(
                    "The ocean clipmap vertex attributes are incomplete.");
            }
            for (var index = 0; index < vertices.Length; index++)
            {
                var vertex = vertices[index];
                var normal = normals[index];
                var textureCoordinate = uv[index];
                if (!float.IsFinite(vertex.x)
                    || !float.IsFinite(vertex.y)
                    || !float.IsFinite(vertex.z)
                    || normal != Vector3.up
                    || !float.IsFinite(textureCoordinate.x)
                    || !float.IsFinite(textureCoordinate.y))
                {
                    throw new InvalidOperationException(
                        "The ocean clipmap contains a non-finite vertex attribute or invalid normal.");
                }
            }
            for (var index = 0; index < triangles.Length; index += 3)
            {
                var first = vertices[triangles[index]];
                var second = vertices[triangles[index + 1]];
                var third = vertices[triangles[index + 2]];
                if (Vector3.Cross(second - first, third - first).y <= 1.0e-6f)
                {
                    throw new InvalidOperationException(
                        "The ocean clipmap contains a degenerate or downward-facing triangle.");
                }
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(mesh);
        }

        var seaShader = Shader.Find("Motu/Sea Water");
        var attenuationShader = Shader.Find("Hidden/Motu/Ocean Wave Attenuation");
        var onshoreShader = Shader.Find("Hidden/Motu/Ocean Onshore Direction");
        if (seaShader == null
            || attenuationShader == null
            || onshoreShader == null
            || !seaShader.isSupported
            || !attenuationShader.isSupported
            || !onshoreShader.isSupported
            || ShaderUtil.ShaderHasError(seaShader)
            || ShaderUtil.ShaderHasError(attenuationShader)
            || ShaderUtil.ShaderHasError(onshoreShader))
        {
            throw new InvalidOperationException(
                "The geometric ocean-wave shaders are missing, unsupported, or contain errors.");
        }
        var seaMaterial = new Material(seaShader);
        var attenuationMaterial = new Material(attenuationShader);
        var onshoreMaterial = new Material(onshoreShader);
        try
        {
            if (!seaMaterial.HasProperty("_GeometricWaves")
                || !seaMaterial.HasProperty("_WaveAttenuationTex")
                || !seaMaterial.HasProperty("_WaveOnshoreTex")
                || !seaMaterial.HasProperty("_WaveAttenuationWorldRect")
                || !seaMaterial.HasProperty("_WaveFadeStart")
                || !seaMaterial.HasProperty("_WaveFadeEnd")
                || !seaMaterial.HasProperty("_OceanWave0")
                || !seaMaterial.HasProperty("_OceanWaveSpeeds")
                || !seaMaterial.HasProperty("_WaveNoiseWorldSize")
                || !seaMaterial.HasProperty("_WaveDomainWarp")
                || !seaMaterial.HasProperty("_WaveAmplitudeVariation")
                || !seaMaterial.HasProperty("_WhitecapColour")
                || !seaMaterial.HasProperty("_WhitecapStrength")
                || !seaMaterial.HasProperty("_WhitecapHeightThreshold")
                || !seaMaterial.HasProperty("_WhitecapSlopeThreshold")
                || !seaMaterial.HasProperty("_WhitecapCoverage")
                || !seaMaterial.HasProperty("_WhitecapNoiseWorldSize")
                || !seaMaterial.HasProperty("_WhitecapFineNoiseScale")
                || !seaMaterial.HasProperty("_WhitecapCounterflowSpeed")
                || !seaMaterial.HasProperty("_OnshoreWaveEnabled")
                || !seaMaterial.HasProperty("_OnshoreWaveParameters")
                || !seaMaterial.HasProperty("_OnshoreWaveBreaking")
                || seaMaterial.HasProperty("_SeaMask")
                || !attenuationMaterial.HasProperty("_SeaMask")
                || !attenuationMaterial.HasProperty("_IslandWorldSize")
                || !attenuationMaterial.HasProperty("_CompositionWorldRect")
                || !attenuationMaterial.HasProperty("_DepthAllowancePower")
                || !attenuationMaterial.HasProperty("_DistanceAllowancePower")
                || !onshoreMaterial.HasProperty("_MainTex"))
            {
                throw new InvalidOperationException(
                    "The global ocean or attenuation composer violates its shader-property contract.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(seaMaterial);
            UnityEngine.Object.DestroyImmediate(attenuationMaterial);
            UnityEngine.Object.DestroyImmediate(onshoreMaterial);
        }
    }

    private static void ValidateCoastalOverlayIsolation()
    {
        var deepOceanShader = Shader.Find("Motu/Sea Water");
        var coastalShader = Shader.Find("Motu/Coastal Water Overlay");
        if (deepOceanShader == null || coastalShader == null)
        {
            throw new InvalidOperationException(
                "The deep-ocean or coastal-overlay shader is unavailable.");
        }
        var deepOcean = new Material(deepOceanShader);
        var firstCoast = new Material(coastalShader);
        var secondCoast = new Material(coastalShader);
        var firstMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        var secondMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        try
        {
            var firstMatrix = Matrix4x4.Translate(new Vector3(-2000f, 0f, 750f));
            var secondMatrix = Matrix4x4.Translate(new Vector3(3200f, 0f, -900f));
            firstCoast.SetMatrix("_IslandWorldToLocal", firstMatrix);
            secondCoast.SetMatrix("_IslandWorldToLocal", secondMatrix);
            firstCoast.SetTexture("_SeaMask", firstMask);
            secondCoast.SetTexture("_SeaMask", secondMask);
            if (deepOcean.HasProperty("_SeaMask")
                || deepOcean.HasProperty("_WorldSize")
                || firstCoast.GetMatrix("_IslandWorldToLocal")
                    == secondCoast.GetMatrix("_IslandWorldToLocal")
                || firstCoast.GetTexture("_SeaMask")
                    == secondCoast.GetTexture("_SeaMask")
                || firstCoast.renderQueue <= deepOcean.renderQueue)
            {
                throw new InvalidOperationException(
                    "Mock islands did not retain isolated coastal masks, transforms, and ordering.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(firstMask);
            UnityEngine.Object.DestroyImmediate(secondMask);
            UnityEngine.Object.DestroyImmediate(firstCoast);
            UnityEngine.Object.DestroyImmediate(secondCoast);
            UnityEngine.Object.DestroyImmediate(deepOcean);
        }
    }

    private static void ValidateOpenSeaEnvironmentOwnership()
    {
        var root = new GameObject("Open Sea Environment Validation");
        var target = new GameObject("Open Sea Follow Target");
        try
        {
            target.transform.position = new Vector3(37f, 120f, -38f);
            var controller = root.AddComponent<WorldEnvironmentController>();
            controller.SetFollowTarget(target.transform);
            var firstSkyMaterial = new Material(Shader.Find("Motu/Sky Dome"));
            var firstSeaMaterial = new Material(Shader.Find("Motu/Sea Water"));
            var firstWeather = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            var firstOceanNoise = new Texture2D(1, 1, TextureFormat.R8, false, true);
            controller.Install(
                firstSkyMaterial,
                firstSeaMaterial,
                firstWeather,
                firstOceanNoise,
                2000f,
                4200f,
                0f,
                true,
                null);
            var retainedSkyMesh = controller.SkyMesh;
            if (retainedSkyMesh == null
                || controller.OceanTransform == null
                || controller.MoonLight == null
                || controller.AnchorPosition != new Vector3(25f, 0f, -50f))
            {
                throw new InvalidOperationException(
                    "The global sky, ocean, moon, or player-relative anchor was not installed.");
            }

            var secondSkyMaterial = new Material(Shader.Find("Motu/Sky Dome"));
            var secondSeaMaterial = new Material(Shader.Find("Motu/Sea Water"));
            var secondWeather = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            var secondOceanNoise = new Texture2D(1, 1, TextureFormat.R8, false, true);
            controller.Install(
                secondSkyMaterial,
                secondSeaMaterial,
                secondWeather,
                secondOceanNoise,
                2000f,
                4200f,
                0f,
                true,
                null);
            if (controller.SkyMesh != retainedSkyMesh
                || firstSkyMaterial != null
                || firstSeaMaterial != null
                || firstWeather != null
                || firstOceanNoise != null)
            {
                throw new InvalidOperationException(
                    "Environment replacement did not reuse global geometry or release old resources.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(target);
            UnityEngine.Object.DestroyImmediate(root);
        }
    }

    private static void ValidateFernShader()
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
                || !material.HasProperty("_FernWindMultiplier")
                || !material.HasProperty("_GrassPatchNoise"))
            {
                throw new InvalidOperationException(
                    "The forest-fern shader is missing its AO, reflection, or wind contract.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(material);
        }
    }

    private static void ValidateWaterfallMistShader()
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

    private static void ValidateSkyDomeShader()
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

    private static void ValidateCloudReceiverShaders()
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

    private static void ValidateSolarLightingCycle()
    {
        const float midnightToNoonRateRatio = 10f;
        var noonClockRate = IslandGenerator.EvaluateSolarClockRateMultiplier(
            12f,
            midnightToNoonRateRatio);
        var midnightClockRate = IslandGenerator.EvaluateSolarClockRateMultiplier(
            0f,
            midnightToNoonRateRatio);
        var sunriseClockRate = IslandGenerator.EvaluateSolarClockRateMultiplier(
            6f,
            midnightToNoonRateRatio);
        var sunsetClockRate = IslandGenerator.EvaluateSolarClockRateMultiplier(
            18f,
            midnightToNoonRateRatio);
        var inverseRateIntegral = 0f;
        const int clockSamples = 4096;
        for (var sample = 0; sample < clockSamples; sample++)
        {
            inverseRateIntegral += 1f / IslandGenerator.EvaluateSolarClockRateMultiplier(
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
            IslandGenerator.EvaluateSolarClockRateMultiplier(3f, 1f),
            1f))
        {
            throw new InvalidOperationException(
                "A one-to-one solar clock rate must remain uniform.");
        }
        var sunrise = IslandGenerator.EvaluateSolarLighting(6f, 45f, 1.25f);
        var noon = IslandGenerator.EvaluateSolarLighting(12f, 45f, 1.25f);
        var sunset = IslandGenerator.EvaluateSolarLighting(18f, 45f, 1.25f);
        var midnight = IslandGenerator.EvaluateSolarLighting(0f, 45f, 1.25f);
        var newMoon = IslandGenerator.EvaluateMoonLighting(
            12f,
            45f,
            22f,
            0f,
            0.14f,
            noon.LocalDirection.y);
        var fullMoon = IslandGenerator.EvaluateMoonLighting(
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
        if (island.Rendering.SunCycleDurationMinutes <= 0.25f
            || island.Rendering.MidnightToNoonClockRateRatio < 1f
            || Mathf.Abs(island.Rendering.SunLatitudeDegrees) < 0.01f
            || island.Rendering.MiddaySunIntensity <= 0f
            || island.Rendering.MoonEquatorOffsetDegrees <= 0f
            || island.Rendering.FullMoonLightIntensity <= 0f)
        {
            throw new InvalidOperationException(
                "The sandbox solar or lunar cycle settings are invalid.");
        }
        if (!island.Rendering.ShowDistanceHaze
            || island.Rendering.DistanceHazeDensity <= 0f)
        {
            throw new InvalidOperationException(
                "The sandbox first-person distance haze is missing or has invalid density.");
        }
        ValidateFirstPersonDistanceHaze(island);
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

    private static void ValidateFirstPersonDistanceHaze(IslandGenerator island)
    {
        var setFirstPerson = typeof(IslandGenerator).GetMethod(
            "SetFirstPersonViewActive",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic);
        if (setFirstPerson == null)
        {
            throw new InvalidOperationException(
                "The IslandGenerator has no first-person haze mode boundary.");
        }

        var originalFog = RenderSettings.fog;
        var originalMode = RenderSettings.fogMode;
        var originalColour = RenderSettings.fogColor;
        var originalDensity = RenderSettings.fogDensity;
        try
        {
            setFirstPerson.Invoke(island, new object[] { true });
            if (!RenderSettings.fog
                || RenderSettings.fogMode != FogMode.ExponentialSquared
                || RenderSettings.fogColor != island.Rendering.DistanceHazeColour
                || !Mathf.Approximately(
                    RenderSettings.fogDensity,
                    island.Rendering.DistanceHazeDensity))
            {
                throw new InvalidOperationException(
                    "Entering first person did not apply exponential-squared distance haze.");
            }

            setFirstPerson.Invoke(island, new object[] { false });
            if (RenderSettings.fog)
            {
                throw new InvalidOperationException(
                    "Returning to overview did not disable distance haze.");
            }
        }
        finally
        {
            setFirstPerson.Invoke(island, new object[] { false });
            RenderSettings.fog = originalFog;
            RenderSettings.fogMode = originalMode;
            RenderSettings.fogColor = originalColour;
            RenderSettings.fogDensity = originalDensity;
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
            || !reflections[0].UseSimplifiedShader
            || reflections[0].SimplifiedReflectionShader == null
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

            InvokePrivateLifecycleMethod(reflection, "OnPreCull");
            if (reflection.ReflectionRenderCount != 1)
            {
                throw new InvalidOperationException(
                    "The planar reflection did not reuse its render on the skipped frame.");
            }
            InvokePrivateLifecycleMethod(reflection, "OnPreCull");
            if (reflection.ReflectionRenderCount != 2)
            {
                throw new InvalidOperationException(
                    "The planar reflection did not render again at its configured interval.");
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
