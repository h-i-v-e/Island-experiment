using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Rendering;
using Debug = UnityEngine.Debug;

public sealed partial class IslandGenerator : MonoBehaviour, IWorldSurfaceQuery
{
    private const float SeaHeight = 0f;
    private const float SeaHorizonOverlap = 1.05f;
    private const float CoastalWaterVerticalOffset = 0.015f;
    private const float UnityPlaneSizeMetres = 10f;
    private const float SkyDomeSkirtDepthRatio = 0.25f;
    private const float ValidationWorldSize = 2000f;
    private const int SurfaceMapDimension = 2048;
    private const int CliffNoiseDimension = 64;
    private const int CliffNoiseLatticePeriod = 16;
    private const int RiverNoiseDimension = 256;
    private const int RiverNoiseLatticePeriod = 32;
    private const int GrassPatchNoiseDimension = 256;
    private const int GrassPatchNoiseLatticePeriod = 64;
    private const int GrassColourNoiseLatticePeriod = 8;
    private const float RockPatchNoiseDetailScale = 8f;
    private const float SunDiscAngularRadiusDegrees = 0.7f;
    private const float MoonDiscAngularRadiusDegrees = 0.68f;
    private const float LunarSynodicPeriodDays = 29.53059f;
    private const float NightSkyExposure = 0.045f;
    private static readonly Color MiddaySunColour = new Color(1f, 0.94f, 0.82f, 1f);
    private static readonly Color SunsetSunColour = new Color(1f, 0.20f, 0.035f, 1f);
    private static readonly Color DayAmbientColour = new Color(0.42f, 0.46f, 0.52f, 1f);
    private static readonly Color TwilightAmbientColour = new Color(0.08f, 0.15f, 0.30f, 1f);
    private static readonly Color NightAmbientColour = new Color(0.012f, 0.025f, 0.065f, 1f);
    private static readonly Color SunsetSunHaloColour = new Color(0.85f, 0.05f, 0.01f, 1f);
    private static readonly Color NightHazeColour = new Color(0.08f, 0.14f, 0.28f, 1f);
    private static readonly Color MoonDiscColour = new Color(0.78f, 0.84f, 0.92f, 1f);
    private static readonly Color MoonDarkColour = new Color(0.012f, 0.018f, 0.035f, 1f);
    private static readonly Color MoonLightColour = new Color(0.48f, 0.62f, 0.90f, 1f);
    private static readonly int IslandWorldToLocalId = Shader.PropertyToID(
        "_IslandWorldToLocal");
    private static readonly int SunDirectionId = Shader.PropertyToID("_SunDirection");
    private static readonly int SunColourId = Shader.PropertyToID("_SunColor");
    private static readonly int SunVisibilityId = Shader.PropertyToID("_SunVisibility");
    private static readonly int SunDiscCosRadiusId = Shader.PropertyToID("_SunDiscCosRadius");
    private static readonly int SunHaloColourId = Shader.PropertyToID("_SunHaloColor");
    private static readonly int SunHaloStrengthId = Shader.PropertyToID("_SunHaloStrength");
    private static readonly int SkyExposureId = Shader.PropertyToID("_SkyExposure");
    private static readonly int WaterSkyExposureId = Shader.PropertyToID(
        "_WaterSkyExposure");
    private static readonly int NightStrengthId = Shader.PropertyToID(
        "_MotuNightStrength");
    private static readonly int MoonDirectionId = Shader.PropertyToID("_MoonDirection");
    private static readonly int MoonLightDirectionId = Shader.PropertyToID(
        "_MoonLightDirection");
    private static readonly int MoonColourId = Shader.PropertyToID("_MoonColor");
    private static readonly int MoonDarkColourId = Shader.PropertyToID("_MoonDarkColor");
    private static readonly int MoonVisibilityId = Shader.PropertyToID("_MoonVisibility");
    private static readonly int MoonDiscCosRadiusId = Shader.PropertyToID(
        "_MoonDiscCosRadius");
    private static readonly int StarSettingsId = Shader.PropertyToID("_StarSettings");
    private static readonly int StarVisibilityId = Shader.PropertyToID("_StarVisibility");
    private static readonly int StarRotationId = Shader.PropertyToID("_StarRotation");
    private static readonly int CloudWeatherTextureId = Shader.PropertyToID(
        "_MotuCloudWeatherTex");
    private static readonly int CloudEnabledId = Shader.PropertyToID("_MotuCloudEnabled");
    private static readonly int CloudLightDirectionId = Shader.PropertyToID(
        "_MotuCloudLightDirection");
    private static readonly int CloudLightActiveId = Shader.PropertyToID(
        "_MotuCloudLightActive");
    private static readonly int CloudLightColourId = Shader.PropertyToID(
        "_MotuCloudLightColor");

    [Header("Shared Configuration")]
    [Tooltip("Optional shared settings asset. Inline values below remain the compatibility fallback.")]
    [SerializeField] private IslandConfiguration configuration;

    [Header("Lifecycle and Generation (Inline Fallback)")]
    [SerializeField] private IslandGenerationSettings generation = new IslandGenerationSettings();

    [Header("Rivers")]
    [SerializeField] private IslandRiverSettings rivers = new IslandRiverSettings();

    [Header("Forest")]
    [SerializeField] private IslandForestSettings forest = new IslandForestSettings();

    [Header("Riverbank Reeds and Rushes")]
    [SerializeField] private IslandReedSettings reeds = new IslandReedSettings();

    [Header("Tree Trunk Ferns")]
    [SerializeField] private IslandFernSettings ferns = new IslandFernSettings();

    [Header("Streaming")]
    [SerializeField] private IslandStreamingSettings streaming = new IslandStreamingSettings();

    [Header("Clouds")]
    [SerializeField] private IslandCloudSettings clouds = new IslandCloudSettings();

    [Header("Rendering and Texture Overrides")]
    [SerializeField] private IslandRenderingSettings rendering = new IslandRenderingSettings();

    [Header("Decoration Asset Libraries")]
    [SerializeField] private IslandDecorationSettings decorations = new IslandDecorationSettings();

    [Header("Debug")]
    [SerializeField] private IslandDebugSettings debugSettings = new IslandDebugSettings();

    private NativeIslandHandle islandHandle;
    private IslandRuntime islandRuntime;
    private TerrainTileStreamer terrainStreamer;
    private GameObject runtimeRoot;
    private WorldEnvironmentController worldEnvironment;
    private Material skyDomeMaterial;
    private Light moonLight;
    private Material terrainMaterial;
    private Material terrainLod1Material;
    private Material terrainLod2Material;
    private Material grassMaterial;
    private Texture2D terrainNormalTexture;
    private Texture2D terrainOcclusionTexture;
    private Texture2D seaMaskTexture;
    private Texture3D cliffNoiseTexture;
    private Texture2D riverNoiseTexture;
    private Texture2D seaNoiseTexture;
    private Texture2D grassPatchNoiseTexture;
    private Texture2D cloudWeatherTexture;
    private TerrainMaterialTextureArrays terrainMaterialTextures;
    private Material rockMaterial;
    private Material riverMaterial;
    private Material seaMaterial;
    private Material coastalWaterMaterial;
    private GameObject coastalWaterObject;
    private Material meshEdgeMaterial;
    private Material treeWoodMaterial;
    private Material treeLod1WoodMaterial;
    private Material treeFoliageMaterial;
    private Material treeLod0FoliageMaterial;
    private Material reedMaterial;
    private Material fernMaterial;
    private string status = "Ready";
    private CancellationTokenSource generationCancellation;
    private Stopwatch generationTimer;
    private bool generationInProgress;
    private bool isDestroyed;
    private bool hasStarted;
    private bool ownsCliffNoiseTexture;
    private bool ownsRiverNoiseTexture;
    private bool ownsSeaNoiseTexture;
    private bool ownsGrassPatchNoiseTexture;
    private bool environmentResourcesInstalled;
    private int appliedCloudSeed = int.MinValue;
    private int appliedCloudResolution;
    private Vector2 cloudWindOffset;
    private Vector2 cloudBroadWindOffset;
    private bool? appliedShowRivers;
    private bool? appliedShowSea;
    private bool? appliedShowGrass;
    private bool? appliedShowRocks;
    private bool? appliedShowForests;
    private bool? appliedShowReeds;
    private Color? appliedReedBaseColour;
    private Color? appliedReedTipColour;
    private float appliedReedWindStrength = float.NaN;
    private bool? appliedShowFerns;
    private Color? appliedFernBaseColour;
    private Color? appliedFernTipColour;
    private float appliedFernWindStrength = float.NaN;
    private bool? appliedShowMeshEdges;
    private bool? appliedShowTreeMeshEdges;
    private bool? appliedWaterfallDebug;
    private Color? appliedGrassColourA;
    private Color? appliedGrassColourB;
    private float appliedGrassColourNoiseWorldSize = float.NaN;
    private float appliedGrassBrightness = float.NaN;
    private Vector2 appliedGrassWindDirection = new Vector2(float.NaN, float.NaN);
    private float appliedGrassWindStrength = float.NaN;
    private float appliedGrassWindSpeed = float.NaN;
    private float appliedGrassWindGustSize = float.NaN;
    private float appliedGrassWindNormalStrength = float.NaN;
    private bool? appliedShowDistanceHaze;
    private Color? appliedDistanceHazeColour;
    private float appliedDistanceHazeDensity = float.NaN;
    private bool firstPersonViewActive;
    private bool worldManaged;
    private bool controlsWorldEnvironment = true;
    private Transform environmentFollowTarget;
    private bool solarClockInitialized;
    private float solarTimeHours;
    private float lunarPhase;
    private float currentSkyExposure = 1f;
    private float currentNightStrength;
    private Matrix4x4 appliedWorldToLocal;
    private bool hasAppliedWorldToLocal;

    public bool IsGenerating => generationInProgress;
    public bool HasActiveRuntime => islandRuntime != null
        && islandRuntime.State == IslandRuntimeState.Active;
    public bool HasRuntime => islandRuntime != null
        && islandRuntime.State != IslandRuntimeState.Disposed;
    internal bool HasInstalledWorldEnvironment => worldEnvironment != null
        && worldEnvironment.SkyMaterial != null
        && worldEnvironment.SeaMaterial != null;
    internal IslandRuntime Runtime => islandRuntime;
    public string Status => status;
    public IslandConfiguration Configuration => configuration;
    public float WorldSizeMetres => Generation.WorldSizeMetres;
    public IslandGenerationSettings Generation =>
        configuration != null ? configuration.Generation : generation;
    public IslandRiverSettings Rivers =>
        configuration != null ? configuration.Rivers : rivers;
    public IslandForestSettings Forest =>
        configuration != null ? configuration.Forest : forest;
    public IslandReedSettings Reeds =>
        configuration != null ? configuration.Reeds : reeds;
    public IslandFernSettings Ferns =>
        configuration != null ? configuration.Ferns : ferns;
    public IslandStreamingSettings Streaming => streaming;
    public IslandCloudSettings Clouds => clouds;
    public IslandRenderingSettings Rendering =>
        configuration != null ? configuration.Rendering : rendering;
    public IslandDecorationSettings Decorations =>
        configuration != null ? configuration.Decorations : decorations;
    public IslandDebugSettings DebugSettings =>
        configuration != null ? configuration.DebugSettings : debugSettings;

    private void Start()
    {
        hasStarted = true;
        if (!worldManaged && Generation.GenerateOnStart)
        {
            Generate();
        }
    }

    private void OnEnable()
    {
        Camera.onPreCull += PrepareCameraRender;
        if (controlsWorldEnvironment && Application.isPlaying)
        {
            EnsureWorldEnvironment();
        }
        EnsureActiveCameraDepthTextures();
        if (controlsWorldEnvironment)
        {
            ApplyDistanceHazeSettings();
            UpdateSolarLighting(0f);
        }
        if (!worldManaged
            && hasStarted
            && Generation.GenerateOnStart
            && terrainStreamer == null)
        {
            Generate();
        }
    }

    private void OnDisable()
    {
        Camera.onPreCull -= PrepareCameraRender;
        if (controlsWorldEnvironment)
        {
            RenderSettings.fog = false;
        }
        generationCancellation?.Cancel();
        ClearGeneratedContent();
    }

    private void PrepareCameraRender(Camera camera)
    {
        if (!controlsWorldEnvironment)
        {
            return;
        }
        EnsureCameraDepthTexture(camera);
        worldEnvironment?.BindReflectionCamera(camera);
        ApplyAtmosphericCameraClearColour(camera);
    }

    private void ApplyAtmosphericCameraClearColour(Camera camera)
    {
        if (camera == null)
        {
            return;
        }
        var clearColour = CurrentAtmosphericHorizonColour();
        clearColour.a = 1f;
        camera.backgroundColor = clearColour;
    }

    private static void EnsureCameraDepthTexture(Camera camera)
    {
        if (camera != null && !PlanarWaterReflection.IsReflectionCamera(camera))
        {
            camera.depthTextureMode |= DepthTextureMode.Depth;
        }
    }

    private static void EnsureActiveCameraDepthTextures()
    {
        foreach (var camera in Camera.allCameras)
        {
            EnsureCameraDepthTexture(camera);
        }
    }

    private void Update()
    {
        var meshEdgeKey = DebugSettings.ToggleMeshEdgesKey;
        if (meshEdgeKey != KeyCode.None && Input.GetKeyDown(meshEdgeKey))
        {
            DebugSettings.ShowMeshEdges = !DebugSettings.ShowMeshEdges;
        }
        var treeMeshEdgeKey = DebugSettings.ToggleTreeMeshEdgesKey;
        if (treeMeshEdgeKey != KeyCode.None && Input.GetKeyDown(treeMeshEdgeKey))
        {
            DebugSettings.ShowTreeMeshEdges = !DebugSettings.ShowTreeMeshEdges;
        }
        var frameRateKey = DebugSettings.ToggleFrameRateKey;
        if (frameRateKey != KeyCode.None && Input.GetKeyDown(frameRateKey))
        {
            DebugSettings.ShowFrameRate = !DebugSettings.ShowFrameRate;
        }
        UpdateMaterialTransforms();
        ApplyLiveSettings();
        if (controlsWorldEnvironment)
        {
            UpdateSolarLighting(Time.unscaledDeltaTime);
            ApplyCloudSettings(Time.unscaledDeltaTime);
            worldEnvironment?.SetFollowTarget(WorldEnvironmentFollowTarget());
        }
        if (!worldManaged && terrainStreamer != null && Streaming.Target != null)
        {
            terrainStreamer.SetPlayerPosition(Streaming.Target.position);
        }
    }

    private void OnValidate()
    {
        if (!HasSupportedTransform())
        {
            Debug.LogWarning(
                "IslandGenerator currently requires a unit scale and rotation around the Y axis only.",
                this);
        }
    }

    private void OnDestroy()
    {
        isDestroyed = true;
        generationCancellation?.Cancel();
        ClearGeneratedContent();
        DestroyRuntimeMaterials();
    }

    private bool HasSupportedTransform()
    {
        var scale = transform.lossyScale;
        return Mathf.Approximately(scale.x, 1f)
            && Mathf.Approximately(scale.y, 1f)
            && Mathf.Approximately(scale.z, 1f)
            && Vector3.Dot(transform.up, Vector3.up) > 0.99999f;
    }

    public async void Generate()
    {
        await GenerateAsync(null, CancellationToken.None);
    }

    internal async Task<bool> GenerateAsync(
        IslandDescriptor? descriptorOverride,
        CancellationToken externalCancellation,
        float installationFrameBudgetMilliseconds = 4f)
    {
        if (generationInProgress)
        {
            return false;
        }
        if (!HasSupportedTransform())
        {
            status = "Island transform must use unit scale and Y-axis rotation only.";
            Debug.LogError(status, this);
            return false;
        }

        status = "Generating island on CPU in background...";
        generationInProgress = true;
        generationTimer = Stopwatch.StartNew();
        var cancellation = externalCancellation.CanBeCanceled
            ? CancellationTokenSource.CreateLinkedTokenSource(externalCancellation)
            : new CancellationTokenSource();
        generationCancellation = cancellation;
        IslandPreparedData prepared = null;
        var installationStarted = false;
        var islandSeed = Generation.Seed;
        var worldSize = Generation.WorldSizeMetres;
        var options = Generation.ToNativeOptions(Rivers);
        var forestOptions = Generation.ToNativeForestOptions(Forest);
        var reedOptions = Generation.ToNativeReedOptions(Reeds);
        var fernOptions = Generation.ToNativeFernOptions(Ferns);
        var materialColours = Rendering.SelectMaterialColours(islandSeed);
        var materialTextureResolution = Rendering.MaterialTextureResolution;
        var descriptor = descriptorOverride
            ?? IslandDescriptor.Origin(islandSeed, worldSize, transform);
        var request = new IslandGenerationRequest(
            descriptor,
            options,
            forestOptions,
            reedOptions,
            fernOptions,
            worldSize,
            materialColours,
            materialTextureResolution,
            Generation.UseSnapshotCache,
            Generation.SnapshotCacheBudgetBytes);
        var installationBudget = new UnityFrameBudget(
            installationFrameBudgetMilliseconds);

        try
        {
            prepared = await IslandGenerationWorker.GenerateAsync(
                request,
                cancellation.Token);
            cancellation.Token.ThrowIfCancellationRequested();
            if (isDestroyed || !isActiveAndEnabled)
            {
                return false;
            }

            status = "Uploading generated island...";
            installationStarted = true;
            ClearGeneratedContent();
            DestroyRuntimeMaterials();
            BuildRuntimeMaterials(prepared.materialTextures);
            await installationBudget.YieldIfExceededAsync(cancellation.Token);
            islandRuntime = IslandRuntime.Create(descriptor, transform);
            runtimeRoot = islandRuntime.gameObject;
            TransferMaterialOwnershipToRuntime();
            UpdateMaterialTransforms(true);
            islandHandle = prepared.TakeHandle();
            islandRuntime.AdoptNativeHandle(islandHandle);

            CreateSurfaceTextures(prepared.surfaceMaps);
            await installationBudget.YieldIfExceededAsync(cancellation.Token);
            CreateSeaMaskTexture(prepared.seaMask);
            await installationBudget.YieldIfExceededAsync(
                cancellation.Token,
                true);

            BindWorldEnvironment(worldSize);
            await installationBudget.YieldIfExceededAsync(cancellation.Token);

            CreateCoastalWaterOverlay(worldSize);
            islandRuntime.SetCoastalWaterObject(coastalWaterObject);
            islandRuntime.SetCoastalWaveMask(
                worldEnvironment,
                seaMaskTexture,
                worldSize);
            if (controlsWorldEnvironment)
            {
                UpdateSolarLighting(0f);
            }
            var terrainRoot = new GameObject("Terrain Tiles");
            terrainRoot.transform.SetParent(runtimeRoot.transform, false);
            terrainStreamer = terrainRoot.AddComponent<TerrainTileStreamer>();
            islandRuntime.SetTerrainStreamer(terrainStreamer);
            await terrainStreamer.InitializeAsync(
                islandHandle.Value,
                terrainMaterial,
                terrainLod1Material,
                terrainLod2Material,
                grassMaterial,
                rockMaterial,
                treeWoodMaterial,
                treeLod1WoodMaterial,
                treeFoliageMaterial,
                treeLod0FoliageMaterial,
                reedMaterial,
                fernMaterial,
                riverMaterial,
                meshEdgeMaterial,
                worldSize,
                prepared.overviewTiles,
                prepared.riverTiles,
                prepared.riverRockTiles,
                prepared.forest,
                prepared.reedTiles,
                prepared.fernTiles,
                prepared.waterfallFeet,
                prepared.colliderHeightMap,
                Rendering.ShowRivers,
                Rendering.ShowGrass,
                Rendering.ShowRocks,
                Forest.ShowForests,
                Reeds.ShowReeds,
                Ferns.ShowFerns,
                cancellation.Token,
                installationBudget);
            terrainStreamer.SetWaterfallFootDebug(DebugSettings.ShowWaterfallFeet);
            islandRuntime.Activate();

            ResetAppliedLiveSettings();
            ApplyLiveSettings();
            if (Streaming.Target != null)
            {
                terrainStreamer.SetPlayerPosition(Streaming.Target.position);
            }

            generationTimer.Stop();
            status = string.Format(
                CultureInfo.InvariantCulture,
                "{0} | Seed {1} | 64 LOD 2 tiles | {2:N0} vertices | {3:N0} triangles | {4:F2}s",
                prepared.loadedFromSnapshot ? "Disk cache" : "CPU",
                islandSeed,
                terrainStreamer.BaseVertexCount,
                terrainStreamer.BaseTriangleCount,
                generationTimer.Elapsed.TotalSeconds);
            status += " | shared 2048 terrain shading map";
            if (prepared.materialTextures.loadedFromCache)
            {
                status += " | cached material maps";
            }
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | {0:N0} waterfall feet / 32 pooled fog volumes",
                terrainStreamer.WaterfallFootCount);
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | 3x3 hidden LOD 1 terrain colliders (129x129 samples each) | {0:F1} km square",
                worldSize / 1000f);
            return true;
        }
        catch (OperationCanceledException)
        {
            if (installationStarted)
            {
                ClearGeneratedContent();
                DestroyRuntimeMaterials();
            }
            if (!isDestroyed)
            {
                status = "Generation cancelled.";
            }
            return false;
        }
        catch (Exception exception)
        {
            status = exception.Message;
            Debug.LogException(exception);
            if (installationStarted)
            {
                islandRuntime?.MarkFailed();
                ClearGeneratedContent();
                DestroyRuntimeMaterials();
            }
            return false;
        }
        finally
        {
            prepared?.Dispose();
            if (ReferenceEquals(generationCancellation, cancellation))
            {
                generationCancellation = null;
                generationInProgress = false;
                generationTimer = null;
            }
            cancellation.Dispose();
        }
    }

    public void Regenerate(int seed)
    {
        Generation.Seed = seed;
        Generate();
    }

    public void Clear()
    {
        generationCancellation?.Cancel();
        ClearGeneratedContent();
        status = "Cleared";
    }

    public void SetStreamingTarget(Transform target)
    {
        Streaming.Target = target;
        if (controlsWorldEnvironment
            && !worldManaged
            && (Application.isPlaying || worldEnvironment != null))
        {
            EnsureWorldEnvironment();
            worldEnvironment.SetFollowTarget(target);
        }
        if (terrainStreamer != null && target != null)
        {
            terrainStreamer.SetPlayerPosition(target.position);
        }
        else if (target == null)
        {
            terrainStreamer?.ClearPlayerFocus();
        }
    }

    public void PrepareStreamingAt(Vector3 worldPosition)
    {
        terrainStreamer?.SetPlayerPosition(worldPosition);
    }

    public void ClearStreamingFocus()
    {
        terrainStreamer?.ClearPlayerFocus();
    }

    internal void ConfigureWorldManagement()
    {
        if (generationInProgress || islandRuntime != null)
        {
            throw new InvalidOperationException(
                "World management must be configured before island generation starts.");
        }
        worldManaged = true;
        controlsWorldEnvironment = false;
        environmentFollowTarget = Streaming.Target;
    }

    internal void ApplyIslandProfile(
        int islandSeed,
        IslandParameterVariationSettings variation)
    {
        if (generationInProgress || islandRuntime != null)
        {
            throw new InvalidOperationException(
                "Island parameters must be selected before generation starts.");
        }
        if (configuration != null)
        {
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Generation),
                generation);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Rivers),
                rivers);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Forest),
                forest);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Reeds),
                reeds);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Ferns),
                ferns);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Rendering),
                rendering);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.Decorations),
                decorations);
            JsonUtility.FromJsonOverwrite(
                JsonUtility.ToJson(configuration.DebugSettings),
                debugSettings);
            configuration = null;
        }
        generation.Seed = islandSeed;
        generation.ApplyDeterministicVariation(islandSeed, variation);
    }

    internal void SetRuntimeDormant(bool dormant)
    {
        if (islandRuntime == null
            || islandRuntime.State == IslandRuntimeState.Disposed
            || islandRuntime.State == IslandRuntimeState.Installing)
        {
            return;
        }
        if (dormant)
        {
            terrainStreamer?.ClearPlayerFocus();
            Streaming.Target = null;
        }
        islandRuntime.SetDormant(dormant);
    }

    internal void SetWorldEnvironmentFollowTarget(Transform target)
    {
        if (!controlsWorldEnvironment)
        {
            return;
        }
        environmentFollowTarget = target;
        EnsureWorldEnvironment();
        worldEnvironment.SetFollowTarget(target);
    }

    internal void SyncSharedWorldLighting(WorldEnvironmentController environment)
    {
        if (environment == null)
        {
            return;
        }
        currentSkyExposure = environment.SkyExposure;
        currentNightStrength = environment.NightStrength;
        riverMaterial?.SetFloat(WaterSkyExposureId, currentSkyExposure);
        treeFoliageMaterial?.SetFloat(NightStrengthId, currentNightStrength);
        var light = RenderSettings.sun;
        var direction = light != null ? -light.transform.forward : Vector3.down;
        var colour = light != null ? light.color * light.intensity : Color.black;
        grassMaterial?.SetVector("_GrassLightDirection", direction);
        grassMaterial?.SetColor("_GrassLightColor", colour);
        grassMaterial?.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
    }

    public bool TryRaycastOverview(Ray worldRay, out Vector3 worldPoint)
    {
        if (terrainStreamer != null)
        {
            return terrainStreamer.TryRaycastOverview(worldRay, out worldPoint);
        }
        worldPoint = default;
        return false;
    }

    public bool TrySnapToTerrain(Vector3 approximateWorldPoint, out Vector3 worldPoint)
    {
        if (terrainStreamer != null)
        {
            return terrainStreamer.TrySnapToCurrentCollider(
                approximateWorldPoint,
                out worldPoint);
        }
        worldPoint = approximateWorldPoint;
        return false;
    }

    public float GetTerrainOrSeaHeight(Vector3 approximateWorldPoint)
    {
        var surfaceHeight = transform.TransformPoint(Vector3.up * SeaHeight).y;
        if (TrySnapToTerrain(approximateWorldPoint, out var terrainPoint))
        {
            surfaceHeight = Mathf.Max(surfaceHeight, terrainPoint.y);
        }
        return surfaceHeight;
    }

    public void ConfigureSceneReferences(
        Transform streamingTarget,
        Light sunlight,
        Material terrainTemplate,
        Material grassTemplate,
        Material riverTemplate,
        Material seaTemplate,
        Material rockTemplate,
        Material treeWoodTemplate = null,
        Material treeFoliageTemplate = null)
    {
        Streaming.Target = streamingTarget;
        if (controlsWorldEnvironment
            && !worldManaged
            && (Application.isPlaying || worldEnvironment != null))
        {
            EnsureWorldEnvironment();
            worldEnvironment.SetFollowTarget(streamingTarget);
        }
        Rendering.Sunlight = sunlight;
        Rendering.AssignMaterialTemplates(
            terrainTemplate,
            grassTemplate,
            riverTemplate,
            seaTemplate,
            rockTemplate,
            treeWoodTemplate,
            treeFoliageTemplate);
    }

    private void ClearGeneratedContent()
    {
        if (islandRuntime != null)
        {
            islandRuntime.Dispose();
            islandRuntime = null;
            ClearIslandRuntimeAliases();
            ResetAppliedLiveSettings();
            return;
        }
        if (terrainStreamer != null)
        {
            terrainStreamer.Dispose();
            DestroyUnityObject(terrainStreamer.gameObject);
            terrainStreamer = null;
        }
        DestroyUnityObject(runtimeRoot);
        runtimeRoot = null;
        coastalWaterObject = null;
        terrainMaterial?.SetTexture("_WorldNormal", null);
        terrainMaterial?.SetTexture("_Occlusion", null);
        terrainLod1Material?.SetTexture("_WorldNormal", null);
        terrainLod1Material?.SetTexture("_Occlusion", null);
        terrainLod2Material?.SetTexture("_WorldNormal", null);
        terrainLod2Material?.SetTexture("_Occlusion", null);
        coastalWaterMaterial?.SetTexture("_SeaMask", null);
        DestroyUnityObject(terrainNormalTexture);
        DestroyUnityObject(terrainOcclusionTexture);
        DestroyUnityObject(seaMaskTexture);
        terrainNormalTexture = null;
        terrainOcclusionTexture = null;
        seaMaskTexture = null;

        islandHandle?.Dispose();
        islandHandle = null;
        ResetAppliedLiveSettings();
    }

    private void ClearIslandRuntimeAliases()
    {
        islandHandle = null;
        terrainStreamer = null;
        runtimeRoot = null;
        coastalWaterObject = null;
        terrainMaterialTextures = null;
        terrainMaterial = null;
        terrainLod1Material = null;
        terrainLod2Material = null;
        grassMaterial = null;
        rockMaterial = null;
        treeWoodMaterial = null;
        treeLod1WoodMaterial = null;
        treeFoliageMaterial = null;
        treeLod0FoliageMaterial = null;
        reedMaterial = null;
        fernMaterial = null;
        riverMaterial = null;
        coastalWaterMaterial = null;
        meshEdgeMaterial = null;
        terrainNormalTexture = null;
        terrainOcclusionTexture = null;
        seaMaskTexture = null;
        cliffNoiseTexture = null;
        riverNoiseTexture = null;
        grassPatchNoiseTexture = null;
        ownsCliffNoiseTexture = false;
        ownsRiverNoiseTexture = false;
        ownsGrassPatchNoiseTexture = false;
    }


}
