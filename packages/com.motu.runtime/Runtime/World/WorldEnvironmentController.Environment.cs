using System;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;

namespace Motu.World
{
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
        private static readonly int CloudSecondaryLightDirectionId = Shader.PropertyToID("_MotuCloudSecondaryLightDirection");
        private static readonly int CloudSecondaryLightActiveId = Shader.PropertyToID("_MotuCloudSecondaryLightActive");
        private static readonly int CloudSecondaryLightColourId = Shader.PropertyToID("_MotuCloudSecondaryLightColor");
        private static readonly int WeatherWindId = Shader.PropertyToID("_MotuWeatherWind");
        private static readonly int WindMaterialId = Shader.PropertyToID("_MotuWindMaterial");
        private static readonly int TreeWindHeightsId = Shader.PropertyToID("_MotuTreeWindHeights");
        private static readonly int WindOffsetId = Shader.PropertyToID("_MotuWindOffset");
        private static readonly int WindNoiseTextureId = Shader.PropertyToID("_MotuWindNoise");
        public const float ReferenceWindSpeedMetresPerSecond = 9f;
        internal const float VegetationDisplacementAtReferenceSpeedMetres = 0.07f;
        internal const float DefaultWindGustSizeMetres = 12f;
        internal const float VegetationNormalResponseAtReferenceSpeed = 0.35f;
        internal const float DefaultTreeWindBasePinHeightMetres = 0.6f;
        internal const float DefaultTreeWindFullBendHeightMetres = 9f;

        private WorldEnvironmentSettings environmentSettings;
        private IslandCloudSettings cloudSettings;
        private Light sunlight;

        [Header("Celestial shadow handover")]
        [Tooltip("No directional shadows while the Sun is within this many degrees of the horizon, on either side.")]
        [Range(0f, 5f)]
        [SerializeField] private float shadowFreeHorizonDegrees = 0.25f;

        [Tooltip("Sun or Moon shadows reach their authored strength this many degrees away from the handover horizon.")]
        [Range(0.5f, 15f)]
        [SerializeField] private float fullShadowElevationDegrees = 3f;

        private int environmentSeed;
        private Vector2 windTravelOffset;
        private float cloudDomeRadius;
        private bool firstPersonViewActive;
        private bool solarClockInitialized;
        private float solarTimeHours;
        private float lunarPhase;
        private float currentSkyExposure = 1f;
        private float currentNightStrength;
        private float? sunLatitudeOverrideDegrees;

        /// <summary>Runtime observer latitude; null uses the authored world latitude.</summary>
        public float? SunLatitudeOverrideDegrees
        {
            get => sunLatitudeOverrideDegrees;
            set => sunLatitudeOverrideDegrees = value.HasValue
                ? Mathf.Clamp(value.Value, -80f, 80f)
                : (float?)null;
        }

        public float SunLatitudeDegrees =>
            sunLatitudeOverrideDegrees ?? environmentSettings?.SunLatitudeDegrees ?? 0f;

        public void Initialize(
            WorldEnvironmentSettings settings,
            IslandCloudSettings clouds,
            float domeRadiusMetres,
            float environmentDiameterMetres,
            Transform target)
        {
            if (settings == null) throw new ArgumentNullException(nameof(settings));
            if (clouds == null) throw new ArgumentNullException(nameof(clouds));
            AcquireEnvironment();
            CloudNoiseTextures.EnsureBound();
            cloudDomeRadius = domeRadiusMetres;
            Shader.SetGlobalFloat("_MotuCloudDomeRadius", cloudDomeRadius);
            environmentSettings = settings;
            cloudSettings = clouds;
            this.weather = WorldWeatherState.FromEnvironment(settings, clouds);
            environmentSeed = settings.Seed;
            sunlight = settings.Sunlight != null ? settings.Sunlight : hostState.Sun;
            hostState.BorrowLight(sunlight);
            SetFollowTarget(target);

            var sky = CreateSkyMaterial(settings);
            var noise = settings.WeatherNoise != null
                ? settings.WeatherNoise
                : ProceduralNoiseTextures.CreateWeatherNoiseTexture();
            var ownedNoise = settings.WeatherNoise == null ? noise : null;
            ApplyWeatherWindNoise(noise);
            var sea = CreateSeaMaterial(settings, noise);
            var weather = ProceduralNoiseTextures.CreateCloudWeatherTexture(settings.Seed, clouds);
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
            ApplyWeatherWind();
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
            var shader = Motu.Rendering.MotuShaders.Find("Motu/Sky Dome")
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
            var shader = Motu.Rendering.MotuShaders.Find("Motu/Sea Water")
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
                settings.DistanceHazeColour * (1f + settings.SkyHorizonLightening));
            material.SetFloat("_ReflectionStrength", 0.65f);
            material.SetFloat("_PlanarReflectionWeight", 1f);
            material.SetFloat("_PlanarReflectionDistortion", 0.008f);
            material.SetFloat("_SunGlintStrength", 0.8f);
            return material;
        }


        internal void ApplyCloudSettings(float deltaTime)
        {
            if (hostState == null || skyDomeMaterial == null || cloudSettings == null)
            {
                return;
            }
            var clouds = weather.Clouds;
            var windDirection = weather.WindDirection;
            if (windDirection.sqrMagnitude > 0.000001f)
            {
                windDirection.Normalize();
                var travel = windDirection
                    * (weather.WindSpeedMetresPerSecond
                        * Mathf.Max(deltaTime, 0f));
                windTravelOffset += travel;
            }
            var worldSize = clouds.WorldSizeMetres;
            var cloudWindOffset = windTravelOffset;
            // Only the periodic weather texture can wrap. Broad lattice noise
            // shares continuous travel with the independent volume noise.
            var cloudBroadWindOffset = windTravelOffset * 0.18f;
            cloudWindOffset.x = Mathf.Repeat(cloudWindOffset.x, worldSize);
            cloudWindOffset.y = Mathf.Repeat(cloudWindOffset.y, worldSize);
            var enabled = clouds.Enabled
                && clouds.Coverage > 0f
                && clouds.Density > 0f
                && cloudWeatherTexture != null;
            Shader.SetGlobalTexture(CloudWeatherTextureId, cloudWeatherTexture);
            Shader.SetGlobalFloat(CloudEnabledId, enabled ? 1f : 0f);
            Shader.SetGlobalFloat("_MotuCloudCoverage", clouds.Coverage);
            Shader.SetGlobalFloat("_MotuCloudDensity", clouds.Density);
            Shader.SetGlobalFloat("_MotuCloudAltitude", clouds.AltitudeMetres);
            Shader.SetGlobalFloat("_MotuCloudWorldSize", worldSize);
            Shader.SetGlobalVector(
                "_MotuCloudVolume",
                new Vector4(clouds.VerticalThicknessMetres, 0f, 0f, 0f));
            Shader.SetGlobalVector(
                "_MotuCloudBroadNoise",
                new Vector4(
                    clouds.BroadNoiseScale,
                    clouds.BroadNoiseStrength,
                    0f,
                    0f));
            Shader.SetGlobalVector(
                "_MotuCloudDetailErosion",
                new Vector4(clouds.DetailStrength, clouds.ErosionStrength, 0f, 0f));
            Shader.SetGlobalVector(
                "_MotuCloudShape",
                new Vector4(clouds.BillowSizeMetres, clouds.ErosionSizeMetres, clouds.CloudType, clouds.BillowStrength));
            Shader.SetGlobalVector(
                "_MotuCloudMotion",
                new Vector4(clouds.WindShear, clouds.Evolution, clouds.Quality, 0f));
            Shader.SetGlobalVector(
                "_MotuCloudLighting",
                new Vector4(clouds.SunlightPenetration, 0f, 0f, 0f));
            // The weather map wraps at its own period, but the independent 3D
            // noise must retain continuous travel across those map boundaries.
            Shader.SetGlobalVector(
                "_MotuCloudNoiseOffset",
                new Vector4(windTravelOffset.x, windTravelOffset.y, 0f, 0f));
            Shader.SetGlobalVector(
                "_MotuCloudWindOffset",
                new Vector4(
                    cloudWindOffset.x,
                    cloudWindOffset.y,
                    cloudBroadWindOffset.x,
                    cloudBroadWindOffset.y));
            Shader.SetGlobalColor("_MotuCloudDayColor", clouds.DayColour);
            Shader.SetGlobalColor("_MotuCloudSunsetColor", clouds.SunsetColour);
            Shader.SetGlobalColor("_MotuCloudNightColor", clouds.NightColour);
            Shader.SetGlobalFloat("_MotuCloudShadowStrength", clouds.ShadowStrength);
            Shader.SetGlobalFloat(
                "_MotuCloudAmbientShadowStrength",
                clouds.AmbientShadowStrength);
            Shader.SetGlobalFloat(
                "_MotuCloudCelestialStrength",
                clouds.CelestialObscurationStrength);
            Shader.SetGlobalFloat(
                "_MotuCloudLowElevationFade",
                clouds.LowElevationShadowFade);
        }

        public static float EvaluateWaveHeightScale(float windSpeedMetresPerSecond)
        {
            var normalizedSpeed = Mathf.Max(windSpeedMetresPerSecond, 0f)
                / ReferenceWindSpeedMetresPerSecond;
            return Mathf.Clamp(Mathf.Sqrt(normalizedSpeed), 0f, 2.5f);
        }

        internal static float ApplyWeatherWindGlobals(
            Vector2 configuredDirection,
            float configuredSpeedMetresPerSecond,
            float gustSizeMetres = DefaultWindGustSizeMetres,
            float treeBasePinHeightMetres = DefaultTreeWindBasePinHeightMetres,
            float treeFullBendHeightMetres = DefaultTreeWindFullBendHeightMetres)
        {
            var direction = configuredDirection;
            if (direction.sqrMagnitude <= 0.000001f)
            {
                direction = Vector2.right;
            }
            else
            {
                direction.Normalize();
            }
            var speed = Mathf.Clamp(configuredSpeedMetresPerSecond, 0f, 40f);
            var waveHeightScale = EvaluateWaveHeightScale(speed);
            Shader.SetGlobalVector(
                WeatherWindId,
                new Vector4(direction.x, direction.y, speed, waveHeightScale));
            Shader.SetGlobalVector(
                WindMaterialId,
                new Vector4(
                    VegetationDisplacementAtReferenceSpeedMetres,
                    Mathf.Clamp(gustSizeMetres, 1f, 64f),
                    VegetationNormalResponseAtReferenceSpeed,
                    0f));
            var basePinHeight = Mathf.Clamp(treeBasePinHeightMetres, 0f, 4f);
            Shader.SetGlobalVector(
                TreeWindHeightsId,
                new Vector4(
                    basePinHeight,
                    Mathf.Clamp(
                        treeFullBendHeightMetres,
                        Mathf.Max(basePinHeight + 0.01f, 1f),
                        24f),
                    0f,
                    0f));
            return waveHeightScale;
        }

        internal static void ApplyWeatherWindNoise(Texture noise)
        {
            if (noise != null)
            {
                Shader.SetGlobalTexture(WindNoiseTextureId, noise);
            }
        }

        internal static void ApplyWeatherWindOffset(Vector2 offsetMetres)
        {
            Shader.SetGlobalVector(
                WindOffsetId,
                new Vector4(offsetMetres.x, offsetMetres.y, 0f, 0f));
        }

        private void ApplyWeatherWind()
        {
            if (hostState == null || environmentSettings == null)
            {
                return;
            }
            var waveHeightScale = ApplyWeatherWindGlobals(
                weather.WindDirection,
                weather.WindSpeedMetresPerSecond,
                weather.WindGustSizeMetres,
                weather.TreeWindBasePinHeightMetres,
                weather.TreeWindFullBendHeightMetres);
            ApplyWeatherWindOffset(windTravelOffset);
            ocean?.ApplyWeatherWindScale(waveHeightScale);
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
            var latitude = SunLatitudeDegrees;
            var state = CelestialLighting.EvaluateSun(
                solarTimeHours,
                latitude,
                environmentSettings.MiddaySunIntensity);
            var moonState = CelestialLighting.EvaluateMoon(
                solarTimeHours,
                latitude,
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
            skyDomeMaterial.SetColor("_HorizonColor", CurrentSkyHorizonBaseColour());
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

            var sunAboveHorizon = sunDirection.y > 0f;
            var moonAboveHorizon = moonDirection.y > 0f;
            var clearAngle = Mathf.Clamp(shadowFreeHorizonDegrees, 0f, 5f);
            var fullAngle = Mathf.Clamp(fullShadowElevationDegrees, clearAngle + 0.1f, 15f);
            var clearElevation = Mathf.Sin(clearAngle * Mathf.Deg2Rad);
            var fullElevation = Mathf.Sin(fullAngle * Mathf.Deg2Rad);
            var maximumShadowStrength = hostState.GetAuthoredShadowStrength(sunlight);
            if (sunlight != null)
            {
                PositionDirectionalLight(sunlight, sunDirection);
                sunlight.color = state.SunColour;
                sunlight.intensity = state.SunIntensity;
                sunlight.enabled = sunAboveHorizon;
                sunlight.shadowStrength = maximumShadowStrength * Mathf.SmoothStep(0f, 1f,
                    Mathf.InverseLerp(clearElevation, fullElevation, sunDirection.y));
            }
            if (moonLight != null)
            {
                PositionDirectionalLight(moonLight, moonDirection);
                moonLight.color = MoonLightColour;
                moonLight.intensity = moonState.LightIntensity;
                moonLight.enabled = moonAboveHorizon;
                // Fade the outgoing map completely before changing main light,
                // then leave a small shadow-free interval before the other fades
                // in. Moonrise/moonset also fade instead of introducing a map.
                var moonHorizonFade = Mathf.SmoothStep(0f, 1f,
                    Mathf.InverseLerp(clearElevation, fullElevation, moonDirection.y));
                var handoverFade = sunlight == null ? 1f : Mathf.SmoothStep(0f, 1f,
                    Mathf.InverseLerp(clearElevation, fullElevation, -sunDirection.y));
                moonLight.shadowStrength = maximumShadowStrength * moonHorizonFade * handoverFade;
            }
            var moonIsPrimary = moonLight != null && moonAboveHorizon
                && (!sunAboveHorizon || sunlight == null);
            var sunColour = sunAboveHorizon
                ? state.SunColour * state.SunIntensity : Color.black;
            var moonColour = moonLight != null && moonAboveHorizon
                ? MoonLightColour * moonState.LightIntensity : Color.black;
            RenderSettings.sun = moonIsPrimary ? moonLight : sunlight;
            RenderSettings.ambientMode = AmbientMode.Flat;
            RenderSettings.ambientLight = state.AmbientColour;
            ApplyCloudCelestialLight(
                CloudLightDirectionId, CloudLightColourId, CloudLightActiveId,
                moonIsPrimary ? moonDirection : sunDirection,
                moonIsPrimary ? moonColour : sunColour);
            ApplyCloudCelestialLight(
                CloudSecondaryLightDirectionId, CloudSecondaryLightColourId, CloudSecondaryLightActiveId,
                moonIsPrimary ? sunDirection : moonDirection,
                moonIsPrimary ? sunColour : moonColour);
            Shader.SetGlobalColor("_MotuCloudAmbientColor", state.AmbientColour);
            Shader.SetGlobalFloat("_MotuCloudSunsetStrength", state.SunHaloStrength);
            Shader.SetGlobalFloat("_MotuCloudNightStrength", state.NightStrength);
        }

        private static void ApplyCloudCelestialLight(
            int directionId, int colourId, int activeId, Vector3 direction, Color colour)
        {
            Shader.SetGlobalVector(directionId, direction);
            Shader.SetGlobalColor(colourId, colour);
            Shader.SetGlobalFloat(activeId, colour.maxColorComponent > 0f ? 1f : 0f);
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
            if (hostState == null || environmentSettings == null)
            {
                return;
            }
            skyDomeMaterial?.SetColor("_HorizonColor", CurrentSkyHorizonBaseColour());
            skyDomeMaterial?.SetFloat(SkyExposureId, currentSkyExposure);
            SeaMaterial?.SetColor("_ReflectionHorizonColor", CurrentSkyHorizonBaseColour());
            Shader.SetGlobalColor("_MotuIslandHorizonColour", CurrentAtmosphericHorizonColour());
            RenderSettings.fog = environmentSettings.ShowDistanceHaze && firstPersonViewActive;
            if (RenderSettings.fog)
            {
                RenderSettings.fogMode = FogMode.ExponentialSquared;
                var colour = CurrentAtmosphericHorizonColour();
                RenderSettings.fogColor = QualitySettings.activeColorSpace == ColorSpace.Linear
                    ? colour.gamma : colour;
                RenderSettings.fogDensity = environmentSettings.DistanceHazeDensity;
            }
        }

        private Color CurrentSkyHorizonBaseColour()
        {
            // Lift only the sky; distant geometry and LOD3 must converge to the same haze.
            var colour = CurrentAtmosphericHorizonBaseColour() * (1f + environmentSettings.SkyHorizonLightening);
            colour.a = environmentSettings.DistanceHazeColour.a;
            return colour;
        }

        private Color CurrentAtmosphericHorizonColour()
        {
            // Global shader colours are uploaded verbatim; material Color properties
            // are converted by Unity. Match the sky's conversion before exposure.
            var colour = CurrentAtmosphericHorizonBaseColour();
            if (QualitySettings.activeColorSpace == ColorSpace.Linear)
                colour = colour.linear;
            colour *= currentSkyExposure;
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
            if (hostState == null || camera == null || environmentSettings == null
                || PlanarWaterReflection.IsReflectionCamera(camera))
            {
                return;
            }
            var colour = CurrentSkyHorizonBaseColour() * currentSkyExposure;
            colour.a = 1f;
            hostState.PrepareCamera(camera, ocean, colour);
        }
    }
}
