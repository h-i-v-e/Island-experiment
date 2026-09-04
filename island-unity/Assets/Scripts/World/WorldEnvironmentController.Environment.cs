using System;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class WorldEnvironmentController
{
    private const float SunDiscAngularRadiusDegrees = 0.7f;
    private const float MoonDiscAngularRadiusDegrees = 0.68f;
    private static readonly Color SunsetSunHaloColour = new Color(0.85f, 0.05f, 0.01f, 1f);
    private static readonly Color NightHazeColour = new Color(0.08f, 0.14f, 0.28f, 1f);
    private static readonly Color MoonDiscColour = new Color(0.78f, 0.84f, 0.92f, 1f);
    private static readonly Color MoonDarkColour = new Color(0.012f, 0.018f, 0.035f, 1f);
    private static readonly Color MoonLightColour = new Color(0.48f, 0.62f, 0.90f, 1f);
    private static readonly int SunDirectionId = Shader.PropertyToID("_SunDirection");
    private static readonly int SunColourId = Shader.PropertyToID("_SunColor");
    private static readonly int SunVisibilityId = Shader.PropertyToID("_SunVisibility");
    private static readonly int SunDiscCosRadiusId = Shader.PropertyToID("_SunDiscCosRadius");
    private static readonly int SunHaloColourId = Shader.PropertyToID("_SunHaloColor");
    private static readonly int SunHaloStrengthId = Shader.PropertyToID("_SunHaloStrength");
    private static readonly int SkyExposureId = Shader.PropertyToID("_SkyExposure");
    private static readonly int WaterSkyExposureId = Shader.PropertyToID("_WaterSkyExposure");
    private static readonly int MoonDirectionId = Shader.PropertyToID("_MoonDirection");
    private static readonly int MoonLightDirectionId = Shader.PropertyToID("_MoonLightDirection");
    private static readonly int MoonColourId = Shader.PropertyToID("_MoonColor");
    private static readonly int MoonDarkColourId = Shader.PropertyToID("_MoonDarkColor");
    private static readonly int MoonVisibilityId = Shader.PropertyToID("_MoonVisibility");
    private static readonly int MoonDiscCosRadiusId = Shader.PropertyToID("_MoonDiscCosRadius");
    private static readonly int StarSettingsId = Shader.PropertyToID("_StarSettings");
    private static readonly int StarVisibilityId = Shader.PropertyToID("_StarVisibility");
    private static readonly int StarRotationId = Shader.PropertyToID("_StarRotation");
    private static readonly int CloudWeatherTextureId = Shader.PropertyToID("_MotuCloudWeatherTex");
    private static readonly int CloudEnabledId = Shader.PropertyToID("_MotuCloudEnabled");
    private static readonly int CloudLightDirectionId = Shader.PropertyToID("_MotuCloudLightDirection");
    private static readonly int CloudLightActiveId = Shader.PropertyToID("_MotuCloudLightActive");
    private static readonly int CloudLightColourId = Shader.PropertyToID("_MotuCloudLightColor");

    private WorldEnvironmentSettings environmentSettings;
    private IslandCloudSettings cloudSettings;
    private Light sunlight;
    private int environmentSeed;
    private Vector2 cloudWindOffset;
    private Vector2 cloudBroadWindOffset;
    private bool firstPersonViewActive;
    private bool solarClockInitialized;
    private float solarTimeHours;
    private float lunarPhase;
    private float currentSkyExposure = 1f;
    private float currentNightStrength;

    public void Initialize(
        WorldEnvironmentSettings settings,
        IslandCloudSettings clouds,
        int seed,
        float domeRadiusMetres,
        float environmentDiameterMetres,
        Transform target)
    {
        environmentSettings = settings
            ?? throw new ArgumentNullException(nameof(settings));
        cloudSettings = clouds ?? throw new ArgumentNullException(nameof(clouds));
        environmentSeed = seed;
        sunlight = settings.Sunlight != null ? settings.Sunlight : RenderSettings.sun;
        SetFollowTarget(target);

        var sky = CreateSkyMaterial(settings);
        var noise = settings.SeaNoise != null
            ? settings.SeaNoise
            : IslandGenerator.CreateRiverNoiseTexture();
        var ownedNoise = settings.SeaNoise == null ? noise : null;
        var sea = CreateSeaMaterial(settings, noise);
        var weather = CreateCloudWeatherTexture(seed, clouds);
        Install(
            sky,
            sea,
            weather,
            ownedNoise,
            domeRadiusMetres,
            environmentDiameterMetres,
            settings.SeaLevelMetres,
            settings.ShowSea,
            settings.OceanWaveProfile != null
                ? settings.OceanWaveProfile.ToRuntimeSettings()
                : OceanWaveRuntimeSettings.Default,
            sunlight);
        solarClockInitialized = false;
        UpdateSolarLighting(0f);
        ApplyCloudSettings(0f);
        ApplyDistanceHazeSettings();
    }

    public void SetFirstPersonViewActive(bool active)
    {
        firstPersonViewActive = active;
        ApplyDistanceHazeSettings();
    }

    private static Material CreateSkyMaterial(WorldEnvironmentSettings settings)
    {
        var shader = Shader.Find("Motu/Sky Dome")
            ?? throw new InvalidOperationException("Could not find shader 'Motu/Sky Dome'.");
        var material = new Material(shader) { name = "Motu/Sky Dome (World)" };
        material.SetColor("_ZenithColor", settings.ZenithColour);
        material.SetColor("_HorizonColor", settings.DistanceHazeColour);
        material.SetFloat(
            SunDiscCosRadiusId,
            Mathf.Cos(SunDiscAngularRadiusDegrees * Mathf.Deg2Rad));
        material.SetColor(SunHaloColourId, SunsetSunHaloColour);
        material.SetColor(MoonColourId, MoonDiscColour);
        material.SetColor(MoonDarkColourId, MoonDarkColour);
        material.SetFloat(
            MoonDiscCosRadiusId,
            Mathf.Cos(MoonDiscAngularRadiusDegrees * Mathf.Deg2Rad));
        return material;
    }

    private static Material CreateSeaMaterial(
        WorldEnvironmentSettings settings,
        Texture noise)
    {
        var shader = Shader.Find("Motu/Sea Water")
            ?? throw new InvalidOperationException("Could not find shader 'Motu/Sea Water'.");
        var material = settings.SeaMaterial != null
            ? new Material(settings.SeaMaterial)
            : new Material(shader);
        material.name = "Motu/Sea Water (World)";
        if (settings.SeaMaterial == null)
        {
            material.SetColor("_Color", new Color(0.03f, 0.28f, 0.55f, 1f));
        }
        material.renderQueue = (int)RenderQueue.Transparent;
        material.SetTexture("_NoiseTex", noise);
        material.SetFloat("_ShallowOpacity", 0.25f);
        material.SetFloat("_OpacityDepth", 5f);
        material.SetColor("_ReflectionColor", settings.ZenithColour);
        material.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(settings.ZenithColour, Color.white, 0.35f));
        material.SetFloat("_ReflectionStrength", 0.65f);
        material.SetFloat("_PlanarReflectionWeight", 1f);
        material.SetFloat("_PlanarReflectionDistortion", 0.008f);
        material.SetFloat("_SunGlintStrength", 0.8f);
        return material;
    }

    private static Texture2D CreateCloudWeatherTexture(
        int worldSeed,
        IslandCloudSettings clouds)
    {
        var combinedSeed = unchecked(worldSeed * 397) ^ clouds.Seed;
        var resolution = clouds.WeatherMapResolution;
        MotuNative.ExportCloudWeatherMap native = default;
        try
        {
            if (MotuNative.CreateCloudWeatherMap(combinedSeed, resolution, out native) == 0
                || native.handle == IntPtr.Zero
                || native.rgba == IntPtr.Zero
                || native.width != resolution
                || native.height != resolution)
            {
                throw new InvalidOperationException(
                    "The Rust cloud weather-field generator returned invalid data.");
            }
            var rgba = new byte[checked(resolution * resolution * 4)];
            Marshal.Copy(native.rgba, rgba, 0, rgba.Length);
            var texture = new Texture2D(
                resolution,
                resolution,
                TextureFormat.RGBA32,
                true,
                true)
            {
                name = "Rust Generated World Cloud Weather Field",
                filterMode = FilterMode.Trilinear,
                wrapMode = TextureWrapMode.Repeat,
                anisoLevel = 1,
            };
            texture.SetPixelData(rgba, 0);
            texture.Apply(true, true);
            return texture;
        }
        finally
        {
            MotuNative.ReleaseCloudWeatherMap(ref native);
        }
    }

    private void ApplyCloudSettings(float deltaTime)
    {
        if (skyDomeMaterial == null || cloudSettings == null)
        {
            return;
        }
        var windDirection = cloudSettings.WindDirection;
        if (windDirection.sqrMagnitude > 0.000001f)
        {
            windDirection.Normalize();
            var travel = windDirection
                * (cloudSettings.WindSpeedMetresPerSecond * Mathf.Max(deltaTime, 0f));
            cloudWindOffset += travel;
            cloudBroadWindOffset += travel * 0.18f;
        }
        var worldSize = cloudSettings.WorldSizeMetres;
        var broadWorldSize = worldSize * cloudSettings.BroadNoiseScale;
        cloudWindOffset.x = Mathf.Repeat(cloudWindOffset.x, worldSize);
        cloudWindOffset.y = Mathf.Repeat(cloudWindOffset.y, worldSize);
        cloudBroadWindOffset.x = Mathf.Repeat(cloudBroadWindOffset.x, broadWorldSize);
        cloudBroadWindOffset.y = Mathf.Repeat(cloudBroadWindOffset.y, broadWorldSize);
        var enabled = cloudSettings.Enabled
            && cloudSettings.Coverage > 0f
            && cloudSettings.Density > 0f
            && cloudWeatherTexture != null;
        Shader.SetGlobalTexture(CloudWeatherTextureId, cloudWeatherTexture);
        Shader.SetGlobalFloat(CloudEnabledId, enabled ? 1f : 0f);
        Shader.SetGlobalFloat("_MotuCloudCoverage", cloudSettings.Coverage);
        Shader.SetGlobalFloat("_MotuCloudDensity", cloudSettings.Density);
        Shader.SetGlobalFloat("_MotuCloudAltitude", cloudSettings.AltitudeMetres);
        Shader.SetGlobalFloat("_MotuCloudWorldSize", worldSize);
        Shader.SetGlobalVector(
            "_MotuCloudVolume",
            new Vector4(cloudSettings.VerticalThicknessMetres, 0f, 0f, 0f));
        Shader.SetGlobalVector(
            "_MotuCloudBroadNoise",
            new Vector4(
                cloudSettings.BroadNoiseScale,
                cloudSettings.BroadNoiseStrength,
                0f,
                0f));
        Shader.SetGlobalVector(
            "_MotuCloudDetailErosion",
            new Vector4(cloudSettings.DetailStrength, cloudSettings.ErosionStrength, 0f, 0f));
        Shader.SetGlobalVector(
            "_MotuCloudWindOffset",
            new Vector4(
                cloudWindOffset.x,
                cloudWindOffset.y,
                cloudBroadWindOffset.x,
                cloudBroadWindOffset.y));
        Shader.SetGlobalColor("_MotuCloudDayColor", cloudSettings.DayColour);
        Shader.SetGlobalColor("_MotuCloudSunsetColor", cloudSettings.SunsetColour);
        Shader.SetGlobalColor("_MotuCloudNightColor", cloudSettings.NightColour);
        Shader.SetGlobalFloat("_MotuCloudShadowStrength", cloudSettings.ShadowStrength);
        Shader.SetGlobalFloat(
            "_MotuCloudAmbientShadowStrength",
            cloudSettings.AmbientShadowStrength);
        Shader.SetGlobalFloat(
            "_MotuCloudCelestialStrength",
            cloudSettings.CelestialObscurationStrength);
        Shader.SetGlobalFloat(
            "_MotuCloudLowElevationFade",
            cloudSettings.LowElevationShadowFade);
    }

    private void UpdateSolarLighting(float deltaTime)
    {
        if (!solarClockInitialized)
        {
            solarTimeHours = environmentSettings.StartingSolarTimeHours;
            lunarPhase = environmentSettings.StartingMoonPhase;
            solarClockInitialized = true;
        }
        if (deltaTime > 0f)
        {
            var cycleSeconds = environmentSettings.SunCycleDurationMinutes * 60f;
            var clockRate = CelestialLighting.EvaluateClockRateMultiplier(
                solarTimeHours,
                environmentSettings.MidnightToNoonClockRateRatio);
            solarTimeHours = Mathf.Repeat(
                solarTimeHours + deltaTime * 24f / cycleSeconds * clockRate,
                24f);
            lunarPhase = Mathf.Repeat(
                lunarPhase + deltaTime
                    / (cycleSeconds * CelestialLighting.LunarSynodicPeriodDays),
                1f);
        }
        var state = CelestialLighting.EvaluateSun(
            solarTimeHours,
            environmentSettings.SunLatitudeDegrees,
            environmentSettings.MiddaySunIntensity);
        var moonState = CelestialLighting.EvaluateMoon(
            solarTimeHours,
            environmentSettings.SunLatitudeDegrees,
            environmentSettings.MoonEquatorOffsetDegrees,
            lunarPhase,
            environmentSettings.FullMoonLightIntensity,
            state.LocalDirection.y);
        currentSkyExposure = state.SkyExposure;
        currentNightStrength = state.NightStrength;
        var sunDirection = state.LocalDirection.normalized;
        var moonDirection = moonState.LocalDirection.normalized;
        skyDomeMaterial.SetVector(SunDirectionId, sunDirection);
        skyDomeMaterial.SetColor(SunColourId, state.SunColour);
        skyDomeMaterial.SetColor("_HorizonColor", CurrentAtmosphericHorizonBaseColour());
        skyDomeMaterial.SetFloat(SunVisibilityId, state.SunVisibility);
        skyDomeMaterial.SetFloat(SunHaloStrengthId, state.SunHaloStrength);
        skyDomeMaterial.SetVector(MoonDirectionId, moonDirection);
        skyDomeMaterial.SetVector(MoonLightDirectionId, moonState.LocalLightDirection.normalized);
        skyDomeMaterial.SetFloat(MoonVisibilityId, moonState.Visibility);
        skyDomeMaterial.SetFloat(SkyExposureId, currentSkyExposure);
        skyDomeMaterial.SetFloat(StarVisibilityId, currentNightStrength);
        skyDomeMaterial.SetVector(
            StarSettingsId,
            new Vector4(
                environmentSettings.StarDensity,
                environmentSettings.StarBrightness,
                environmentSettings.StarSize,
                Mathf.Repeat(environmentSeed * 0.61803398875f, 4096f)));
        ApplyStarRotation(moonState.OrbitLatitudeDegrees);
        SeaMaterial?.SetFloat(WaterSkyExposureId, currentSkyExposure);

        if (sunlight != null)
        {
            PositionDirectionalLight(sunlight, sunDirection);
            sunlight.color = state.SunColour;
            sunlight.intensity = state.SunIntensity;
            sunlight.enabled = state.SunIntensity > 0.0001f;
        }
        var moonShadows = state.SunVisibility <= 0.0001f
            && state.SunIntensity <= 0.0001f
            && moonState.LightIntensity > 0.0001f;
        if (moonLight != null)
        {
            PositionDirectionalLight(moonLight, moonDirection);
            moonLight.color = MoonLightColour;
            moonLight.intensity = moonState.LightIntensity;
            moonLight.enabled = moonShadows;
        }
        var activeDirection = moonShadows ? moonDirection : sunDirection;
        var activeColour = moonShadows
            ? MoonLightColour * moonState.LightIntensity
            : state.SunColour * state.SunIntensity;
        RenderSettings.sun = moonShadows && moonLight != null ? moonLight : sunlight;
        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = state.AmbientColour;
        Shader.SetGlobalVector(CloudLightDirectionId, activeDirection);
        Shader.SetGlobalFloat(
            CloudLightActiveId,
            activeColour.maxColorComponent > 0.0001f ? 1f : 0f);
        Shader.SetGlobalColor(CloudLightColourId, activeColour);
        Shader.SetGlobalColor("_MotuCloudAmbientColor", state.AmbientColour);
        Shader.SetGlobalFloat("_MotuCloudSunsetStrength", state.SunHaloStrength);
        Shader.SetGlobalFloat("_MotuCloudNightStrength", state.NightStrength);
    }

    private void ApplyStarRotation(float orbitLatitudeDegrees)
    {
        var latitude = orbitLatitudeDegrees * Mathf.Deg2Rad;
        var orbitAxis = new Vector3(0f, Mathf.Sin(latitude), Mathf.Cos(latitude)).normalized;
        var orbitAngle = (Mathf.Repeat(solarTimeHours, 24f) - 6f) * Mathf.PI / 12f;
        skyDomeMaterial.SetVector(
            StarRotationId,
            new Vector4(orbitAxis.x, orbitAxis.y, orbitAxis.z, -orbitAngle));
    }

    private void PositionDirectionalLight(Light light, Vector3 sourceDirection)
    {
        light.transform.position = AnchorPosition
            + sourceDirection * Mathf.Max(skyDomeWorldSize, 1f) * 0.9f;
        var up = Vector3.forward;
        if (Mathf.Abs(Vector3.Dot(-sourceDirection, up)) > 0.98f)
        {
            up = Vector3.right;
        }
        light.transform.rotation = Quaternion.LookRotation(-sourceDirection, up);
    }

    private void ApplyDistanceHazeSettings()
    {
        if (environmentSettings == null)
        {
            return;
        }
        skyDomeMaterial?.SetColor("_HorizonColor", CurrentAtmosphericHorizonBaseColour());
        skyDomeMaterial?.SetFloat(SkyExposureId, currentSkyExposure);
        RenderSettings.fog = environmentSettings.ShowDistanceHaze && firstPersonViewActive;
        if (RenderSettings.fog)
        {
            RenderSettings.fogMode = FogMode.ExponentialSquared;
            RenderSettings.fogColor = CurrentAtmosphericHorizonColour();
            RenderSettings.fogDensity = environmentSettings.DistanceHazeDensity;
        }
    }

    private Color CurrentAtmosphericHorizonColour()
    {
        var colour = CurrentAtmosphericHorizonBaseColour() * currentSkyExposure;
        colour.a = environmentSettings.DistanceHazeColour.a;
        return colour;
    }

    private Color CurrentAtmosphericHorizonBaseColour()
    {
        var colour = Color.Lerp(
            environmentSettings.DistanceHazeColour,
            NightHazeColour,
            currentNightStrength);
        colour.a = environmentSettings.DistanceHazeColour.a;
        return colour;
    }

    private void PrepareCameraRender(Camera camera)
    {
        if (camera == null || environmentSettings == null)
        {
            return;
        }
        if (!PlanarWaterReflection.IsReflectionCamera(camera))
        {
            camera.depthTextureMode |= DepthTextureMode.Depth;
        }
        BindReflectionCamera(camera);
        var colour = CurrentAtmosphericHorizonColour();
        colour.a = 1f;
        camera.backgroundColor = colour;
    }
}
