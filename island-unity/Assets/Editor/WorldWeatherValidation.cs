using System;
using System.Reflection;
using UnityEditor;
using UnityEngine;

public static class WorldWeatherValidation
{
    public static void BatchValidateRuntimeWeather()
    {
        ValidateCoherentWeatherDriver();
        var root = new GameObject("Runtime weather validation");
        var authored = new WorldEnvironmentSettings();
        var original = WorldWeatherState.FromEnvironment(authored);
        var environment = root.AddComponent<WorldEnvironmentController>();
        try
        {
            var authoredClouds = new IslandCloudSettings { Coverage = .37f, Density = 1.7f };
            environment.Initialize(authored, authoredClouds, 64f, 128f, null);
            ValidateCloudWeather(environment, authoredClouds);
            var ocean = root.GetComponent<OceanSurfaceController>();
            var composer = root.GetComponent("OceanWaveMaskComposer");
            Invoke(composer, "LateUpdate");
            var mesh = ocean.SurfaceMesh;
            var vertices = mesh.vertexCount;
            var mask = ocean.SurfaceMaterial.GetTexture("_WaveAttenuationTex");
            var onshore = ocean.SurfaceMaterial.GetTexture("_WaveOnshoreTex");
            var compositions = ocean.WaveMaskCompositionCount;
            var initialBounds = mesh.bounds;

            var next = environment.Weather;
            next.WindDirection = new Vector2(0f, 2f);
            next.WindSpeedMetresPerSecond = 36f;
            next.WindGustSizeMetres = 24f;
            next.TreeWindBasePinHeightMetres = 1.2f;
            next.TreeWindFullBendHeightMetres = 12f;
            next.Waves.Wave0.AmplitudeMetres = 8f;
            next.Waves.Wave0.Choppiness = 1f;
            next.Waves.Wave1.WavelengthMetres = 19f;
            next.Waves.Wave2.SpeedMetresPerSecond = 3f;
            next.Waves.Wave3.Direction = Vector2.up;
            next.Waves.AmplitudeVariation = 0.7f;
            next.Waves.NoiseWorldSizeMetres = 4096f;
            next.Waves.DomainWarpMetres = 12f;
            next.Waves.WhitecapColour = Color.cyan;
            next.Waves.WhitecapStrength = 1.3f;
            next.Waves.WhitecapCoverage = 0.75f;
            next.Waves.WhitecapHeightThreshold = 0.7f;
            next.Waves.WhitecapSlopeThreshold = 0.2f;
            next.Waves.WhitecapNoiseWorldSizeMetres = 9f;
            next.Waves.WhitecapFineNoiseScale = 0.4f;
            next.Waves.WhitecapCounterflowSpeed = 0.8f;
            next.Waves.OnshoreWaveWavelengthMetres = 15f;
            next.Waves.OnshoreWaveAmplitudeMetres = 2f;
            next.Waves.OnshoreWaveSpeedMetresPerSecond = 4f;
            next.Waves.OnshoreWaveChoppiness = 0.6f;
            next.Waves.OnshoreWaveLeadingEdgeSharpness = 0.8f;
            next.Waves.OnshoreWaveSharpeningDistanceMetres = 14f;
            next.Waves.OnshoreWaveBreakingStartDepthMetres = 4.5f;
            next.Waves.OnshoreWaveBreakingFullDepthMetres = 3f;
            environment.ApplyWeather(next);
            Invoke(composer, "LateUpdate");

            Require(Shader.GetGlobalVector("_MotuWeatherWind") == new Vector4(0, 1, 36, 2),
                "Wind direction, speed or response did not reach shader globals.");
            Require(Shader.GetGlobalVector("_MotuWindMaterial") == new Vector4(.07f, 24, .35f, 0),
                "Vegetation wind settings did not reach shader globals.");
            Require(Shader.GetGlobalVector("_MotuTreeWindHeights") == new Vector4(1.2f, 12, 0, 0),
                "Tree root-pinning settings did not reach shader globals.");
            var material = ocean.SurfaceMaterial;
            Require(material.GetVector("_OceanWave0").w == 8f
                && material.GetVector("_OceanWave1").z == 19f
                && material.GetVector("_OceanWaveSpeeds").z == 3f
                && material.GetVector("_OceanWave3").y == 1f
                && material.GetVector("_OceanWaveChoppiness").x == 1f,
                "Directional wave edits did not reach the ocean material.");
            Require(material.GetFloat("_WaveAmplitudeVariation") == .7f
                && material.GetFloat("_WaveNoiseWorldSize") == 4096f
                && material.GetFloat("_WaveDomainWarp") == 12f,
                "Wave noise edits did not reach the ocean material.");
            Require(material.GetColor("_WhitecapColour") == Color.cyan
                && material.GetFloat("_WhitecapStrength") == 1.3f
                && material.GetFloat("_WhitecapCoverage") == .75f
                && material.GetFloat("_WhitecapHeightThreshold") == .7f
                && material.GetFloat("_WhitecapSlopeThreshold") == .2f
                && material.GetFloat("_WhitecapNoiseWorldSize") == 9f
                && material.GetFloat("_WhitecapFineNoiseScale") == .4f
                && material.GetFloat("_WhitecapCounterflowSpeed") == .8f,
                "Whitecap edits did not reach the ocean material.");
            Require(material.GetVector("_OnshoreWaveParameters") == new Vector4(15, 2, 4, .6f)
                && material.GetVector("_OnshoreWaveBreaking") == new Vector4(.8f, 14, 4.5f, 3f),
                "Onshore wave edits did not reach the ocean material.");
            Require(ocean.SurfaceMesh == mesh && mesh.vertexCount == vertices,
                "Changing weather rebuilt the ocean mesh.");
            Require(mesh.bounds.extents.y > initialBounds.extents.y
                && mesh.bounds.extents.y >= 8f * 1.22f * 1.7f * 2f,
                "Runtime wave heights did not expand culling bounds safely.");
            Require(material.GetTexture("_WaveAttenuationTex") == mask
                && material.GetTexture("_WaveOnshoreTex") == onshore
                && ocean.WaveMaskCompositionCount == compositions,
                "Changing wave shape reset or recomposed coastal masks.");
            Require(authored.WindDirection == new Vector2(1f, .25f)
                && authored.WindSpeedMetresPerSecond == original.WindSpeedMetresPerSecond,
                "Runtime weather mutated authored settings.");

            next.Waves.DepthAllowancePower = 2f;
            next.Waves.DistanceAllowancePower = 3f;
            next.Waves.Enabled = false;
            next.Waves.OnshoreWaveEnabled = false;
            environment.ApplyWeather(next);
            Invoke(composer, "LateUpdate");
            Require(ocean.SurfaceMesh == mesh && ocean.WaveMaskCompositionCount == compositions + 1
                && material.GetTexture("_WaveAttenuationTex") == mask
                && material.GetFloat("_GeometricWaves") == 0f
                && material.GetFloat("_OnshoreWaveEnabled") == 0f,
                "Attenuation edits must reuse resources and wave toggles must apply immediately.");

            var driver = root.AddComponent<RuntimeWeatherValidationDriver>();
            environment.WeatherDriver = driver;
            Invoke(environment, "UpdateWeatherDriver", .5f);
            Require(driver.Calls == 1 && environment.WindSpeedMetresPerSecond == 5f
                && material.GetFloat("_WhitecapStrength") == .2f
                && Shader.GetGlobalFloat("_MotuCloudCoverage") == .8f
                && Shader.GetGlobalFloat("_MotuCloudDensity") == 3f,
                "The assigned driver did not apply its weather state.");
            driver.enabled = false;
            Invoke(environment, "UpdateWeatherDriver", .5f);
            Require(driver.Calls == 1 && environment.WindSpeedMetresPerSecond == 5f
                && Shader.GetGlobalFloat("_MotuCloudCoverage") == .8f,
                "A disabled weather driver must retain the last weather state.");
            environment.SetWind(Vector2.left, 100f);
            Require(environment.WindDirection == Vector2.left
                && environment.WindSpeedMetresPerSecond == 40f
                && material.GetFloat("_WhitecapStrength") == .2f,
                "Direct wind changes must clamp speed and preserve wave settings.");
            var validWeather = environment.Weather;
            var invalid = validWeather;
            invalid.Waves.OnshoreWaveBreakingStartDepthMetres = float.NaN;
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.Waves.OnshoreWaveBreakingFullDepthMetres = float.PositiveInfinity;
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.Waves.Wave0.AmplitudeMetres = float.PositiveInfinity;
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.Waves.OnshoreWaveAmplitudeMetres = float.NaN;
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.WindSpeedMetresPerSecond = float.NaN;
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.Waves.WhitecapCoverage = float.NaN;
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.Waves.Wave0.Direction = new Vector2(float.PositiveInfinity, 0f);
            RequireRejected(environment, invalid);
            invalid = validWeather;
            invalid.Waves.Wave0.AmplitudeMetres = float.MaxValue;
            RequireRejected(environment, invalid);
            Require(environment.Weather.Waves.Wave0.AmplitudeMetres
                    == validWeather.Waves.Wave0.AmplitudeMetres
                && environment.WindSpeedMetresPerSecond == validWeather.WindSpeedMetresPerSecond
                && float.IsFinite(mesh.bounds.size.x)
                && float.IsFinite(mesh.bounds.size.y)
                && float.IsFinite(mesh.bounds.size.z),
                "Invalid driver output replaced the last valid weather or poisoned ocean bounds.");
            ValidateIndependentWaveDirections(environment, ocean);
            Debug.Log("Runtime weather validation passed: wind globals, cloud globals and drift, independent wave directions, all wave groups, driver lifecycle, authored settings, mesh retention, coastal masks, culling bounds and non-finite input rejection.");
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(root);
        }
    }

    private static void ValidateCloudWeather(
        WorldEnvironmentController environment, IslandCloudSettings authored)
    {
        var initial = environment.Weather;
        Require(initial.Clouds.Coverage == .37f && initial.Clouds.Density == 1.7f,
            "Runtime cloud state did not start from authored settings.");
        var texture = Shader.GetGlobalTexture("_MotuCloudWeatherTex");
        var windOffset = Shader.GetGlobalVector("_MotuCloudWindOffset");
        var next = initial;
        next.WindDirection = Vector2.up;
        next.WindSpeedMetresPerSecond = 10f;
        next.Clouds.Enabled = true;
        next.Clouds.Coverage = .8f;
        next.Clouds.Density = 4f;
        next.Clouds.AltitudeMetres = 500f;
        next.Clouds.VerticalThicknessMetres = 350f;
        next.Clouds.WorldSizeMetres = 3000f;
        next.Clouds.BroadNoiseScale = 8f;
        next.Clouds.BroadNoiseStrength = .4f;
        next.Clouds.DetailStrength = .6f;
        next.Clouds.ErosionStrength = .2f;
        next.Clouds.DayColour = Color.cyan;
        next.Clouds.SunsetColour = Color.red;
        next.Clouds.NightColour = Color.blue;
        next.Clouds.ShadowStrength = .9f;
        next.Clouds.AmbientShadowStrength = .3f;
        next.Clouds.CelestialObscurationStrength = 1.5f;
        next.Clouds.LowElevationShadowFade = .2f;
        Require(environment.Weather.Clouds.Coverage == .37f,
            "Editing a cloud snapshot mutated live weather before ApplyWeather.");
        environment.ApplyWeather(next);
        Require(Shader.GetGlobalFloat("_MotuCloudEnabled") == 1f
            && Shader.GetGlobalFloat("_MotuCloudCoverage") == .8f
            && Shader.GetGlobalFloat("_MotuCloudDensity") == 4f
            && Shader.GetGlobalFloat("_MotuCloudAltitude") == 500f
            && Shader.GetGlobalFloat("_MotuCloudWorldSize") == 3000f
            && Shader.GetGlobalVector("_MotuCloudVolume").x == 350f
            && Shader.GetGlobalVector("_MotuCloudBroadNoise") == new Vector4(8, .4f, 0, 0)
            && Shader.GetGlobalVector("_MotuCloudDetailErosion") == new Vector4(.6f, .2f, 0, 0),
            "Cloud shape changes did not reach shader globals immediately.");
        Require(Shader.GetGlobalColor("_MotuCloudDayColor") == Color.cyan
            && Shader.GetGlobalColor("_MotuCloudSunsetColor") == Color.red
            && Shader.GetGlobalColor("_MotuCloudNightColor") == Color.blue
            && Shader.GetGlobalFloat("_MotuCloudShadowStrength") == .9f
            && Shader.GetGlobalFloat("_MotuCloudAmbientShadowStrength") == .3f
            && Shader.GetGlobalFloat("_MotuCloudCelestialStrength") == 1.5f
            && Shader.GetGlobalFloat("_MotuCloudLowElevationFade") == .2f,
            "Cloud colour or lighting changes did not reach shader globals.");
        Require(Shader.GetGlobalTexture("_MotuCloudWeatherTex") == texture
            && Shader.GetGlobalVector("_MotuCloudWindOffset") == windOffset
            && authored.Coverage == .37f && authored.Density == 1.7f,
            "Applying clouds rebuilt the texture, advanced drift, or mutated authored settings.");
        Invoke(environment, "ApplyCloudSettings", .5f);
        Require(Mathf.Abs(Shader.GetGlobalVector("_MotuCloudWindOffset").y - windOffset.y - 5f) < .001f,
            "Runtime clouds did not drift with the shared wind.");
        next.Clouds.Enabled = false;
        environment.ApplyWeather(next);
        Require(Shader.GetGlobalFloat("_MotuCloudEnabled") == 0f,
            "Cloud disable did not apply immediately.");
        next.Clouds.Enabled = true;
        next.Clouds.Coverage = 0f;
        environment.ApplyWeather(next);
        Require(Shader.GetGlobalFloat("_MotuCloudEnabled") == 0f,
            "Zero coverage left clouds enabled.");
        next.Clouds.Coverage = 2f;
        next.Clouds.Density = 100f;
        environment.ApplyWeather(next);
        Require(environment.Weather.Clouds.Coverage == 1f && environment.Weather.Clouds.Density == 8f,
            "Runtime cloud settings did not enforce authored ranges.");
        var valid = environment.Weather;
        foreach (var field in typeof(CloudWeatherSettings).GetFields(BindingFlags.Instance | BindingFlags.Public))
        {
            if (field.FieldType == typeof(bool)) continue;
            object invalidClouds = valid.Clouds;
            field.SetValue(invalidClouds, field.FieldType == typeof(Color)
                ? (object)new Color(float.NaN, 0, 0, 1) : float.PositiveInfinity);
            var invalid = valid;
            invalid.Clouds = (CloudWeatherSettings)invalidClouds;
            invalid.WindSpeedMetresPerSecond = 39f;
            RequireRejected(environment, invalid);
        }
        Require(environment.Weather.Clouds.Coverage == valid.Clouds.Coverage
            && environment.WindSpeedMetresPerSecond == valid.WindSpeedMetresPerSecond
            && Shader.GetGlobalFloat("_MotuCloudCoverage") == valid.Clouds.Coverage,
            "Invalid cloud output changed live state or shaders.");
    }

    private static void ValidateIndependentWaveDirections(
        WorldEnvironmentController environment, OceanSurfaceController ocean)
    {
        var weather = environment.Weather;
        var directions = new[] { Vector2.up, Vector2.right, Vector2.left, new Vector2(1f, 1f).normalized };
        weather.WindDirection = Vector2.down;
        weather.Waves.Wave0.Direction = directions[0];
        weather.Waves.Wave1.Direction = directions[1];
        weather.Waves.Wave2.Direction = directions[2];
        weather.Waves.Wave3.Direction = directions[3];
        environment.ApplyWeather(weather);
        Invoke(ocean, "AdvanceWaveAnimation", 4f);
        var material = ocean.SurfaceMaterial;
        var patterns = new Vector4[4];
        for (var index = 0; index < 4; index++)
        {
            patterns[index] = material.GetVector($"_OceanWaveTo{index}");
            Require((new Vector2(patterns[index].x, patterns[index].y) - directions[index]).sqrMagnitude < .000001f,
                "The weather script's independent world-space direction did not reach the wave bank.");
        }
        foreach (var wind in new[] { Vector2.left, Vector2.up, Vector2.right })
        {
            environment.SetWind(wind, weather.WindSpeedMetresPerSecond);
            Invoke(ocean, "AdvanceWaveAnimation", 0f);
            for (var index = 0; index < 4; index++)
                Require(material.GetVector($"_OceanWaveTo{index}") == patterns[index],
                    "Changing only WindDirection rotated or reset a wave pattern.");
            Require(material.GetFloat("_OceanWaveTransition") == 0f,
                "Changing only WindDirection started a wave transition.");
        }
        weather.Waves.Wave0.Direction = Vector2.down;
        environment.ApplyWeather(weather);
        Invoke(ocean, "AdvanceWaveAnimation", 4f);
        for (var index = 1; index < 4; index++)
        {
            var actual = material.GetVector($"_OceanWaveTo{index}");
            Require(actual.x == patterns[index].x && actual.y == patterns[index].y,
                "Changing wave 0 must not rotate the other three waves.");
        }
    }

    private static void ValidateCoherentWeatherDriver()
    {
        var root = new GameObject("Coherent weather finite-output validation");
        var globalRandomState = UnityEngine.Random.state;
        try
        {
            var driver = root.AddComponent<CoherentWeatherDriver>();
            foreach (var seed in new[] { 42, 0, -1, int.MaxValue })
            {
                typeof(CoherentWeatherDriver).GetField("seed", BindingFlags.NonPublic | BindingFlags.Instance)
                    .SetValue(driver, seed);
                Invoke(driver, "Awake");
                Require(UnityEngine.Random.state.Equals(globalRandomState),
                    "Seeding weather must not modify Unity's global random sequence.");
                var weather = WorldWeatherState.FromEnvironment(new WorldEnvironmentSettings());
                driver.UpdateWeather(ref weather, 0f);
                var start = weather;
                for (var frame = 0; frame < 600; frame++)
                {
                    driver.UpdateWeather(ref weather, 1f / 60f);
                    var settings = OceanWaveRuntimeSettings.Default.WithWeather(weather.Waves);
                    Require(float.IsFinite(settings.MaximumVerticalDisplacement)
                        && float.IsFinite(settings.MaximumHorizontalDisplacement),
                        "Coherent weather produced invalid ocean bounds.");
                }
                Require((weather.WindDirection - start.WindDirection).sqrMagnitude > 0.000001f,
                    "Coherent weather must evolve with accumulated time at a constant frame rate.");
                var firstRun = weather;
                Invoke(driver, "Awake");
                weather = WorldWeatherState.FromEnvironment(new WorldEnvironmentSettings());
                for (var frame = 0; frame < 600; frame++)
                    driver.UpdateWeather(ref weather, 1f / 60f);
                Require(weather.WindDirection == firstRun.WindDirection
                    && weather.Waves.Wave0.AmplitudeMetres == firstRun.Waves.Wave0.AmplitudeMetres,
                    "Coherent weather must reproduce the same sequence for the same seed.");
            }
            Debug.Log("Coherent weather validation passed: finite bounds, accumulated time, deterministic seeds and independent random state.");
        }
        finally
        {
            UnityEngine.Random.state = globalRandomState;
            UnityEngine.Object.DestroyImmediate(root);
        }
    }

    private static void RequireRejected(WorldEnvironmentController environment, WorldWeatherState invalid)
    {
        try
        {
            environment.ApplyWeather(invalid);
        }
        catch (ArgumentOutOfRangeException)
        {
            return;
        }
        throw new InvalidOperationException("Invalid weather must be rejected before updating ocean bounds.");
    }

    private static void Require(bool success, string message)
    {
        if (!success) throw new InvalidOperationException(message);
    }

    private static void Invoke(object target, string method, params object[] arguments)
    {
        var entry = target.GetType().GetMethod(method, BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMethodException(target.GetType().Name, method);
        entry.Invoke(target, arguments);
    }
}

public sealed class RuntimeWeatherValidationDriver : WorldWeatherDriver
{
    public int Calls { get; private set; }

    public override void UpdateWeather(ref WorldWeatherState weather, float deltaTime)
    {
        Calls++;
        weather.WindSpeedMetresPerSecond = 10f * deltaTime;
        weather.Waves.WhitecapStrength = .2f;
        weather.Clouds.Coverage = .8f;
        weather.Clouds.Density = 3f;
    }
}
