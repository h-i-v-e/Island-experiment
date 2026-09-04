using System;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class IslandGenerator
{
    private void UpdateMaterialTransforms(bool force = false)
    {
        var islandTransform = islandRuntime != null
            ? islandRuntime.transform
            : transform;
        var worldToLocal = islandTransform.worldToLocalMatrix;
        if (!force && hasAppliedWorldToLocal && appliedWorldToLocal == worldToLocal)
        {
            return;
        }
        appliedWorldToLocal = worldToLocal;
        hasAppliedWorldToLocal = true;
        terrainMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        terrainLod1Material?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        terrainLod2Material?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        grassMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        rockMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        riverMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        coastalWaterMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        treeWoodMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        treeLod1WoodMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        treeFoliageMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        treeLod0FoliageMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        reedMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        fernMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
    }

    private void EnsureWorldEnvironment()
    {
        if (worldEnvironment == null)
        {
            worldEnvironment = WorldEnvironmentController.FindOrCreate();
        }
        if (controlsWorldEnvironment)
        {
            worldEnvironment.SetFollowTarget(WorldEnvironmentFollowTarget());
        }
    }

    private Transform WorldEnvironmentFollowTarget()
    {
        return worldManaged ? environmentFollowTarget : Streaming.Target;
    }

    private void BindWorldEnvironment(float worldSize)
    {
        EnsureWorldEnvironment();
        if (worldManaged)
        {
            if (!worldEnvironment.IsInstalled)
            {
                throw new InvalidOperationException(
                    "The world environment must be initialized before an island is installed.");
            }
        }
        else if (controlsWorldEnvironment)
        {
            worldEnvironment.Install(
                skyDomeMaterial,
                seaMaterial,
                cloudWeatherTexture,
                ownsSeaNoiseTexture ? seaNoiseTexture : null,
                worldSize,
                worldSize * 2f * SeaHorizonOverlap,
                transform.TransformPoint(Vector3.up * SeaHeight).y,
                Rendering.ShowSea,
                Rendering.OceanWaveProfile != null
                    ? Rendering.OceanWaveProfile.ToRuntimeSettings()
                    : OceanWaveRuntimeSettings.Default,
                RenderSettings.sun);
            seaNoiseTexture = null;
            ownsSeaNoiseTexture = false;
        }

        environmentResourcesInstalled = true;
        skyDomeMaterial = worldEnvironment.SkyMaterial;
        seaMaterial = worldEnvironment.SeaMaterial;
        moonLight = worldEnvironment.MoonLight;
    }

    private void EnsureCloudWeatherTexture()
    {
        var combinedSeed = unchecked(Generation.Seed * 397) ^ Clouds.Seed;
        var resolution = Clouds.WeatherMapResolution;
        if (cloudWeatherTexture != null
            && appliedCloudSeed == combinedSeed
            && appliedCloudResolution == resolution)
        {
            return;
        }

        DestroyUnityObject(cloudWeatherTexture);
        cloudWeatherTexture = null;
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
            cloudWeatherTexture = CreateCloudWeatherTexture(
                rgba,
                resolution,
                resolution,
                "Rust Generated Cloud Weather Field");
            appliedCloudSeed = combinedSeed;
            appliedCloudResolution = resolution;
        }
        finally
        {
            MotuNative.ReleaseCloudWeatherMap(ref native);
        }
    }

    private static Texture2D CreateCloudWeatherTexture(
        byte[] rgba,
        int width,
        int height,
        string textureName)
    {
        var expectedLength = checked(width * height * 4);
        if (rgba == null || rgba.Length != expectedLength)
        {
            throw new ArgumentException(
                $"Cloud weather texture requires {expectedLength} base-mip RGBA bytes.",
                nameof(rgba));
        }
        var texture = new Texture2D(
            width,
            height,
            TextureFormat.RGBA32,
            true,
            true)
        {
            name = textureName,
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Repeat,
            anisoLevel = 1,
        };
        // The native map contains mip zero only. Upload that level explicitly,
        // then let Apply generate the remaining levels for trilinear filtering.
        texture.SetPixelData(rgba, 0);
        texture.Apply(true, true);
        return texture;
    }

    private void ApplyCloudSettings(float deltaTime)
    {
        if (skyDomeMaterial == null)
        {
            return;
        }
        EnsureCloudWeatherTexture();
        var windDirection = Clouds.WindDirection;
        if (windDirection.sqrMagnitude > 0.000001f)
        {
            windDirection.Normalize();
            var windTravel = windDirection
                * (Clouds.WindSpeedMetresPerSecond * Mathf.Max(deltaTime, 0f));
            cloudWindOffset += windTravel;
            cloudBroadWindOffset += windTravel * 0.18f;
        }
        var worldSize = Clouds.WorldSizeMetres;
        var broadWorldSize = worldSize * Clouds.BroadNoiseScale;
        cloudWindOffset.x = Mathf.Repeat(cloudWindOffset.x, worldSize);
        cloudWindOffset.y = Mathf.Repeat(cloudWindOffset.y, worldSize);
        cloudBroadWindOffset.x = Mathf.Repeat(
            cloudBroadWindOffset.x,
            broadWorldSize);
        cloudBroadWindOffset.y = Mathf.Repeat(
            cloudBroadWindOffset.y,
            broadWorldSize);
        var enabled = Clouds.Enabled
            && Clouds.Coverage > 0f
            && Clouds.Density > 0f
            && cloudWeatherTexture != null;

        Shader.SetGlobalTexture(CloudWeatherTextureId, cloudWeatherTexture);
        Shader.SetGlobalFloat(CloudEnabledId, enabled ? 1f : 0f);
        Shader.SetGlobalFloat("_MotuCloudCoverage", Clouds.Coverage);
        Shader.SetGlobalFloat("_MotuCloudDensity", Clouds.Density);
        Shader.SetGlobalFloat("_MotuCloudAltitude", Clouds.AltitudeMetres);
        Shader.SetGlobalFloat("_MotuCloudWorldSize", worldSize);
        Shader.SetGlobalVector(
            "_MotuCloudVolume",
            new Vector4(Clouds.VerticalThicknessMetres, 0f, 0f, 0f));
        Shader.SetGlobalVector(
            "_MotuCloudBroadNoise",
            new Vector4(
                Clouds.BroadNoiseScale,
                Clouds.BroadNoiseStrength,
                0f,
                0f));
        Shader.SetGlobalVector(
            "_MotuCloudDetailErosion",
            new Vector4(Clouds.DetailStrength, Clouds.ErosionStrength, 0f, 0f));
        Shader.SetGlobalVector(
            "_MotuCloudWindOffset",
            new Vector4(
                cloudWindOffset.x,
                cloudWindOffset.y,
                cloudBroadWindOffset.x,
                cloudBroadWindOffset.y));
        Shader.SetGlobalColor("_MotuCloudDayColor", Clouds.DayColour);
        Shader.SetGlobalColor("_MotuCloudSunsetColor", Clouds.SunsetColour);
        Shader.SetGlobalColor("_MotuCloudNightColor", Clouds.NightColour);
        Shader.SetGlobalFloat("_MotuCloudShadowStrength", Clouds.ShadowStrength);
        Shader.SetGlobalFloat(
            "_MotuCloudAmbientShadowStrength",
            Clouds.AmbientShadowStrength);
        Shader.SetGlobalFloat(
            "_MotuCloudCelestialStrength",
            Clouds.CelestialObscurationStrength);
        Shader.SetGlobalFloat(
            "_MotuCloudLowElevationFade",
            Clouds.LowElevationShadowFade);
    }

    private void ApplyLiveSettings()
    {
        if (appliedShowDistanceHaze != Rendering.ShowDistanceHaze
            || !appliedDistanceHazeColour.HasValue
            || appliedDistanceHazeColour.Value != Rendering.DistanceHazeColour
            || !Mathf.Approximately(
                appliedDistanceHazeDensity,
                Rendering.DistanceHazeDensity))
        {
            ApplyDistanceHazeSettings();
        }
        if (!appliedGrassColourA.HasValue
            || !appliedGrassColourB.HasValue
            || appliedGrassColourA.Value != Rendering.GrassColourA
            || appliedGrassColourB.Value != Rendering.GrassColourB
            || !Mathf.Approximately(
                appliedGrassColourNoiseWorldSize,
                Rendering.GrassColourNoiseWorldSizeMetres))
        {
            ApplyGrassColourSettings();
        }
        if (!Mathf.Approximately(appliedGrassBrightness, Rendering.GrassBrightness))
        {
            appliedGrassBrightness = Rendering.GrassBrightness;
            grassMaterial?.SetFloat("_GrassBrightness", appliedGrassBrightness);
        }
        if (appliedGrassWindDirection != Rendering.GrassWindDirection
            || !Mathf.Approximately(
                appliedGrassWindStrength,
                Rendering.GrassWindStrengthMetres)
            || !Mathf.Approximately(
                appliedGrassWindSpeed,
                Rendering.GrassWindSpeedMetresPerSecond)
            || !Mathf.Approximately(
                appliedGrassWindGustSize,
                Rendering.GrassWindGustSizeMetres)
            || !Mathf.Approximately(
                appliedGrassWindNormalStrength,
                Rendering.GrassWindNormalStrength))
        {
            ApplyGrassWindSettings();
        }
        if (appliedShowRivers != Rendering.ShowRivers)
        {
            appliedShowRivers = Rendering.ShowRivers;
            terrainStreamer?.SetRiversVisible(Rendering.ShowRivers);
        }
        if (appliedShowSea != Rendering.ShowSea)
        {
            appliedShowSea = Rendering.ShowSea;
            if (controlsWorldEnvironment)
            {
                worldEnvironment?.SetSeaVisible(Rendering.ShowSea);
            }
            if (coastalWaterObject != null)
            {
                coastalWaterObject.SetActive(Rendering.ShowSea);
            }
        }
        if (appliedShowGrass != Rendering.ShowGrass)
        {
            appliedShowGrass = Rendering.ShowGrass;
            terrainStreamer?.SetGrassVisible(Rendering.ShowGrass);
        }
        if (appliedShowRocks != Rendering.ShowRocks)
        {
            appliedShowRocks = Rendering.ShowRocks;
            terrainStreamer?.SetRocksVisible(Rendering.ShowRocks);
        }
        var snowline = Forest.SnowlineMetres;
        var terrainSnowlineChanged = terrainMaterial != null
            && terrainMaterial.HasProperty("_SnowLine")
            && !Mathf.Approximately(terrainMaterial.GetFloat("_SnowLine"), snowline);
        var grassSnowlineChanged = grassMaterial != null
            && grassMaterial.HasProperty("_SnowLine")
            && !Mathf.Approximately(grassMaterial.GetFloat("_SnowLine"), snowline);
        if (terrainSnowlineChanged || grassSnowlineChanged)
        {
            ApplySnowlineSettings();
        }
        if (appliedShowForests != Forest.ShowForests)
        {
            appliedShowForests = Forest.ShowForests;
            terrainStreamer?.SetForestsVisible(Forest.ShowForests);
        }
        if (appliedShowReeds != Reeds.ShowReeds)
        {
            appliedShowReeds = Reeds.ShowReeds;
            terrainStreamer?.SetReedsVisible(Reeds.ShowReeds);
        }
        if (appliedReedBaseColour != Reeds.BaseColour
            || appliedReedTipColour != Reeds.TipColour
            || !Mathf.Approximately(appliedReedWindStrength, Reeds.WindStrength))
        {
            appliedReedBaseColour = Reeds.BaseColour;
            appliedReedTipColour = Reeds.TipColour;
            appliedReedWindStrength = Reeds.WindStrength;
            reedMaterial?.SetColor("_BaseColor", Reeds.BaseColour);
            reedMaterial?.SetColor("_TipColor", Reeds.TipColour);
            reedMaterial?.SetFloat("_ReedWindMultiplier", Reeds.WindStrength);
        }
        if (appliedShowFerns != Ferns.ShowFerns)
        {
            appliedShowFerns = Ferns.ShowFerns;
            terrainStreamer?.SetFernsVisible(Ferns.ShowFerns);
        }
        if (appliedFernBaseColour != Ferns.BaseColour
            || appliedFernTipColour != Ferns.TipColour
            || !Mathf.Approximately(appliedFernWindStrength, Ferns.WindStrength))
        {
            appliedFernBaseColour = Ferns.BaseColour;
            appliedFernTipColour = Ferns.TipColour;
            appliedFernWindStrength = Ferns.WindStrength;
            fernMaterial?.SetColor("_BaseColor", Ferns.BaseColour);
            fernMaterial?.SetColor("_TipColor", Ferns.TipColour);
            fernMaterial?.SetFloat("_FernWindMultiplier", Ferns.WindStrength);
        }
        if (appliedShowMeshEdges != DebugSettings.ShowMeshEdges)
        {
            appliedShowMeshEdges = DebugSettings.ShowMeshEdges;
            terrainStreamer?.SetMeshEdgesVisible(DebugSettings.ShowMeshEdges);
        }
        if (appliedShowTreeMeshEdges != DebugSettings.ShowTreeMeshEdges)
        {
            appliedShowTreeMeshEdges = DebugSettings.ShowTreeMeshEdges;
            terrainStreamer?.SetTreeMeshEdgesVisible(DebugSettings.ShowTreeMeshEdges);
        }
        if (appliedWaterfallDebug != DebugSettings.ShowWaterfallFeet)
        {
            appliedWaterfallDebug = DebugSettings.ShowWaterfallFeet;
            terrainStreamer?.SetWaterfallFootDebug(DebugSettings.ShowWaterfallFeet);
        }
    }

    private void ApplyDistanceHazeSettings()
    {
        appliedShowDistanceHaze = Rendering.ShowDistanceHaze;
        appliedDistanceHazeColour = Rendering.DistanceHazeColour;
        appliedDistanceHazeDensity = Rendering.DistanceHazeDensity;
        if (!controlsWorldEnvironment)
        {
            return;
        }

        skyDomeMaterial?.SetColor(
            "_HorizonColor",
            CurrentAtmosphericHorizonBaseColour());
        skyDomeMaterial?.SetFloat(SkyExposureId, currentSkyExposure);
        RenderSettings.fog = Rendering.ShowDistanceHaze && firstPersonViewActive;
        if (!RenderSettings.fog)
        {
            return;
        }
        RenderSettings.fogMode = FogMode.ExponentialSquared;
        RenderSettings.fogColor = CurrentAtmosphericHorizonColour();
        RenderSettings.fogDensity = Rendering.DistanceHazeDensity;
    }

    private void UpdateSolarLighting(float deltaTime)
    {
        if (!solarClockInitialized)
        {
            solarTimeHours = Rendering.StartingSolarTimeHours;
            lunarPhase = Rendering.StartingMoonPhase;
            solarClockInitialized = true;
        }
        if (deltaTime > 0f)
        {
            var cycleSeconds = Rendering.SunCycleDurationMinutes * 60f;
            var clockRate = CelestialLighting.EvaluateClockRateMultiplier(
                solarTimeHours,
                Rendering.MidnightToNoonClockRateRatio);
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
            Rendering.SunLatitudeDegrees,
            Rendering.MiddaySunIntensity);
        currentSkyExposure = state.SkyExposure;
        currentNightStrength = state.NightStrength;
        treeFoliageMaterial?.SetFloat(NightStrengthId, currentNightStrength);
        var moonState = CelestialLighting.EvaluateMoon(
            solarTimeHours,
            Rendering.SunLatitudeDegrees,
            Rendering.MoonEquatorOffsetDegrees,
            lunarPhase,
            Rendering.FullMoonLightIntensity,
            state.LocalDirection.y);
        var worldSunDirection = transform.TransformDirection(
            state.LocalDirection).normalized;
        var worldMoonDirection = transform.TransformDirection(
            moonState.LocalDirection).normalized;
        var worldMoonLightDirection = transform.TransformDirection(
            moonState.LocalLightDirection).normalized;

        skyDomeMaterial?.SetVector(SunDirectionId, worldSunDirection);
        skyDomeMaterial?.SetColor(SunColourId, state.SunColour);
        skyDomeMaterial?.SetColor(
            "_HorizonColor",
            CurrentAtmosphericHorizonBaseColour());
        skyDomeMaterial?.SetFloat(SunVisibilityId, state.SunVisibility);
        skyDomeMaterial?.SetFloat(SunHaloStrengthId, state.SunHaloStrength);
        skyDomeMaterial?.SetVector(MoonDirectionId, worldMoonDirection);
        skyDomeMaterial?.SetVector(
            MoonLightDirectionId,
            worldMoonLightDirection);
        skyDomeMaterial?.SetFloat(MoonVisibilityId, moonState.Visibility);
        skyDomeMaterial?.SetFloat(SkyExposureId, currentSkyExposure);
        skyDomeMaterial?.SetFloat(StarVisibilityId, currentNightStrength);
        ApplyStarSettings();
        ApplyStarRotation(moonState.OrbitLatitudeDegrees);
        riverMaterial?.SetFloat(WaterSkyExposureId, currentSkyExposure);
        seaMaterial?.SetFloat(WaterSkyExposureId, currentSkyExposure);

        var sun = RenderSettings.sun;
        if (sun != null)
        {
            PositionDirectionalLight(sun, worldSunDirection);
            sun.color = state.SunColour;
            sun.intensity = state.SunIntensity;
            sun.enabled = state.SunIntensity > 0.0001f;
        }

        var moonShadowsActive = state.SunVisibility <= 0.0001f
            && state.SunIntensity <= 0.0001f
            && moonState.LightIntensity > 0.0001f;
        if (moonLight != null)
        {
            PositionDirectionalLight(moonLight, worldMoonDirection);
            moonLight.color = MoonLightColour;
            moonLight.intensity = moonState.LightIntensity;
            moonLight.enabled = moonShadowsActive;
        }

        var activeLightDirection = worldSunDirection;
        var activeLightColour = state.SunColour * state.SunIntensity;
        if (moonShadowsActive && moonLight != null)
        {
            activeLightDirection = worldMoonDirection;
            activeLightColour = MoonLightColour * moonState.LightIntensity;
            RenderSettings.sun = moonLight;
        }
        else if (sun != null)
        {
            RenderSettings.sun = sun;
        }

        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = state.AmbientColour;
        if (RenderSettings.fog)
        {
            RenderSettings.fogColor = CurrentAtmosphericHorizonColour();
        }

        grassMaterial?.SetVector("_GrassLightDirection", activeLightDirection);
        grassMaterial?.SetColor(
            "_GrassLightColor",
            activeLightColour);
        grassMaterial?.SetColor("_GrassAmbientColor", state.AmbientColour);
        Shader.SetGlobalVector(CloudLightDirectionId, activeLightDirection.normalized);
        Shader.SetGlobalFloat(
            CloudLightActiveId,
            activeLightColour.maxColorComponent > 0.0001f ? 1f : 0f);
        Shader.SetGlobalColor(CloudLightColourId, activeLightColour);
        Shader.SetGlobalColor("_MotuCloudAmbientColor", state.AmbientColour);
        Shader.SetGlobalFloat("_MotuCloudSunsetStrength", state.SunHaloStrength);
        Shader.SetGlobalFloat("_MotuCloudNightStrength", state.NightStrength);
    }

    private void ApplyStarSettings()
    {
        if (skyDomeMaterial == null)
        {
            return;
        }
        var starSeed = Mathf.Repeat(Generation.Seed * 0.61803398875f, 4096f);
        skyDomeMaterial.SetVector(
            StarSettingsId,
            new Vector4(
                Rendering.StarDensity,
                Rendering.StarBrightness,
                Rendering.StarSize,
                starSeed));
    }

    private void ApplyStarRotation(float orbitLatitudeDegrees)
    {
        if (skyDomeMaterial == null)
        {
            return;
        }
        var latitudeRadians = orbitLatitudeDegrees * Mathf.Deg2Rad;
        var localOrbitAxis = new Vector3(
            0f,
            Mathf.Sin(latitudeRadians),
            Mathf.Cos(latitudeRadians));
        var orbitAxis = transform.TransformDirection(localOrbitAxis).normalized;
        var orbitAngle = (Mathf.Repeat(solarTimeHours, 24f) - 6f)
            * Mathf.PI
            / 12f;
        // The shader rotates the observed direction back into the fixed star
        // field, so use the inverse of the sky's apparent orbital rotation.
        skyDomeMaterial.SetVector(
            StarRotationId,
            new Vector4(orbitAxis.x, orbitAxis.y, orbitAxis.z, -orbitAngle));
    }

    private void PositionDirectionalLight(Light light, Vector3 sourceDirection)
    {
        var centre = worldEnvironment != null
            ? worldEnvironment.AnchorPosition
            : transform.position;
        light.transform.position = centre
            + sourceDirection * Generation.WorldSizeMetres * 0.9f;
        var lightForward = -sourceDirection;
        var rotationUp = transform.TransformDirection(Vector3.forward);
        if (Mathf.Abs(Vector3.Dot(lightForward, rotationUp)) > 0.98f)
        {
            rotationUp = transform.TransformDirection(Vector3.right);
        }
        light.transform.rotation = Quaternion.LookRotation(lightForward, rotationUp);
    }

    private Color CurrentAtmosphericHorizonColour()
    {
        var colour = CurrentAtmosphericHorizonBaseColour() * currentSkyExposure;
        colour.a = Rendering.DistanceHazeColour.a;
        return colour;
    }

    private Color CurrentAtmosphericHorizonBaseColour()
    {
        var colour = Color.Lerp(
            Rendering.DistanceHazeColour,
            NightHazeColour,
            currentNightStrength);
        colour.a = Rendering.DistanceHazeColour.a;
        return colour;
    }

    public void SetFirstPersonViewActive(bool active)
    {
        if (!controlsWorldEnvironment)
        {
            return;
        }
        if (firstPersonViewActive == active)
        {
            return;
        }
        firstPersonViewActive = active;
        ApplyDistanceHazeSettings();
    }

}
