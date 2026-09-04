using System;
using System.Collections.Generic;
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
    [Tooltip("Required asset containing this island's generation and rendering profile.")]
    [SerializeField] private IslandConfiguration configuration;

    [Header("Streaming")]
    [SerializeField] private IslandStreamingSettings streaming = new IslandStreamingSettings();

    [HideInInspector] [SerializeField]
    private IslandCloudSettings clouds = new IslandCloudSettings();

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
    private readonly IslandGenerationLifecycle generationLifecycle =
        new IslandGenerationLifecycle();
    private IslandGenerationProfile activeProfile;
    private IslandRuntimeLoop runtimeLoop;
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

    public bool IsGenerating => generationLifecycle.IsGenerating;
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
    private IslandGenerationProfile Profile =>
        activeProfile ??= IslandGenerationProfile.FromConfiguration(configuration);
    public IslandGenerationSettings Generation => Profile.Generation;
    public IslandRiverSettings Rivers => Profile.Rivers;
    public IslandForestSettings Forest => Profile.Forest;
    public IslandReedSettings Reeds => Profile.Reeds;
    public IslandFernSettings Ferns => Profile.Ferns;
    public IslandStreamingSettings Streaming => streaming;
    public IslandCloudSettings Clouds => clouds;
    public IslandRenderingSettings Rendering => Profile.Rendering;
    public IslandDecorationSettings Decorations => Profile.Decorations;
    public IslandDebugSettings DebugSettings => Profile.DebugSettings;
    private IslandRuntimeLoop RuntimeLoop =>
        runtimeLoop ??= new IslandRuntimeLoop(this);

    private void Start()
    {
        RuntimeLoop.Start();
    }

    private void OnEnable()
    {
        RuntimeLoop.Enable();
    }

    private void OnDisable()
    {
        RuntimeLoop.Disable();
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

    private void Update()
    {
        RuntimeLoop.Update();
    }

    private void OnValidate()
    {
        if (!generationLifecycle.IsGenerating && islandRuntime == null)
        {
            activeProfile = null;
        }
        if (!HasSupportedTransform())
        {
            Debug.LogWarning(
                "IslandGenerator currently requires a unit scale and rotation around the Y axis only.",
                this);
        }
    }

    private void OnDestroy()
    {
        generationLifecycle.MarkDestroyed();
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
        await GenerateAsync((IslandDescriptor?)null, CancellationToken.None);
    }

    internal async Task<bool> GenerateAsync(
        IslandDescriptor? descriptorOverride,
        CancellationToken externalCancellation,
        float installationFrameBudgetMilliseconds = 4f)
    {
        var descriptor = descriptorOverride
            ?? IslandDescriptor.Origin(
                Generation.Seed,
                Generation.WorldSizeMetres,
                transform);
        var request = CreateGenerationRequest(descriptor);
        return await GenerateAsync(
            request,
            externalCancellation,
            installationFrameBudgetMilliseconds);
    }

    internal async Task<bool> GenerateAsync(
        IslandGenerationRequest request,
        CancellationToken externalCancellation,
        float installationFrameBudgetMilliseconds = 4f)
    {
        if (request == null) throw new ArgumentNullException(nameof(request));
        if (generationLifecycle.IsGenerating)
        {
            return false;
        }
        if (!HasSupportedTransform())
        {
            status = "Island transform must use unit scale and Y-axis rotation only.";
            Debug.LogError(status, this);
            return false;
        }

        request.ApplyProfileTo(this);
        if (!generationLifecycle.TryBegin(externalCancellation, out var cancellation))
        {
            return false;
        }
        status = "Generating island on CPU in background...";
        IslandPreparedData prepared = null;
        var installationStarted = false;
        var islandSeed = request.RandomSeed;
        var worldSize = request.WorldSizeMetres;
        var descriptor = request.Descriptor;
        var installationBudget = new UnityFrameBudget(
            installationFrameBudgetMilliseconds);

        try
        {
            prepared = await IslandGenerationWorker.GenerateAsync(
                request,
                cancellation.Token);
            cancellation.Token.ThrowIfCancellationRequested();
            if (generationLifecycle.IsDestroyed || !isActiveAndEnabled)
            {
                return false;
            }

            status = "Uploading generated island...";
            installationStarted = true;
            var runtimeInstaller = new IslandRuntimeInstaller(this);
            await runtimeInstaller.InstallAsync(
                prepared,
                descriptor,
                worldSize,
                cancellation.Token,
                installationBudget);

            generationLifecycle.StopTimer();
            status = string.Format(
                CultureInfo.InvariantCulture,
                "{0} | Seed {1} | 64 LOD 2 tiles | {2:N0} vertices | {3:N0} triangles | {4:F2}s",
                prepared.loadedFromSnapshot ? "Disk cache" : "CPU",
                islandSeed,
                terrainStreamer.BaseVertexCount,
                terrainStreamer.BaseTriangleCount,
                generationLifecycle.Elapsed.TotalSeconds);
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
            if (!generationLifecycle.IsDestroyed)
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
            generationLifecycle.End(cancellation);
        }
    }

    public void Regenerate(int seed)
    {
        Generation.Seed = seed;
        Generate();
    }

    public void Clear()
    {
        generationLifecycle.Cancel();
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
        if (generationLifecycle.IsGenerating || islandRuntime != null)
        {
            throw new InvalidOperationException(
                "World management must be configured before island generation starts.");
        }
        worldManaged = true;
        controlsWorldEnvironment = false;
        environmentFollowTarget = Streaming.Target;
    }

    internal IslandGenerationRequest CreateGenerationRequest(
        IslandDescriptor descriptor)
    {
        return new IslandGenerationRequest(
            descriptor,
            Generation,
            Rivers,
            Forest,
            Reeds,
            Ferns,
            Rendering,
            Decorations,
            DebugSettings);
    }

    internal void ApplyRequestProfile(IslandGenerationProfile profile)
    {
        if (profile == null) throw new ArgumentNullException(nameof(profile));
        if (generationLifecycle.IsGenerating || islandRuntime != null)
        {
            throw new InvalidOperationException(
                "An island request profile must be applied before generation starts.");
        }
        activeProfile = profile.Clone();
    }

    public void Configure(
        IslandConfiguration islandConfiguration,
        Transform streamingTarget = null)
    {
        if (islandConfiguration == null)
        {
            throw new ArgumentNullException(nameof(islandConfiguration));
        }
        if (generationLifecycle.IsGenerating || islandRuntime != null)
        {
            throw new InvalidOperationException(
                "Island configuration must be selected before generation starts.");
        }
        configuration = islandConfiguration;
        activeProfile = null;
        Streaming.Target = streamingTarget;
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
