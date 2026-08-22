using System;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Rendering;
using Debug = UnityEngine.Debug;

public sealed class IslandGenerator : MonoBehaviour
{
    private const float SeaHeight = 0f;
    private const float ValidationWorldSize = 2000f;
    private const int SurfaceMapDimension = 2048;
    private const int CliffNoiseDimension = 64;
    private const int CliffNoiseLatticePeriod = 16;
    private const int RiverNoiseDimension = 256;
    private const int RiverNoiseLatticePeriod = 32;
    private const int GrassPatchNoiseDimension = 256;
    private const int GrassPatchNoiseLatticePeriod = 64;
    private const int GrassColourNoiseLatticePeriod = 8;
    private const int AverageColourSampleDimension = 256;
    private const float RockPatchNoiseDetailScale = 8f;
    private static readonly Color DefaultRockColor = new Color(0.34f, 0.32f, 0.29f, 1f);
    private static readonly int IslandWorldToLocalId = Shader.PropertyToID(
        "_IslandWorldToLocal");

    [Header("Lifecycle and Generation")]
    [SerializeField] private IslandGenerationSettings generation = new IslandGenerationSettings();

    [Header("Rivers")]
    [SerializeField] private IslandRiverSettings rivers = new IslandRiverSettings();

    [Header("Streaming")]
    [SerializeField] private IslandStreamingSettings streaming = new IslandStreamingSettings();

    [Header("Rendering and Texture Overrides")]
    [SerializeField] private IslandRenderingSettings rendering = new IslandRenderingSettings();

    [Header("Decoration Asset Libraries")]
    [SerializeField] private IslandDecorationSettings decorations = new IslandDecorationSettings();

    [Header("Debug")]
    [SerializeField] private IslandDebugSettings debugSettings = new IslandDebugSettings();

    private NativeIslandHandle islandHandle;
    private TerrainTileStreamer terrainStreamer;
    private GameObject runtimeRoot;
    private GameObject seaObject;
    private Material terrainMaterial;
    private Material grassMaterial;
    private Texture2D terrainNormalTexture;
    private Texture2D terrainOcclusionTexture;
    private Texture2D seaMaskTexture;
    private Texture3D cliffNoiseTexture;
    private Texture2D riverNoiseTexture;
    private Texture2D grassPatchNoiseTexture;
    private Material rockMaterial;
    private Material riverMaterial;
    private Material seaMaterial;
    private Material meshEdgeMaterial;
    private Material waterfallFaceTerrainDebugMaterial;
    private Material waterfallPlaneDebugMaterial;
    private Material waterfallLipPlaneDebugMaterial;
    private GameObject riverBedDebugOverlay;
    private GameObject waterfallFaceTerrainDebugOverlay;
    private GameObject waterfallPlaneDebugOverlay;
    private GameObject waterfallLipPlaneDebugOverlay;
    private Mesh riverBedDebugWireMesh;
    private Mesh waterfallFaceTerrainDebugWireMesh;
    private Mesh waterfallPlaneDebugWireMesh;
    private Mesh waterfallLipPlaneDebugWireMesh;
    private string status = "Ready";
    private CancellationTokenSource generationCancellation;
    private Stopwatch generationTimer;
    private bool generationInProgress;
    private bool isDestroyed;
    private bool hasStarted;
    private bool ownsCliffNoiseTexture;
    private bool ownsRiverNoiseTexture;
    private bool ownsGrassPatchNoiseTexture;
    private bool? appliedShowRivers;
    private bool? appliedShowSea;
    private bool? appliedShowGrass;
    private bool? appliedShowRocks;
    private bool? appliedShowMeshEdges;
    private bool? appliedShowRiverDebugGeometry;
    private bool? appliedEmitterDebug;
    private Color? appliedGrassColourA;
    private Color? appliedGrassColourB;
    private float appliedGrassColourNoiseWorldSize = float.NaN;
    private float appliedGrassBrightness = float.NaN;

    public bool IsGenerating => generationInProgress;
    public string Status => status;
    public float WorldSizeMetres => generation.WorldSizeMetres;
    public IslandGenerationSettings Generation => generation;
    public IslandRiverSettings Rivers => rivers;
    public IslandStreamingSettings Streaming => streaming;
    public IslandRenderingSettings Rendering => rendering;
    public IslandDecorationSettings Decorations => decorations;
    public IslandDebugSettings DebugSettings => debugSettings;

    private void Start()
    {
        hasStarted = true;
        if (generation.GenerateOnStart)
        {
            Generate();
        }
    }

    private void OnEnable()
    {
        Camera.onPreCull += PrepareCameraRender;
        EnsureActiveCameraDepthTextures();
        if (hasStarted && generation.GenerateOnStart && terrainStreamer == null)
        {
            Generate();
        }
    }

    private void OnDisable()
    {
        Camera.onPreCull -= PrepareCameraRender;
        generationCancellation?.Cancel();
        ClearGeneratedContent();
    }

    private void PrepareCameraRender(Camera camera)
    {
        EnsureCameraDepthTexture(camera);
    }

    private static void EnsureCameraDepthTexture(Camera camera)
    {
        if (camera != null)
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
        var meshEdgeKey = debugSettings.ToggleMeshEdgesKey;
        if (meshEdgeKey != KeyCode.None && Input.GetKeyDown(meshEdgeKey))
        {
            debugSettings.ShowMeshEdges = !debugSettings.ShowMeshEdges;
        }
        var riverDebugKey = debugSettings.ToggleRiverDebugGeometryKey;
        if (riverDebugKey != KeyCode.None && Input.GetKeyDown(riverDebugKey))
        {
            debugSettings.ShowRiverDebugGeometry = !debugSettings.ShowRiverDebugGeometry;
        }
        UpdateMaterialTransforms();
        ApplyLiveSettings();
        if (terrainStreamer != null && streaming.Target != null)
        {
            terrainStreamer.SetPlayerPosition(streaming.Target.position);
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

    private void BuildRuntimeMaterials()
    {
        var skyColor = new Color(0.49f, 0.68f, 0.82f);
        terrainMaterial = CreateMaterial(
            "Motu/Terrain Unified",
            Color.white,
            rendering.TerrainMaterial,
            generation.WorldSizeMetres);
        cliffNoiseTexture = rendering.CliffDetailNoise;
        ownsCliffNoiseTexture = cliffNoiseTexture == null;
        if (ownsCliffNoiseTexture) cliffNoiseTexture = CreateCliffNoiseTexture();
        terrainMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        var rockColor = ApplyAverageTextureColor(
            terrainMaterial,
            "_RockAlbedoMap",
            "_RockColor",
            DefaultRockColor);
        ApplyAverageTextureColor(
            terrainMaterial,
            "_RiverBedAlbedoMap",
            "_RiverBedColor",
            DefaultRockColor);
        grassMaterial = CreateMaterial(
            "Motu/Terrain Grass",
            Color.white,
            rendering.GrassMaterial,
            generation.WorldSizeMetres);
        grassMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        CopyTerrainBlendSettingsToGrass();
        rockMaterial = CreateMaterial(
            "Motu/Rock Decoration",
            rockColor,
            rendering.RockMaterial,
            generation.WorldSizeMetres);
        rockMaterial.name = "Island rock decoration material";
        rockMaterial.enableInstancing = true;
        rockMaterial.SetColor("_RockColor", rockColor);
        rockMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        rockMaterial.SetFloat(
            "_CliffNoisePeriod",
            terrainMaterial.GetFloat("_CliffNoisePeriod"));
        rockMaterial.SetFloat(
            "_CliffNoiseDetailScale",
            terrainMaterial.GetFloat("_CliffNoiseDetailScale"));
        rockMaterial.SetFloat(
            "_CliffNormalStrength",
            terrainMaterial.GetFloat("_CliffNormalStrength"));
        terrainMaterial.SetFloat(
            "_RockPatchNoiseDetailScale",
            RockPatchNoiseDetailScale);
        grassMaterial.SetFloat(
            "_RockPatchNoiseDetailScale",
            RockPatchNoiseDetailScale);
        terrainMaterial.SetFloat(
            "_BeachMaximumElevation",
            rendering.BeachMaximumElevationMetres);
        grassMaterial.SetFloat(
            "_BeachMaximumElevation",
            rendering.BeachMaximumElevationMetres);
        terrainMaterial.SetFloat(
            "_SandPatchNoiseWorldSize",
            rendering.SandPatchSizeMetres);
        grassMaterial.SetFloat(
            "_SandPatchNoiseWorldSize",
            rendering.SandPatchSizeMetres);
        grassPatchNoiseTexture = rendering.GrassPatchNoise;
        ownsGrassPatchNoiseTexture = grassPatchNoiseTexture == null;
        if (ownsGrassPatchNoiseTexture) grassPatchNoiseTexture = CreateGrassPatchNoiseTexture();
        terrainMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        terrainMaterial.SetFloat(
            "_GrassPatchNoiseWorldSize",
            rendering.GrassPatchSizeMetres);
        grassMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        grassMaterial.SetFloat(
            "_GrassPatchNoiseWorldSize",
            rendering.GrassPatchSizeMetres);
        ApplyGrassColourSettings();
        grassMaterial.SetFloat("_GrassBrightness", rendering.GrassBrightness);
        var sun = rendering.Sunlight != null ? rendering.Sunlight : RenderSettings.sun;
        grassMaterial.SetVector(
            "_GrassLightDirection",
            sun != null ? -sun.transform.forward : Vector3.down);
        grassMaterial.SetColor(
            "_GrassLightColor",
            sun != null ? sun.color * sun.intensity : Color.white);
        grassMaterial.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
        riverNoiseTexture = rendering.RiverNoise;
        ownsRiverNoiseTexture = riverNoiseTexture == null;
        if (ownsRiverNoiseTexture) riverNoiseTexture = CreateRiverNoiseTexture();
        var waterColor = new Color(0.03f, 0.28f, 0.55f, 1f);
        var sandColor = new Color(0.62f, 0.57f, 0.34f, 1f);
        const float shallowWaterOpacity = 0.25f;
        const float fullOpacityDepth = 5f;
        const float shoreWaveStrength = 0.35f;
        const float riverShoreWaveSpeed = -0.07f;
        const float seaShoreWaveSpeed = 0.35f;
        const float riverShoreWaveSpacing = 0.11f;
        const float riverShoreWaveDepth = 0.5f;
        const float riverShoreWaveNoiseWorldSize = 1f;
        const float seaShoreWaveSpacing = 0.55f;
        const float seaShoreWaveDepth = 2.5f;
        const float seaShoreWaveNoiseWorldSize = 5f;
        riverMaterial = CreateMaterial(
            "Motu/River Water",
            waterColor,
            rendering.RiverMaterial,
            generation.WorldSizeMetres);
        riverMaterial.renderQueue = (int)RenderQueue.Transparent + 10;
        riverMaterial.SetTexture("_NoiseTex", riverNoiseTexture);
        riverMaterial.SetFloat("_WhitewaterStrength", 0.9f);
        riverMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
        riverMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
        riverMaterial.SetFloat("_EstuaryStrength", 1f);
        riverMaterial.SetColor(
            "_EstuaryColor",
            Color.Lerp(sandColor, waterColor, 0.5f));
        riverMaterial.SetFloat("_EstuaryBlendHeight", rendering.EstuaryBlendHeightMetres);
        riverMaterial.SetFloat("_SeaLevel", SeaHeight);
        riverMaterial.SetColor("_ReflectionColor", skyColor);
        riverMaterial.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(skyColor, Color.white, 0.35f));
        riverMaterial.SetFloat("_ReflectionStrength", 0.45f);
        riverMaterial.SetFloat("_SunGlintStrength", 0.55f);
        ConfigureShoreWaves(
            riverMaterial,
            riverNoiseTexture,
            shoreWaveStrength,
            riverShoreWaveSpacing,
            riverShoreWaveSpeed,
            riverShoreWaveDepth,
            riverShoreWaveNoiseWorldSize);
        seaMaterial = CreateMaterial(
            "Motu/Sea Water",
            waterColor,
            rendering.SeaMaterial,
            generation.WorldSizeMetres);
        seaMaterial.renderQueue = (int)RenderQueue.Transparent;
        seaMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
        seaMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
        seaMaterial.SetColor(
            "_SiltColor",
            Color.Lerp(sandColor, waterColor, 0.5f));
        seaMaterial.SetColor("_ReflectionColor", skyColor);
        seaMaterial.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(skyColor, Color.white, 0.35f));
        seaMaterial.SetFloat("_ReflectionStrength", 0.65f);
        seaMaterial.SetFloat("_SunGlintStrength", 0.8f);
        ConfigureShoreWaves(
            seaMaterial,
            riverNoiseTexture,
            shoreWaveStrength,
            seaShoreWaveSpacing,
            seaShoreWaveSpeed,
            seaShoreWaveDepth,
            seaShoreWaveNoiseWorldSize);
        meshEdgeMaterial = CreateMaterial(
            "Motu/Mesh Edge Overlay",
            Color.black,
            null,
            generation.WorldSizeMetres);
        meshEdgeMaterial.renderQueue = (int)RenderQueue.Overlay + 100;
        meshEdgeMaterial.SetColor("_Color", Color.black);
        meshEdgeMaterial.SetFloat("_ZTest", (float)CompareFunction.LessEqual);
        var waterfallFaceTerrainDebugColor = new Color(1f, 0.45f, 0f, 1f);
        waterfallFaceTerrainDebugMaterial = CreateMaterial(
            "Motu/Mesh Edge Overlay",
            waterfallFaceTerrainDebugColor,
            null,
            generation.WorldSizeMetres);
        waterfallFaceTerrainDebugMaterial.renderQueue = (int)RenderQueue.Overlay + 500;
        waterfallFaceTerrainDebugMaterial.SetColor(
            "_Color",
            waterfallFaceTerrainDebugColor);
        waterfallFaceTerrainDebugMaterial.SetFloat(
            "_ZTest",
            (float)CompareFunction.Always);
        waterfallPlaneDebugMaterial = CreateMaterial(
            "Motu/Mesh Edge Overlay",
            Color.red,
            null,
            generation.WorldSizeMetres);
        waterfallPlaneDebugMaterial.renderQueue = (int)RenderQueue.Overlay + 110;
        waterfallPlaneDebugMaterial.SetColor("_Color", Color.red);
        waterfallPlaneDebugMaterial.SetFloat("_ZTest", (float)CompareFunction.Always);
        waterfallLipPlaneDebugMaterial = CreateMaterial(
            "Motu/Mesh Edge Overlay",
            Color.yellow,
            null,
            generation.WorldSizeMetres);
        waterfallLipPlaneDebugMaterial.renderQueue = (int)RenderQueue.Overlay + 120;
        waterfallLipPlaneDebugMaterial.SetColor("_Color", Color.yellow);
        waterfallLipPlaneDebugMaterial.SetFloat("_ZTest", (float)CompareFunction.Always);
        UpdateMaterialTransforms();
    }

    private void CopyTerrainBlendSettingsToGrass()
    {
        CopyMaterialTexture("_RockMaskMap");
        CopyMaterialFloat("_RockTextureWorldSize");
        CopyMaterialFloat("_RockHeightBlendStrength");
        CopyMaterialTexture("_RiverBedMaskMap");
        CopyMaterialFloat("_RiverBedTextureWorldSize");
        CopyMaterialFloat("_RiverBedHeightBlendStrength");
    }

    private void CopyMaterialTexture(string propertyName)
    {
        if (terrainMaterial.HasProperty(propertyName)
            && grassMaterial.HasProperty(propertyName))
        {
            grassMaterial.SetTexture(
                propertyName,
                terrainMaterial.GetTexture(propertyName));
        }
    }

    private void CopyMaterialFloat(string propertyName)
    {
        if (terrainMaterial.HasProperty(propertyName)
            && grassMaterial.HasProperty(propertyName))
        {
            grassMaterial.SetFloat(
                propertyName,
                terrainMaterial.GetFloat(propertyName));
        }
    }

    private static void ConfigureShoreWaves(
        Material material,
        Texture noise,
        float strength,
        float spacing,
        float speed,
        float depth,
        float noiseWorldSize)
    {
        material.SetTexture("_NoiseTex", noise);
        material.SetFloat("_ShoreWaveStrength", strength);
        material.SetFloat("_ShoreWaveSpacing", spacing);
        material.SetFloat("_ShoreWaveSpeed", speed);
        material.SetFloat("_ShoreWaveDepth", depth);
        material.SetFloat("_ShoreWaveNoiseWorldSize", noiseWorldSize);
    }

    private bool HasSupportedTransform()
    {
        var scale = transform.lossyScale;
        return Mathf.Approximately(scale.x, 1f)
            && Mathf.Approximately(scale.y, 1f)
            && Mathf.Approximately(scale.z, 1f)
            && Vector3.Dot(transform.up, Vector3.up) > 0.99999f;
    }

    private void UpdateMaterialTransforms()
    {
        var worldToLocal = transform.worldToLocalMatrix;
        terrainMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        grassMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        rockMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        riverMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
        seaMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
    }

    private void ApplyLiveSettings()
    {
        if (!appliedGrassColourA.HasValue
            || !appliedGrassColourB.HasValue
            || appliedGrassColourA.Value != rendering.GrassColourA
            || appliedGrassColourB.Value != rendering.GrassColourB
            || !Mathf.Approximately(
                appliedGrassColourNoiseWorldSize,
                rendering.GrassColourNoiseWorldSizeMetres))
        {
            ApplyGrassColourSettings();
        }
        if (!Mathf.Approximately(appliedGrassBrightness, rendering.GrassBrightness))
        {
            appliedGrassBrightness = rendering.GrassBrightness;
            grassMaterial?.SetFloat("_GrassBrightness", appliedGrassBrightness);
        }
        if (appliedShowRivers != rendering.ShowRivers)
        {
            appliedShowRivers = rendering.ShowRivers;
            terrainStreamer?.SetRiversVisible(rendering.ShowRivers);
        }
        if (appliedShowSea != rendering.ShowSea)
        {
            appliedShowSea = rendering.ShowSea;
            seaObject?.SetActive(rendering.ShowSea);
        }
        if (appliedShowGrass != rendering.ShowGrass)
        {
            appliedShowGrass = rendering.ShowGrass;
            terrainStreamer?.SetGrassVisible(rendering.ShowGrass);
        }
        if (appliedShowRocks != rendering.ShowRocks)
        {
            appliedShowRocks = rendering.ShowRocks;
            terrainStreamer?.SetRocksVisible(rendering.ShowRocks);
        }
        if (appliedShowMeshEdges != debugSettings.ShowMeshEdges)
        {
            appliedShowMeshEdges = debugSettings.ShowMeshEdges;
            terrainStreamer?.SetMeshEdgesVisible(debugSettings.ShowMeshEdges);
        }
        if (appliedShowRiverDebugGeometry != debugSettings.ShowRiverDebugGeometry)
        {
            appliedShowRiverDebugGeometry = debugSettings.ShowRiverDebugGeometry;
            riverBedDebugOverlay?.SetActive(debugSettings.ShowRiverDebugGeometry);
            waterfallFaceTerrainDebugOverlay?.SetActive(debugSettings.ShowRiverDebugGeometry);
            waterfallPlaneDebugOverlay?.SetActive(debugSettings.ShowRiverDebugGeometry);
            waterfallLipPlaneDebugOverlay?.SetActive(debugSettings.ShowRiverDebugGeometry);
        }
        if (appliedEmitterDebug != debugSettings.ShowRoughWaterEmitters)
        {
            appliedEmitterDebug = debugSettings.ShowRoughWaterEmitters;
            terrainStreamer?.SetRiverEmitterDebug(debugSettings.ShowRoughWaterEmitters);
        }
    }

    private void ApplyGrassColourSettings()
    {
        appliedGrassColourA = rendering.GrassColourA;
        appliedGrassColourB = rendering.GrassColourB;
        appliedGrassColourNoiseWorldSize = rendering.GrassColourNoiseWorldSizeMetres;
        terrainMaterial?.SetColor("_GrassColorA", appliedGrassColourA.Value);
        terrainMaterial?.SetColor("_GrassColorB", appliedGrassColourB.Value);
        terrainMaterial?.SetFloat(
            "_GrassColorNoiseWorldSize",
            appliedGrassColourNoiseWorldSize);
        grassMaterial?.SetColor("_GrassColorA", appliedGrassColourA.Value);
        grassMaterial?.SetColor("_GrassColorB", appliedGrassColourB.Value);
        grassMaterial?.SetFloat(
            "_GrassColorNoiseWorldSize",
            appliedGrassColourNoiseWorldSize);
    }

    public async void Generate()
    {
        if (generationInProgress)
        {
            return;
        }
        if (!HasSupportedTransform())
        {
            status = "Island transform must use unit scale and Y-axis rotation only.";
            Debug.LogError(status, this);
            return;
        }

        status = "Generating island in background...";
        generationInProgress = true;
        generationTimer = Stopwatch.StartNew();
        var cancellation = new CancellationTokenSource();
        generationCancellation = cancellation;
        IslandPreparedData prepared = null;
        var installationStarted = false;
        var islandSeed = generation.Seed;
        var worldSize = generation.WorldSizeMetres;
        var options = generation.ToNativeOptions(rivers);
        var emitterSharpness = rivers.RoughWaterSharpnessDegrees;
        var emitterSpacing = rivers.RoughWaterSpacingMetres;

        try
        {
            prepared = await IslandGenerationWorker.GenerateAsync(
                islandSeed,
                options,
                worldSize,
                emitterSharpness,
                emitterSpacing,
                cancellation.Token);
            cancellation.Token.ThrowIfCancellationRequested();
            if (isDestroyed || !isActiveAndEnabled)
            {
                return;
            }

            status = "Uploading generated island...";
            installationStarted = true;
            ClearGeneratedContent();
            DestroyRuntimeMaterials();
            BuildRuntimeMaterials();
            islandHandle = prepared.TakeHandle();

            CreateSurfaceTextures(prepared.surfaceMaps);
            CreateSeaMaskTexture(prepared.seaMask);
            await Task.Yield();
            cancellation.Token.ThrowIfCancellationRequested();

            runtimeRoot = new GameObject("Generated Island");
            runtimeRoot.transform.SetParent(transform, false);
            CreateRiverDebugOverlays(
                prepared.riverBedDebugMesh,
                prepared.waterfallFaceTerrainDebugMesh,
                prepared.waterfallPlaneDebugMesh,
                prepared.waterfallLipPlaneDebugMesh);
            var terrainRoot = new GameObject("Terrain Tiles");
            terrainRoot.transform.SetParent(runtimeRoot.transform, false);
            terrainStreamer = terrainRoot.AddComponent<TerrainTileStreamer>();
            await terrainStreamer.InitializeAsync(
                islandHandle.Value,
                terrainMaterial,
                grassMaterial,
                rockMaterial,
                riverMaterial,
                meshEdgeMaterial,
                worldSize,
                prepared.overviewTiles,
                prepared.riverTiles,
                prepared.riverRockTiles,
                prepared.riverEmitters,
                prepared.rocks,
                prepared.colliderHeightMap,
                rendering.ShowRivers,
                rendering.ShowGrass,
                rendering.ShowRocks,
                cancellation.Token);
            terrainStreamer.SetRiverEmitterDebug(debugSettings.ShowRoughWaterEmitters);

            seaObject = GameObject.CreatePrimitive(PrimitiveType.Plane);
            seaObject.name = "Sea";
            seaObject.transform.SetParent(runtimeRoot.transform, false);
            seaObject.transform.localPosition = Vector3.up * SeaHeight;
            seaObject.transform.localScale = Vector3.one * (worldSize / 10f);
            seaObject.GetComponent<MeshRenderer>().sharedMaterial = seaMaterial;
            DestroyUnityObject(seaObject.GetComponent<Collider>());
            seaObject.SetActive(rendering.ShowSea);
            ResetAppliedLiveSettings();
            ApplyLiveSettings();
            if (streaming.Target != null)
            {
                terrainStreamer.SetPlayerPosition(streaming.Target.position);
            }

            generationTimer.Stop();
            status = string.Format(
                CultureInfo.InvariantCulture,
                "Seed {0} | 64 LOD 2 tiles | {1:N0} vertices | {2:N0} triangles | {3:F2}s",
                islandSeed,
                terrainStreamer.BaseVertexCount,
                terrainStreamer.BaseTriangleCount,
                generationTimer.Elapsed.TotalSeconds);
            status += " | shared 2048 terrain shading map";
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | {0:N0} rough-water candidates / 32 pooled systems",
                terrainStreamer.RiverEmitterCandidateCount);
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | {0:N0} rock candidates / 128 pooled renderers",
                terrainStreamer.RockCandidateCount);
            status += string.Format(
                CultureInfo.InvariantCulture,
                " | 3x3 hidden LOD 1 terrain colliders (129x129 samples each) | {0:F1} km square",
                worldSize / 1000f);
        }
        catch (OperationCanceledException)
        {
            if (installationStarted)
            {
                ClearGeneratedContent();
            }
            if (!isDestroyed)
            {
                status = "Generation cancelled.";
            }
        }
        catch (Exception exception)
        {
            status = exception.Message;
            Debug.LogException(exception);
            if (installationStarted)
            {
                ClearGeneratedContent();
            }
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
        generation.Seed = seed;
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
        streaming.Target = target;
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

    public void ConfigureSceneReferences(
        Transform streamingTarget,
        Light sunlight,
        Material terrainTemplate,
        Material grassTemplate,
        Material riverTemplate,
        Material seaTemplate,
        Material rockTemplate)
    {
        streaming.Target = streamingTarget;
        rendering.Sunlight = sunlight;
        rendering.AssignMaterialTemplates(
            terrainTemplate,
            grassTemplate,
            riverTemplate,
            seaTemplate,
            rockTemplate);
    }

    internal static IslandPreparedData PrepareIsland(
        int islandSeed,
        MotuNative.Options options,
        float worldSize,
        float emitterSharpnessDegrees,
        float emitterSpacingMetres,
        CancellationToken cancellationToken)
    {
        var handle = MotuNative.CreateMotu(islandSeed, ref options);
        if (handle == IntPtr.Zero)
        {
            throw new InvalidOperationException("The Rust generator returned a null island handle.");
        }

        try
        {
            cancellationToken.ThrowIfCancellationRequested();
            var surfaceMaps = PrepareSurfaceMaps(handle, SurfaceMapDimension);
            cancellationToken.ThrowIfCancellationRequested();
            var seaMask = PrepareSeaMask(handle, SurfaceMapDimension);
            cancellationToken.ThrowIfCancellationRequested();
            var colliderHeightMap = PrepareColliderHeightMap(handle, worldSize);
            cancellationToken.ThrowIfCancellationRequested();
            var overviewTiles = TerrainTileStreamer.PrepareOverviewTiles(handle, worldSize);
            cancellationToken.ThrowIfCancellationRequested();
            var riverTiles = PrepareRiverTiles(handle, worldSize);
            cancellationToken.ThrowIfCancellationRequested();
            var riverRockTiles = PrepareRiverRockTiles(handle, worldSize);
            cancellationToken.ThrowIfCancellationRequested();
            var riverBedDebugMesh = PrepareRiverBedDebugMesh(handle, worldSize);
            var waterfallFaceTerrainDebugMesh = PrepareWaterfallFaceTerrainDebugMesh(
                handle,
                worldSize);
            var waterfallPlaneDebugMesh = PrepareWaterfallPlaneDebugMesh(handle, worldSize);
            var waterfallLipPlaneDebugMesh = PrepareWaterfallLipPlaneDebugMesh(handle, worldSize);
            cancellationToken.ThrowIfCancellationRequested();
            var riverEmitters = PrepareRiverEmitters(
                handle,
                worldSize,
                emitterSharpnessDegrees,
                emitterSpacingMetres);
            cancellationToken.ThrowIfCancellationRequested();
            var rocks = PrepareRockDecorations(
                handle,
                surfaceMaps,
                islandSeed,
                worldSize);
            cancellationToken.ThrowIfCancellationRequested();
            var result = new IslandPreparedData(
                handle,
                surfaceMaps,
                seaMask,
                overviewTiles,
                riverTiles,
                riverRockTiles,
                riverBedDebugMesh,
                waterfallFaceTerrainDebugMesh,
                waterfallPlaneDebugMesh,
                waterfallLipPlaneDebugMesh,
                riverEmitters,
                rocks,
                colliderHeightMap);
            handle = IntPtr.Zero;
            return result;
        }
        finally
        {
            if (handle != IntPtr.Zero)
            {
                MotuNative.ReleaseMotu(handle);
            }
        }
    }

    private static IslandPreparedSurfaceMaps PrepareSurfaceMaps(
        IntPtr handle,
        int dimension)
    {
        MotuNative.CreateSurfaceMaps(handle, 0, dimension, out var surfaceMaps);
        try
        {
            if (surfaceMaps.handle == IntPtr.Zero
                || surfaceMaps.occlusion == IntPtr.Zero
                || surfaceMaps.normalRgb == IntPtr.Zero
                || surfaceMaps.width != dimension
                || surfaceMaps.height != dimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned invalid terrain surface maps.");
            }

            var pixelCount = checked(dimension * dimension);
            var occlusionBytes = new byte[pixelCount];
            Marshal.Copy(surfaceMaps.occlusion, occlusionBytes, 0, occlusionBytes.Length);
            var normalBytes = new byte[checked(pixelCount * 3)];
            Marshal.Copy(surfaceMaps.normalRgb, normalBytes, 0, normalBytes.Length);
            return new IslandPreparedSurfaceMaps(dimension, normalBytes, occlusionBytes);
        }
        finally
        {
            MotuNative.ReleaseSurfaceMaps(ref surfaceMaps);
        }
    }

    private static IslandPreparedSeaMask PrepareSeaMask(IntPtr handle, int dimension)
    {
        MotuNative.CreateSeaMask(handle, dimension, out var seaMask);
        try
        {
            if (seaMask.handle == IntPtr.Zero
                || seaMask.rg == IntPtr.Zero
                || seaMask.width != dimension
                || seaMask.height != dimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned an invalid sea coast/silt mask.");
            }

            var byteCount = checked(dimension * dimension * 2);
            var rg = new byte[byteCount];
            Marshal.Copy(seaMask.rg, rg, 0, rg.Length);
            return new IslandPreparedSeaMask(dimension, rg);
        }
        finally
        {
            MotuNative.ReleaseSeaMask(ref seaMask);
        }
    }

    private static IslandPreparedColliderHeightMap PrepareColliderHeightMap(
        IntPtr handle,
        float terrainWorldSize)
    {
        var mapPointer = MotuNative.CreateTerrainColliderHeightMap(
            handle,
            TerrainTileStreamer.ColliderSamplesPerTile);
        try
        {
            if (mapPointer == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned a null terrain-collider height map.");
            }
            var native = Marshal.PtrToStructure<MotuNative.ExportHeightMapWithSeaLevel>(
                mapPointer);
            var expectedDimension = checked(
                TerrainTileStreamer.Lod1Resolution
                * (TerrainTileStreamer.ColliderSamplesPerTile - 1)
                + 1);
            if (native.data == IntPtr.Zero
                || native.width != expectedDimension
                || native.height != expectedDimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned invalid terrain-collider height-map data.");
            }

            var heights = new float[checked(native.width * native.height)];
            Marshal.Copy(native.data, heights, 0, heights.Length);
            return new IslandPreparedColliderHeightMap(
                native.width,
                TerrainTileStreamer.ColliderSamplesPerTile,
                heights,
                terrainWorldSize);
        }
        finally
        {
            MotuNative.ReleaseTerrainColliderHeightMap(mapPointer);
        }
    }

    private static IslandPreparedMesh[] PrepareRiverTiles(IntPtr handle, float worldSize)
    {
        var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
        MotuNative.CreateRiverMeshGrid(
            handle,
            ref area,
            TerrainTileStreamer.Lod1Resolution,
            out var export);
        try
        {
            var expectedLength = TerrainTileStreamer.Lod1Resolution
                * TerrainTileStreamer.Lod1Resolution;
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != expectedLength)
            {
                throw new InvalidOperationException(
                    "The Rust river slicer returned an invalid LOD 1 tile batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = CopyRiverMeshData(nativeMesh, worldSize);
                }
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private static IslandPreparedMesh[] PrepareRiverRockTiles(IntPtr handle, float worldSize)
    {
        var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
        MotuNative.CreateRiverRockMeshGrid(
            handle,
            ref area,
            TerrainTileStreamer.Lod1Resolution,
            out var export);
        try
        {
            var expectedLength = TerrainTileStreamer.Lod1Resolution
                * TerrainTileStreamer.Lod1Resolution;
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != expectedLength)
            {
                throw new InvalidOperationException(
                    "The Rust river-rock slicer returned an invalid LOD 1 tile batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = CopyRiverMeshData(nativeMesh, worldSize);
                }
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private static IslandPreparedMesh PrepareRiverBedDebugMesh(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateRiverBedDebugMesh(handle, out var export);
        return PrepareDebugMesh(ref export, worldSize, "river-bed");
    }

    private static IslandPreparedMesh PrepareWaterfallFaceTerrainDebugMesh(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateWaterfallFaceTerrainDebugMesh(handle, out var export);
        return PrepareDebugMesh(ref export, worldSize, "waterfall-face-terrain");
    }

    private static IslandPreparedMesh PrepareWaterfallPlaneDebugMesh(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateWaterfallPlaneDebugMesh(handle, out var export);
        return PrepareDebugMesh(ref export, worldSize, "waterfall-plane");
    }

    private static IslandPreparedMesh PrepareWaterfallLipPlaneDebugMesh(
        IntPtr handle,
        float worldSize)
    {
        MotuNative.CreateWaterfallLipPlaneDebugMesh(handle, out var export);
        return PrepareDebugMesh(ref export, worldSize, "waterfall-lip-plane");
    }

    private static IslandPreparedMesh PrepareDebugMesh(
        ref MotuNative.ExportMesh export,
        float worldSize,
        string geometryName)
    {
        try
        {
            if (export.handle == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    $"The Rust {geometryName} debug export has no owner.");
            }
            if (export.triangles.length == 0)
            {
                Debug.LogWarning($"Motu debug mesh '{geometryName}' exported no triangles.");
                return null;
            }
            if (export.vertices.data == IntPtr.Zero
                || export.normals.data == IntPtr.Zero
                || export.triangles.data == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    $"The Rust {geometryName} debug export is incomplete.");
            }
            var prepared = CopyRiverMeshData(export, worldSize);
            Debug.Log(
                $"Motu debug mesh '{geometryName}': "
                + $"{prepared.vertices.Length} vertices, "
                + $"{prepared.triangles.Length / 3} triangles.");
            return prepared;
        }
        finally
        {
            MotuNative.ReleaseMesh(ref export);
        }
    }

    private static IslandPreparedRiverEmitter[] PrepareRiverEmitters(
        IntPtr handle,
        float worldSize,
        float sharpnessDegrees,
        float spacingMetres)
    {
        MotuNative.CreateRiverEmitters(
            handle,
            sharpnessDegrees,
            spacingMetres,
            out var export);
        try
        {
            if (export.handle == IntPtr.Zero || export.length < 0)
            {
                throw new InvalidOperationException(
                    "The Rust rough-water emitter export is invalid.");
            }
            if (export.length == 0)
            {
                return Array.Empty<IslandPreparedRiverEmitter>();
            }
            if (export.data == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust rough-water emitter data is missing.");
            }

            var result = new IslandPreparedRiverEmitter[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.RiverEmitterExport>();
            for (var index = 0; index < export.length; index++)
            {
                var native = Marshal.PtrToStructure<MotuNative.RiverEmitterExport>(
                    IntPtr.Add(export.data, index * exportSize));
                var position = new Vector3(
                    (native.position.x - 0.5f) * worldSize,
                    native.position.z * worldSize,
                    (native.position.y - 0.5f) * worldSize);
                var direction = new Vector3(
                    native.direction.x,
                    native.direction.z,
                    native.direction.y).normalized;
                result[index] = new IslandPreparedRiverEmitter(
                    position,
                    direction,
                    Mathf.Clamp01(native.strength));
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseRiverEmitters(ref export);
        }
    }

    private static IslandPreparedRockDecoration[] PrepareRockDecorations(
        IntPtr handle,
        IslandPreparedSurfaceMaps surfaceMaps,
        int islandSeed,
        float worldSize)
    {
        MotuNative.GetDecoration(handle, out var decoration);
        ValidateBorrowedArray(decoration.trees, "tree");
        ValidateBorrowedArray(decoration.bushes, "bush");
        ValidateBorrowedArray(decoration.rocks, "rock");
        ValidateBorrowedArray(decoration.rockAppearanceIds, "rock appearance ID");
        if (decoration.rockAppearanceIds.length != decoration.rocks.length)
        {
            throw new InvalidOperationException(
                "Native rock positions and appearance IDs have different lengths.");
        }
        if (decoration.rocks.length == 0)
        {
            return Array.Empty<IslandPreparedRockDecoration>();
        }

        var result = new IslandPreparedRockDecoration[decoration.rocks.length];
        var nativeSize = Marshal.SizeOf<MotuNative.NativeVector3>();
        for (var sourceIndex = 0; sourceIndex < result.Length; sourceIndex++)
        {
            var native = Marshal.PtrToStructure<MotuNative.NativeVector3>(
                IntPtr.Add(decoration.rocks.data, sourceIndex * nativeSize));
            var appearanceId = unchecked((uint)Marshal.ReadInt32(
                decoration.rockAppearanceIds.data,
                sourceIndex * sizeof(uint)));
            if (!IsFinite(native.x)
                || !IsFinite(native.y)
                || !IsFinite(native.z)
                || native.x < 0f
                || native.x > 1f
                || native.y < 0f
                || native.y > 1f)
            {
                throw new InvalidOperationException(
                    $"Native rock decoration {sourceIndex} has an invalid position.");
            }

            var position = new Vector3(
                (native.x - 0.5f) * worldSize,
                native.z * worldSize,
                (native.y - 0.5f) * worldSize);
            var normal = SampleSurfaceNormal(surfaceMaps, native.x, native.y);
            var random = RockHashState(islandSeed, appearanceId);
            var isBoulder = NextRockRandom(ref random) < 0.15f;
            var classPrototypeCount = isBoulder
                ? RockPrototypeLibrary.BoulderCount
                : RockPrototypeLibrary.StoneCount;
            var classPrototypeOffset = isBoulder ? RockPrototypeLibrary.StoneCount : 0;
            var prototypeIndex = classPrototypeOffset
                + Mathf.Min(
                    Mathf.FloorToInt(NextRockRandom(ref random) * classPrototypeCount),
                    classPrototypeCount - 1);
            var diameter = isBoulder
                ? Mathf.Lerp(
                    RockPrototypeLibrary.BoulderMinimumDiameter,
                    RockPrototypeLibrary.BoulderMaximumDiameter,
                    NextRockRandom(ref random))
                : Mathf.Lerp(
                    RockPrototypeLibrary.StoneMinimumDiameter,
                    RockPrototypeLibrary.StoneMaximumDiameter,
                    NextRockRandom(ref random));
            var depthRatio = Mathf.Lerp(0.74f, 1.0f, NextRockRandom(ref random));
            var verticalRatio = isBoulder
                ? Mathf.Lerp(0.70f, 1.20f, NextRockRandom(ref random))
                : Mathf.Lerp(0.35f, 0.75f, NextRockRandom(ref random));
            var scale = new Vector3(diameter, diameter * verticalRatio, diameter * depthRatio);
            var alignment = Quaternion.FromToRotation(Vector3.up, normal);
            var yaw = Quaternion.AngleAxis(NextRockRandom(ref random) * 360f, normal);
            var tangent = Vector3.Cross(normal, Vector3.right);
            if (tangent.sqrMagnitude < 1.0e-6f)
            {
                tangent = Vector3.Cross(normal, Vector3.forward);
            }
            tangent.Normalize();
            var maximumTilt = isBoulder ? 8f : 12f;
            var tilt = Quaternion.AngleAxis(
                Mathf.Lerp(-maximumTilt, maximumTilt, NextRockRandom(ref random)),
                tangent);
            var rotation = tilt * yaw * alignment;
            var tintValue = Mathf.Lerp(0.94f, 1.06f, NextRockRandom(ref random));
            var tint = new Color(tintValue, tintValue, tintValue, 1f);
            var embedDepth = scale.y * RockPrototypeLibrary.EmbedRatioForNormal(normal);
            result[sourceIndex] = new IslandPreparedRockDecoration(
                sourceIndex,
                appearanceId,
                position,
                normal,
                isBoulder,
                prototypeIndex,
                scale,
                rotation,
                tint,
                embedDepth);
        }
        return result;
    }

    private static void ValidateBorrowedArray(
        MotuNative.Vector3Array values,
        string label)
    {
        if (values.length < 0 || (values.length > 0 && values.data == IntPtr.Zero))
        {
            throw new InvalidOperationException(
                $"The native {label} decoration array is invalid.");
        }
    }

    private static void ValidateBorrowedArray(
        MotuNative.UInt32Array values,
        string label)
    {
        if (values.length < 0 || (values.length > 0 && values.data == IntPtr.Zero))
        {
            throw new InvalidOperationException($"Native {label} array is invalid.");
        }
    }

    private static Vector3 SampleSurfaceNormal(
        IslandPreparedSurfaceMaps maps,
        float normalizedX,
        float normalizedY)
    {
        var sampleX = Mathf.Clamp01(normalizedX) * (maps.dimension - 1);
        var sampleY = Mathf.Clamp01(normalizedY) * (maps.dimension - 1);
        var x0 = Mathf.FloorToInt(sampleX);
        var y0 = Mathf.FloorToInt(sampleY);
        var x1 = Mathf.Min(x0 + 1, maps.dimension - 1);
        var y1 = Mathf.Min(y0 + 1, maps.dimension - 1);
        var tx = sampleX - x0;
        var ty = sampleY - y0;
        var lower = Vector3.Lerp(
            DecodeSurfaceNormal(maps, x0, y0),
            DecodeSurfaceNormal(maps, x1, y0),
            tx);
        var upper = Vector3.Lerp(
            DecodeSurfaceNormal(maps, x0, y1),
            DecodeSurfaceNormal(maps, x1, y1),
            tx);
        var normal = Vector3.Lerp(lower, upper, ty);
        if (!IsFinite(normal.x)
            || !IsFinite(normal.y)
            || !IsFinite(normal.z)
            || normal.sqrMagnitude < 1.0e-8f)
        {
            return Vector3.up;
        }
        return normal.normalized;
    }

    private static Vector3 DecodeSurfaceNormal(
        IslandPreparedSurfaceMaps maps,
        int x,
        int y)
    {
        var offset = (y * maps.dimension + x) * 3;
        var rustX = maps.normalRgb[offset] * (2f / 255f) - 1f;
        var rustY = maps.normalRgb[offset + 1] * (2f / 255f) - 1f;
        var rustZ = maps.normalRgb[offset + 2] * (2f / 255f) - 1f;
        return new Vector3(rustX, rustZ, rustY);
    }

    private static ulong RockHashState(int islandSeed, uint appearanceId)
    {
        return unchecked(
            ((ulong)(uint)islandSeed << 32)
            ^ appearanceId
            ^ 0xD1B54A32D192ED03UL);
    }

    private static float NextRockRandom(ref ulong state)
    {
        unchecked
        {
            state += 0x9E3779B97F4A7C15UL;
            var value = state;
            value = (value ^ (value >> 30)) * 0xBF58476D1CE4E5B9UL;
            value = (value ^ (value >> 27)) * 0x94D049BB133111EBUL;
            value ^= value >> 31;
            return (value >> 40) * (1f / 16777216f);
        }
    }

    private void CreateSurfaceTextures(IslandPreparedSurfaceMaps surfaceMaps)
    {
        terrainOcclusionTexture = CreateSurfaceTexture(
            "Motu Shared Terrain Occlusion",
            surfaceMaps.dimension,
            TextureFormat.R8,
            surfaceMaps.occlusion);
        terrainNormalTexture = CreateSurfaceTexture(
            "Motu Shared Terrain World Normal",
            surfaceMaps.dimension,
            TextureFormat.RGB24,
            surfaceMaps.normalRgb);
        if (!terrainMaterial.HasProperty("_WorldNormal")
            || !terrainMaterial.HasProperty("_Occlusion"))
        {
            throw new InvalidOperationException(
                "The unified terrain shader does not expose its shared surface textures.");
        }
        terrainMaterial.SetTexture("_WorldNormal", terrainNormalTexture);
        terrainMaterial.SetTexture("_Occlusion", terrainOcclusionTexture);
    }

    private void CreateSeaMaskTexture(IslandPreparedSeaMask seaMask)
    {
        if (!SystemInfo.SupportsTextureFormat(TextureFormat.RG16))
        {
            throw new InvalidOperationException(
                "This graphics device does not support the required RG16 sea mask texture.");
        }
        if (!seaMaterial.HasProperty("_SeaMask"))
        {
            throw new InvalidOperationException(
                "The water shader does not expose the generated sea mask.");
        }
        seaMaskTexture = CreateSurfaceTexture(
            "Motu Sea Coast And Silt Mask",
            seaMask.dimension,
            TextureFormat.RG16,
            seaMask.rg);
        seaMaterial.SetTexture("_SeaMask", seaMaskTexture);
    }

    private static Texture2D CreateSurfaceTexture(
        string textureName,
        int dimension,
        TextureFormat format,
        byte[] pixels)
    {
        var texture = new Texture2D(dimension, dimension, format, true, true)
        {
            name = textureName,
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Clamp,
            anisoLevel = 4,
        };
        // Rust supplies only mip 0. LoadRawTextureData expects storage for the
        // entire mip chain when the texture was created with mipmaps enabled.
        // Upload the base mip explicitly and let Apply generate the rest.
        texture.SetPixelData(pixels, 0);
        texture.Apply(true, true);
        return texture;
    }

    internal static IslandPreparedMesh CopyTerrainMeshData(
        MotuNative.ExportMesh source,
        int lod,
        float worldSize)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            true,
            true,
            worldSize);
    }

    internal static Mesh CopyTerrainMesh(
        MotuNative.ExportMesh source,
        int lod,
        float worldSize)
    {
        return CreateTerrainMesh(CopyTerrainMeshData(source, lod, worldSize), lod);
    }

    internal static Mesh CreateTerrainMesh(IslandPreparedMesh source, int lod)
    {
        return CreateMesh(source, false);
    }

    internal static IslandPreparedMesh CopyRiverMeshData(
        MotuNative.ExportMesh source,
        float worldSize)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            false,
            false,
            worldSize);
    }

    internal static Mesh CreateRiverMesh(IslandPreparedMesh source)
    {
        return CreateMesh(source, false);
    }

    private static IslandPreparedMesh CopyMeshData(
        MotuNative.Vector3Array sourceVertices,
        MotuNative.Vector3Array sourceNormals,
        MotuNative.TriangleArray sourceTriangles,
        MotuNative.Vector2Array sourceUv,
        MotuNative.Vector3Array sourceMaterial,
        bool requireMaterial,
        bool createSurfaceMapCoordinates,
        float worldSize)
    {
        if (sourceVertices.data == IntPtr.Zero || sourceVertices.length == 0)
        {
            throw new InvalidOperationException("The Rust generator returned an empty mesh.");
        }

        var vertices = CopyVector3Array(sourceVertices, true, worldSize);
        var normals = CopyVector3Array(sourceNormals, false, worldSize);
        var triangles = new int[sourceTriangles.length];
        Marshal.Copy(sourceTriangles.data, triangles, 0, triangles.Length);

        // Rust is Z-up while Unity is Y-up. Swapping axes reflects the coordinate
        // system, so reverse each triangle to retain the original front face.
        for (var index = 0; index + 2 < triangles.Length; index += 3)
        {
            (triangles[index + 1], triangles[index + 2]) =
                (triangles[index + 2], triangles[index + 1]);
        }

        Vector2[] uv;
        if (sourceUv.data != IntPtr.Zero && sourceUv.length == vertices.Length)
        {
            uv = CopyVector2Array(sourceUv);
        }
        else if (createSurfaceMapCoordinates)
        {
            uv = CreateTerrainUv(vertices, worldSize);
        }
        else
        {
            uv = Array.Empty<Vector2>();
        }

        var material = CopyMaterialArray(sourceMaterial);
        if (requireMaterial && material.Length != vertices.Length)
        {
            throw new InvalidOperationException(
                "The Rust terrain export returned invalid material attributes.");
        }

        return new IslandPreparedMesh(vertices, normals, triangles, uv, material);
    }

    private static Mesh CreateMesh(IslandPreparedMesh source, bool createTangents)
    {
        var mesh = new Mesh
        {
            name = "Motu Generated Mesh",
            indexFormat = source.vertices.Length > ushort.MaxValue
                ? IndexFormat.UInt32
                : IndexFormat.UInt16,
            vertices = source.vertices,
            normals = source.normals,
            triangles = source.triangles,
        };

        if (source.uv.Length == source.vertices.Length)
        {
            mesh.uv = source.uv;
        }
        if (source.material.Length == source.vertices.Length)
        {
            mesh.colors = source.material;
        }
        if (createTangents)
        {
            mesh.RecalculateTangents();
        }

        mesh.RecalculateBounds();
        mesh.UploadMeshData(false);
        return mesh;
    }

    private static Vector2[] CreateTerrainUv(Vector3[] vertices, float worldSize)
    {
        var uv = new Vector2[vertices.Length];
        for (var index = 0; index < vertices.Length; index++)
        {
            uv[index] = new Vector2(
                vertices[index].x / worldSize + 0.5f,
                vertices[index].z / worldSize + 0.5f);
        }
        return uv;
    }

    private static Vector3[] CopyVector3Array(
        MotuNative.Vector3Array source,
        bool position,
        float worldSize)
    {
        if (source.data == IntPtr.Zero || source.length == 0)
        {
            return Array.Empty<Vector3>();
        }

        var packed = new float[checked(source.length * 3)];
        Marshal.Copy(source.data, packed, 0, packed.Length);
        var result = new Vector3[source.length];
        for (var index = 0; index < result.Length; index++)
        {
            var offset = index * 3;
            var x = packed[offset];
            var y = packed[offset + 1];
            var z = packed[offset + 2];
            result[index] = position
                ? new Vector3(
                    (x - 0.5f) * worldSize,
                    z * worldSize,
                    (y - 0.5f) * worldSize)
                : new Vector3(x, z, y).normalized;
        }

        return result;
    }

    private static Vector2[] CopyVector2Array(MotuNative.Vector2Array source)
    {
        var packed = new float[checked(source.length * 2)];
        Marshal.Copy(source.data, packed, 0, packed.Length);
        var result = new Vector2[source.length];
        for (var index = 0; index < result.Length; index++)
        {
            result[index] = new Vector2(packed[index * 2], packed[index * 2 + 1]);
        }

        return result;
    }

    private static Color[] CopyMaterialArray(MotuNative.Vector3Array source)
    {
        if (source.data == IntPtr.Zero || source.length == 0)
        {
            return Array.Empty<Color>();
        }

        var packed = new float[checked(source.length * 3)];
        Marshal.Copy(source.data, packed, 0, packed.Length);
        var result = new Color[source.length];
        for (var index = 0; index < result.Length; index++)
        {
            var offset = index * 3;
            result[index] = new Color(
                packed[offset],
                packed[offset + 1],
                packed[offset + 2],
                1f);
        }
        return result;
    }

    private void CreateRiverDebugOverlays(
        IslandPreparedMesh riverBed,
        IslandPreparedMesh waterfallFaceTerrain,
        IslandPreparedMesh waterfallFootPlanes,
        IslandPreparedMesh waterfallLipPlanes)
    {
        riverBedDebugOverlay = CreateDebugWireOverlay(
            "River Bed Debug (Black)",
            riverBed,
            meshEdgeMaterial,
            out riverBedDebugWireMesh);
        waterfallFaceTerrainDebugOverlay = CreateDebugWireOverlay(
            "Waterfall Face Terrain Debug (Orange)",
            waterfallFaceTerrain,
            waterfallFaceTerrainDebugMaterial,
            out waterfallFaceTerrainDebugWireMesh,
            0.1f);
        waterfallPlaneDebugOverlay = CreateDebugWireOverlay(
            "Waterfall Foot Planes Debug (Red)",
            waterfallFootPlanes,
            waterfallPlaneDebugMaterial,
            out waterfallPlaneDebugWireMesh);
        waterfallLipPlaneDebugOverlay = CreateDebugWireOverlay(
            "Waterfall Lip Planes Debug (Yellow)",
            waterfallLipPlanes,
            waterfallLipPlaneDebugMaterial,
            out waterfallLipPlaneDebugWireMesh);
    }

    private GameObject CreateDebugWireOverlay(
        string objectName,
        IslandPreparedMesh prepared,
        Material material,
        out Mesh wireMesh,
        float verticalOffsetMetres = 0f)
    {
        wireMesh = null;
        if (prepared == null)
        {
            return null;
        }

        var triangleMesh = CreateRiverMesh(prepared);
        try
        {
            wireMesh = TerrainTileStreamer.CreateEdgeMesh(triangleMesh);
            wireMesh.name = objectName;
        }
        finally
        {
            DestroyUnityObject(triangleMesh);
        }

        var overlay = new GameObject(objectName);
        overlay.transform.SetParent(runtimeRoot.transform, false);
        overlay.transform.localPosition = Vector3.up * verticalOffsetMetres;
        overlay.AddComponent<MeshFilter>().sharedMesh = wireMesh;
        var renderer = overlay.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = material;
        renderer.shadowCastingMode = ShadowCastingMode.Off;
        renderer.receiveShadows = false;
        overlay.SetActive(debugSettings.ShowRiverDebugGeometry);
        return overlay;
    }

    private void ClearGeneratedContent()
    {
        if (terrainStreamer != null)
        {
            terrainStreamer.Dispose();
            DestroyUnityObject(terrainStreamer.gameObject);
            terrainStreamer = null;
        }
        DestroyUnityObject(seaObject);
        seaObject = null;
        DestroyUnityObject(riverBedDebugWireMesh);
        DestroyUnityObject(waterfallFaceTerrainDebugWireMesh);
        DestroyUnityObject(waterfallPlaneDebugWireMesh);
        DestroyUnityObject(waterfallLipPlaneDebugWireMesh);
        riverBedDebugWireMesh = null;
        waterfallFaceTerrainDebugWireMesh = null;
        waterfallPlaneDebugWireMesh = null;
        waterfallLipPlaneDebugWireMesh = null;
        riverBedDebugOverlay = null;
        waterfallFaceTerrainDebugOverlay = null;
        waterfallPlaneDebugOverlay = null;
        waterfallLipPlaneDebugOverlay = null;
        DestroyUnityObject(runtimeRoot);
        runtimeRoot = null;
        terrainMaterial?.SetTexture("_WorldNormal", null);
        terrainMaterial?.SetTexture("_Occlusion", null);
        seaMaterial?.SetTexture("_SeaMask", null);
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

    private void DestroyRuntimeMaterials()
    {
        DestroyUnityObject(terrainMaterial);
        DestroyUnityObject(grassMaterial);
        DestroyUnityObject(rockMaterial);
        DestroyUnityObject(riverMaterial);
        DestroyUnityObject(seaMaterial);
        DestroyUnityObject(meshEdgeMaterial);
        DestroyUnityObject(waterfallFaceTerrainDebugMaterial);
        DestroyUnityObject(waterfallPlaneDebugMaterial);
        DestroyUnityObject(waterfallLipPlaneDebugMaterial);
        if (ownsCliffNoiseTexture) DestroyUnityObject(cliffNoiseTexture);
        if (ownsRiverNoiseTexture) DestroyUnityObject(riverNoiseTexture);
        if (ownsGrassPatchNoiseTexture) DestroyUnityObject(grassPatchNoiseTexture);
        terrainMaterial = null;
        grassMaterial = null;
        rockMaterial = null;
        riverMaterial = null;
        seaMaterial = null;
        meshEdgeMaterial = null;
        waterfallFaceTerrainDebugMaterial = null;
        waterfallPlaneDebugMaterial = null;
        waterfallLipPlaneDebugMaterial = null;
        cliffNoiseTexture = null;
        riverNoiseTexture = null;
        grassPatchNoiseTexture = null;
        ownsCliffNoiseTexture = false;
        ownsRiverNoiseTexture = false;
        ownsGrassPatchNoiseTexture = false;
    }

    private void ResetAppliedLiveSettings()
    {
        appliedShowRivers = null;
        appliedShowSea = null;
        appliedShowGrass = null;
        appliedShowRocks = null;
        appliedShowMeshEdges = null;
        appliedShowRiverDebugGeometry = null;
        appliedEmitterDebug = null;
        appliedGrassColourA = null;
        appliedGrassColourB = null;
        appliedGrassColourNoiseWorldSize = float.NaN;
        appliedGrassBrightness = float.NaN;
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value != null)
        {
            Destroy(value);
        }
    }

    private static Texture3D CreateCliffNoiseTexture()
    {
        var texture = new Texture3D(
            CliffNoiseDimension,
            CliffNoiseDimension,
            CliffNoiseDimension,
            TextureFormat.RGBA32,
            false)
        {
            name = "Cliff coherent noise",
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Repeat,
        };
        var pixels = new Color[CliffNoiseDimension * CliffNoiseDimension * CliffNoiseDimension];
        var latticeScale = CliffNoiseLatticePeriod / (float)CliffNoiseDimension;
        for (var z = 0; z < CliffNoiseDimension; z++)
        {
            for (var y = 0; y < CliffNoiseDimension; y++)
            {
                for (var x = 0; x < CliffNoiseDimension; x++)
                {
                    var sampleX = (x + 0.5f) * latticeScale;
                    var sampleY = (y + 0.5f) * latticeScale;
                    var sampleZ = (z + 0.5f) * latticeScale;
                    var index = x + CliffNoiseDimension * (y + CliffNoiseDimension * z);
                    pixels[index] = new Color(
                        PeriodicValueNoise(sampleX, sampleY, sampleZ, 0xA341316Cu),
                        PeriodicValueNoise(sampleX, sampleY, sampleZ, 0xC8013EA4u),
                        PeriodicValueNoise(sampleX, sampleY, sampleZ, 0xAD90777Du),
                        1f);
                }
            }
        }
        texture.SetPixels(pixels);
        texture.Apply(false, true);
        return texture;
    }

    private static Texture2D CreateRiverNoiseTexture()
    {
        var texture = new Texture2D(
            RiverNoiseDimension,
            RiverNoiseDimension,
            TextureFormat.RGBA32,
            false,
            true)
        {
            name = "River coherent flow noise",
            filterMode = FilterMode.Bilinear,
            wrapMode = TextureWrapMode.Repeat,
        };
        var pixels = new Color[RiverNoiseDimension * RiverNoiseDimension];
        var latticeScale = RiverNoiseLatticePeriod / (float)RiverNoiseDimension;
        for (var y = 0; y < RiverNoiseDimension; y++)
        {
            for (var x = 0; x < RiverNoiseDimension; x++)
            {
                var sampleX = (x + 0.5f) * latticeScale;
                var sampleY = (y + 0.5f) * latticeScale;
                var index = x + RiverNoiseDimension * y;
                pixels[index] = new Color(
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0x9E3779B9u,
                        RiverNoiseLatticePeriod),
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0xD1B54A35u,
                        RiverNoiseLatticePeriod),
                    0f,
                    1f);
            }
        }
        texture.SetPixels(pixels);
        texture.Apply(false, true);
        return texture;
    }

    private static Texture2D CreateGrassPatchNoiseTexture()
    {
        var texture = new Texture2D(
            GrassPatchNoiseDimension,
            GrassPatchNoiseDimension,
            TextureFormat.RGBA32,
            true,
            true)
        {
            name = "Grass coverage and broad colour noise",
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Repeat,
            anisoLevel = 2,
        };
        var pixels = new Color[GrassPatchNoiseDimension * GrassPatchNoiseDimension];
        var latticeScale = GrassPatchNoiseLatticePeriod
            / (float)GrassPatchNoiseDimension;
        var colourLatticeScale = GrassColourNoiseLatticePeriod
            / (float)GrassPatchNoiseDimension;
        for (var y = 0; y < GrassPatchNoiseDimension; y++)
        {
            for (var x = 0; x < GrassPatchNoiseDimension; x++)
            {
                var sampleX = (x + 0.5f) * latticeScale;
                var sampleY = (y + 0.5f) * latticeScale;
                var colourSampleX = (x + 0.5f) * colourLatticeScale;
                var colourSampleY = (y + 0.5f) * colourLatticeScale;
                var index = x + GrassPatchNoiseDimension * y;
                pixels[index] = new Color(
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0xB5297A4Du,
                        GrassPatchNoiseLatticePeriod),
                    PeriodicNoise2D(
                        sampleX,
                        sampleY,
                        0x68E31DA4u,
                        GrassPatchNoiseLatticePeriod),
                    PeriodicNoise2D(
                        colourSampleX,
                        colourSampleY,
                        0x1B56C4E9u,
                        GrassColourNoiseLatticePeriod),
                    1f);
            }
        }
        texture.SetPixels(pixels);
        texture.Apply(true, true);
        return texture;
    }

    private static float PeriodicNoise2D(float x, float y, uint seed, int period)
    {
        var latticeX = Mathf.FloorToInt(x);
        var latticeY = Mathf.FloorToInt(y);
        var x0 = latticeX % period;
        var y0 = latticeY % period;
        var x1 = (x0 + 1) % period;
        var y1 = (y0 + 1) % period;
        var fadeX = QuinticFade(x - latticeX);
        var fadeY = QuinticFade(y - latticeY);
        var near = Mathf.Lerp(
            LatticeNoise(x0, y0, 0, seed),
            LatticeNoise(x1, y0, 0, seed),
            fadeX);
        var far = Mathf.Lerp(
            LatticeNoise(x0, y1, 0, seed),
            LatticeNoise(x1, y1, 0, seed),
            fadeX);
        return Mathf.Lerp(near, far, fadeY);
    }

    private static float PeriodicValueNoise(float x, float y, float z, uint seed)
    {
        var latticeX = Mathf.FloorToInt(x);
        var latticeY = Mathf.FloorToInt(y);
        var latticeZ = Mathf.FloorToInt(z);
        var x0 = latticeX % CliffNoiseLatticePeriod;
        var y0 = latticeY % CliffNoiseLatticePeriod;
        var z0 = latticeZ % CliffNoiseLatticePeriod;
        var x1 = (x0 + 1) % CliffNoiseLatticePeriod;
        var y1 = (y0 + 1) % CliffNoiseLatticePeriod;
        var z1 = (z0 + 1) % CliffNoiseLatticePeriod;

        var fadeX = QuinticFade(x - latticeX);
        var fadeY = QuinticFade(y - latticeY);
        var fadeZ = QuinticFade(z - latticeZ);
        var lowerNear = Mathf.Lerp(
            LatticeNoise(x0, y0, z0, seed),
            LatticeNoise(x1, y0, z0, seed),
            fadeX);
        var lowerFar = Mathf.Lerp(
            LatticeNoise(x0, y1, z0, seed),
            LatticeNoise(x1, y1, z0, seed),
            fadeX);
        var upperNear = Mathf.Lerp(
            LatticeNoise(x0, y0, z1, seed),
            LatticeNoise(x1, y0, z1, seed),
            fadeX);
        var upperFar = Mathf.Lerp(
            LatticeNoise(x0, y1, z1, seed),
            LatticeNoise(x1, y1, z1, seed),
            fadeX);
        return Mathf.Lerp(
            Mathf.Lerp(lowerNear, lowerFar, fadeY),
            Mathf.Lerp(upperNear, upperFar, fadeY),
            fadeZ);
    }

    private static float LatticeNoise(int x, int y, int z, uint seed)
    {
        unchecked
        {
            var value = (uint)x * 0x8DA6B343u;
            value ^= (uint)y * 0xD8163841u;
            value ^= (uint)z * 0xCB1AB31Fu;
            return HashNoise(value ^ seed);
        }
    }

    private static float QuinticFade(float value)
    {
        return value * value * value * (value * (value * 6f - 15f) + 10f);
    }

    private static float HashNoise(uint value)
    {
        value ^= value >> 16;
        value *= 0x7FEB352Du;
        value ^= value >> 15;
        value *= 0x846CA68Bu;
        value ^= value >> 16;
        return (value & 0x00FFFFFFu) / 16777215f;
    }

    private static Material CreateMaterial(
        string shaderName,
        Color color,
        Material template,
        float worldSize)
    {
        Material material;
        if (template != null)
        {
            material = new Material(template);
        }
        else
        {
            var shader = Shader.Find(shaderName) ?? Shader.Find("Standard");
            if (shader == null)
            {
                throw new InvalidOperationException($"Could not find shader '{shaderName}'.");
            }
            material = new Material(shader);
            material.color = color;
            if (material.HasProperty("_BaseColor"))
            {
                material.SetColor("_BaseColor", color);
            }
        }
        material.name = $"{shaderName} (Island Instance)";
        if (material.HasProperty("_WorldSize"))
        {
            material.SetFloat("_WorldSize", worldSize);
        }

        return material;
    }

    private static Color ApplyAverageTextureColor(
        Material material,
        string textureProperty,
        string colorProperty,
        Color defaultColor)
    {
        if (material == null || !material.HasProperty(colorProperty))
        {
            return defaultColor;
        }

        var fallbackColor = material.GetColor(colorProperty);
        if (!material.HasProperty(textureProperty))
        {
            return fallbackColor;
        }

        var colorMap = material.GetTexture(textureProperty);
        if (colorMap == null)
        {
            return fallbackColor;
        }

        var averageColor = CalculateAverageTextureColor(colorMap, fallbackColor);
        material.SetColor(colorProperty, averageColor);
        return averageColor;
    }

    private static Color CalculateAverageTextureColor(Texture texture, Color fallbackColor)
    {
        var width = Mathf.Min(Mathf.Max(texture.width, 1), AverageColourSampleDimension);
        var height = Mathf.Min(Mathf.Max(texture.height, 1), AverageColourSampleDimension);
        var previousRenderTarget = RenderTexture.active;
        RenderTexture sampleTarget = null;
        Texture2D readableSample = null;

        try
        {
            sampleTarget = RenderTexture.GetTemporary(
                width,
                height,
                0,
                RenderTextureFormat.ARGB32,
                RenderTextureReadWrite.Default);
            sampleTarget.filterMode = FilterMode.Bilinear;
            Graphics.Blit(texture, sampleTarget);

            RenderTexture.active = sampleTarget;
            readableSample = new Texture2D(
                width,
                height,
                TextureFormat.RGBA32,
                false);
            readableSample.ReadPixels(new Rect(0f, 0f, width, height), 0, 0, false);
            readableSample.Apply(false, false);

            var pixels = readableSample.GetPixels32();
            ulong red = 0;
            ulong green = 0;
            ulong blue = 0;
            foreach (var pixel in pixels)
            {
                red += pixel.r;
                green += pixel.g;
                blue += pixel.b;
            }

            var inverseTotal = 1f / (pixels.Length * 255f);
            return new Color(
                red * inverseTotal,
                green * inverseTotal,
                blue * inverseTotal,
                fallbackColor.a);
        }
        catch (Exception exception)
        {
            Debug.LogWarning(
                $"Could not calculate the average color of '{texture.name}': "
                    + exception.Message);
            return fallbackColor;
        }
        finally
        {
            RenderTexture.active = previousRenderTarget;
            DestroyUnityObject(readableSample);
            if (sampleTarget != null)
            {
                RenderTexture.ReleaseTemporary(sampleTarget);
            }
        }
    }

#if UNITY_EDITOR
    public static void BatchValidateNativeInterop()
    {
        var options = new MotuNative.Options
        {
            maxZ = 0.2f,
            waterRatio = 0.6f,
            slopeMultiplier = 1.3f,
            coastalSlopeMultiplier = 1f,
            hydraulicErosionStrength = 1f,
            hydraulicDepositionStrength = 1.5f,
            hydraulicDepositionSlopeDegrees = 12f,
            riverSourceCatchmentHectares = 0.05f,
            riverSourceSteepMultiplier = 4f,
            riverSourceElevationBoost = 9f,
        };
        var handle = MotuNative.CreateMotu(2018, ref options);
        if (handle == IntPtr.Zero)
        {
            throw new InvalidOperationException("Native validation could not generate an island.");
        }

        try
        {
            const int validationMapDimension = 32;
            var validationMaps = PrepareSurfaceMaps(handle, validationMapDimension);
            var validationSeaMask = PrepareSeaMask(handle, validationMapDimension);
            if (validationSeaMask.rg.Length
                != validationMapDimension * validationMapDimension * 2)
            {
                throw new InvalidOperationException(
                    "Native sea mask byte count does not match its RG dimensions.");
            }
            var hasTerrainNormal = false;
            for (var index = 0; index < validationMaps.normalRgb.Length; index += 3)
            {
                if (validationMaps.normalRgb[index] != 127
                    || validationMaps.normalRgb[index + 1] != 127
                    || validationMaps.normalRgb[index + 2] != 255)
                {
                    hasTerrainNormal = true;
                    break;
                }
            }
            if (!hasTerrainNormal)
            {
                throw new InvalidOperationException(
                    "Native LOD 0 surface maps contain only a flat normal.");
            }

            var terrainShader = Shader.Find("Motu/Terrain Unified");
            if (terrainShader == null
                || !terrainShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(terrainShader))
            {
                throw new InvalidOperationException(
                    "The unified terrain shader is missing or unsupported.");
            }
            var terrainMaterial = new Material(terrainShader);
            try
            {
                if (!terrainMaterial.HasProperty("_WorldNormal")
                    || !terrainMaterial.HasProperty("_WorldNormalWeight")
                    || !terrainMaterial.HasProperty("_Occlusion")
                    || !terrainMaterial.HasProperty("_RockColor")
                    || !terrainMaterial.HasProperty("_RockAlbedoMap")
                    || !terrainMaterial.HasProperty("_RockNormalMap")
                    || !terrainMaterial.HasProperty("_RockMaskMap")
                    || !terrainMaterial.HasProperty("_RockTextureWorldSize")
                    || !terrainMaterial.HasProperty("_RockNormalMapStrength")
                    || !terrainMaterial.HasProperty("_RockHeightBlendStrength")
                    || !terrainMaterial.HasProperty("_RockTextureOcclusionStrength")
                    || !terrainMaterial.HasProperty("_RiverBedColor")
                    || !terrainMaterial.HasProperty("_RiverBedAlbedoMap")
                    || !terrainMaterial.HasProperty("_RiverBedNormalMap")
                    || !terrainMaterial.HasProperty("_RiverBedMaskMap")
                    || !terrainMaterial.HasProperty("_RiverBedTextureWorldSize")
                    || !terrainMaterial.HasProperty("_RiverBedNormalMapStrength")
                    || !terrainMaterial.HasProperty("_RiverBedHeightBlendStrength")
                    || !terrainMaterial.HasProperty("_RiverBedTextureOcclusionStrength")
                    || !terrainMaterial.HasProperty("_CliffNoise3D")
                    || !terrainMaterial.HasProperty("_RockPatchNoiseDetailScale")
                    || !terrainMaterial.HasProperty("_CliffNormalStrength")
                    || !terrainMaterial.HasProperty("_GrassNormalDetailScale")
                    || !terrainMaterial.HasProperty("_SandNormalDetailScale")
                    || !terrainMaterial.HasProperty("_SnowNormalDetailScale")
                    || !terrainMaterial.HasProperty("_DirtNormalStrength")
                    || !terrainMaterial.HasProperty("_GrassNormalStrength")
                    || !terrainMaterial.HasProperty("_SandNormalStrength")
                    || !terrainMaterial.HasProperty("_SnowNormalStrength")
                    || !terrainMaterial.HasProperty("_GrassThinDepositColor")
                    || !terrainMaterial.HasProperty("_GrassColorA")
                    || !terrainMaterial.HasProperty("_GrassColorB")
                    || !terrainMaterial.HasProperty("_GrassColorNoiseWorldSize")
                    || !terrainMaterial.HasProperty("_GrassPatchNoise")
                    || !terrainMaterial.HasProperty("_GrassPatchNoiseWorldSize")
                    || !terrainMaterial.HasProperty("_BeachMaximumElevation")
                    || !terrainMaterial.HasProperty("_SandPatchNoiseWorldSize")
                    || !terrainMaterial.HasProperty("_RockBoundaryNoiseStrength")
                    || !terrainMaterial.HasProperty("_SandRockSlopeThreshold")
                    || !terrainMaterial.HasProperty("_GrassPlayerPosition")
                    || !terrainMaterial.HasProperty("_GroundDirtColor")
                    || !terrainMaterial.HasProperty("_GroundDirtCoreRadius")
                    || !terrainMaterial.HasProperty("_GroundDirtFadeWidth")
                    || !terrainMaterial.HasProperty("_SnowMacroNoiseMetres"))
                {
                    throw new InvalidOperationException(
                        "The unified terrain shader is missing its shared map properties.");
                }
                var cliffNoise = CreateCliffNoiseTexture();
                try
                {
                    terrainMaterial.SetTexture("_CliffNoise3D", cliffNoise);
                    if (cliffNoise.width != CliffNoiseDimension
                        || cliffNoise.height != CliffNoiseDimension
                        || cliffNoise.depth != CliffNoiseDimension)
                    {
                        throw new InvalidOperationException(
                            "The cliff noise texture has invalid dimensions.");
                    }
                }
                finally
                {
                    DestroyImmediate(cliffNoise);
                }
            }
            finally
            {
                DestroyImmediate(terrainMaterial);
            }

            var rockShader = Shader.Find("Motu/Rock Decoration");
            if (rockShader == null
                || !rockShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(rockShader))
            {
                throw new InvalidOperationException(
                    "The rock decoration shader is missing or unsupported.");
            }
            var validationRockMaterial = new Material(rockShader);
            try
            {
                if (!validationRockMaterial.HasProperty("_RockColor")
                    || !validationRockMaterial.HasProperty("_RockTint")
                    || !validationRockMaterial.HasProperty("_CliffNoise3D")
                    || !validationRockMaterial.HasProperty("_CliffNoisePeriod")
                    || !validationRockMaterial.HasProperty("_CliffNoiseDetailScale")
                    || !validationRockMaterial.HasProperty("_CliffNormalStrength"))
                {
                    throw new InvalidOperationException(
                        "The rock decoration shader is missing its shared geology properties.");
                }
            }
            finally
            {
                DestroyImmediate(validationRockMaterial);
            }

            var riverWaterShader = Shader.Find("Motu/River Water");
            if (riverWaterShader == null
                || !riverWaterShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(riverWaterShader))
            {
                throw new InvalidOperationException(
                    "The animated river-water shader is missing or unsupported.");
            }
            var riverWaterMaterial = new Material(riverWaterShader);
            try
            {
                if (!riverWaterMaterial.HasProperty("_NoiseTex")
                    || !riverWaterMaterial.HasProperty("_CoarseNoiseWorldSize")
                    || !riverWaterMaterial.HasProperty("_FineNoiseWorldSize")
                    || !riverWaterMaterial.HasProperty("_CoarseFlowSpeed")
                    || !riverWaterMaterial.HasProperty("_FineFlowSpeed")
                    || !riverWaterMaterial.HasProperty("_WorldSize")
                    || !riverWaterMaterial.HasProperty("_ShallowOpacity")
                    || !riverWaterMaterial.HasProperty("_OpacityDepth")
                    || !riverWaterMaterial.HasProperty("_EstuaryStrength")
                    || !riverWaterMaterial.HasProperty("_EstuaryColor")
                    || !riverWaterMaterial.HasProperty("_EstuaryBlendHeight")
                    || !riverWaterMaterial.HasProperty("_SeaLevel")
                    || !riverWaterMaterial.HasProperty("_ReflectionColor")
                    || !riverWaterMaterial.HasProperty("_ShoreWaveStrength")
                    || !riverWaterMaterial.HasProperty("_WhitewaterStrength")
                    || !riverWaterMaterial.HasProperty("_WhitewaterSlopeStart")
                    || !riverWaterMaterial.HasProperty("_WhitewaterSlopeFull")
                    || riverWaterMaterial.HasProperty("_SeaMask"))
                {
                    throw new InvalidOperationException(
                        "The river-water shader has invalid river or sea properties.");
                }
                var riverNoise = CreateRiverNoiseTexture();
                try
                {
                    riverWaterMaterial.SetTexture("_NoiseTex", riverNoise);
                    if (riverNoise.width != RiverNoiseDimension
                        || riverNoise.height != RiverNoiseDimension)
                    {
                        throw new InvalidOperationException(
                            "The river noise texture has invalid dimensions.");
                    }
                }
                finally
                {
                    DestroyImmediate(riverNoise);
                }
            }
            finally
            {
                DestroyImmediate(riverWaterMaterial);
            }

            var seaWaterShader = Shader.Find("Motu/Sea Water");
            if (seaWaterShader == null
                || !seaWaterShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(seaWaterShader))
            {
                throw new InvalidOperationException(
                    "The animated sea-water shader is missing or unsupported.");
            }
            var seaWaterMaterial = new Material(seaWaterShader);
            try
            {
                if (!seaWaterMaterial.HasProperty("_NoiseTex")
                    || !seaWaterMaterial.HasProperty("_SeaMask")
                    || !seaWaterMaterial.HasProperty("_WorldSize")
                    || !seaWaterMaterial.HasProperty("_ShallowOpacity")
                    || !seaWaterMaterial.HasProperty("_OpacityDepth")
                    || !seaWaterMaterial.HasProperty("_SiltColor")
                    || !seaWaterMaterial.HasProperty("_ReflectionColor")
                    || !seaWaterMaterial.HasProperty("_ShoreWaveStrength")
                    || !seaWaterMaterial.HasProperty("_ShoreWaveSpacing")
                    || !seaWaterMaterial.HasProperty("_ShoreWaveSpeed")
                    || !seaWaterMaterial.HasProperty("_ShoreWaveDepth")
                    || !seaWaterMaterial.HasProperty("_ShoreWaveNoiseWorldSize")
                    || seaWaterMaterial.HasProperty("_CoarseFlowSpeed")
                    || seaWaterMaterial.HasProperty("_EstuaryStrength")
                    || seaWaterMaterial.HasProperty("_WhitewaterStrength"))
                {
                    throw new InvalidOperationException(
                        "The sea-water shader has invalid sea or river properties.");
                }
            }
            finally
            {
                DestroyImmediate(seaWaterMaterial);
            }

            var waterCameraObject = new GameObject("Water depth validation camera");
            try
            {
                var waterCamera = waterCameraObject.AddComponent<Camera>();
                waterCamera.enabled = false;
                waterCamera.depthTextureMode = DepthTextureMode.DepthNormals;
                EnsureCameraDepthTexture(waterCamera);
                if ((waterCamera.depthTextureMode & DepthTextureMode.Depth) == 0
                    || (waterCamera.depthTextureMode & DepthTextureMode.DepthNormals) == 0)
                {
                    throw new InvalidOperationException(
                        "Island cameras do not retain the depth texture required by sea waves.");
                }
            }
            finally
            {
                DestroyImmediate(waterCameraObject);
            }

            var grassShader = Shader.Find("Motu/Terrain Grass");
            if (grassShader == null
                || !grassShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(grassShader))
            {
                throw new InvalidOperationException(
                    "The terrain grass shader is missing or unsupported.");
            }
            var grassMaterial = new Material(grassShader);
            try
            {
                if (!grassMaterial.HasProperty("_CliffNoise3D")
                    || !grassMaterial.HasProperty("_RockMaskMap")
                    || !grassMaterial.HasProperty("_RockTextureWorldSize")
                    || !grassMaterial.HasProperty("_RockHeightBlendStrength")
                    || !grassMaterial.HasProperty("_RiverBedMaskMap")
                    || !grassMaterial.HasProperty("_RiverBedTextureWorldSize")
                    || !grassMaterial.HasProperty("_RiverBedHeightBlendStrength")
                    || !grassMaterial.HasProperty("_RockPatchNoiseDetailScale")
                    || !grassMaterial.HasProperty("_GrassPatchNoise")
                    || !grassMaterial.HasProperty("_GrassPatchNoiseWorldSize")
                    || !grassMaterial.HasProperty("_GrassColorA")
                    || !grassMaterial.HasProperty("_GrassColorB")
                    || !grassMaterial.HasProperty("_GrassColorNoiseWorldSize")
                    || !grassMaterial.HasProperty("_GrassPlayerPosition")
                    || !grassMaterial.HasProperty("_GrassRadius")
                    || !grassMaterial.HasProperty("_GrassHeight")
                    || !grassMaterial.HasProperty("_GrassBrightness")
                    || !grassMaterial.HasProperty("_GrassLightDirection")
                    || !grassMaterial.HasProperty("_GrassLightColor")
                    || !grassMaterial.HasProperty("_GrassAmbientColor")
                    || !grassMaterial.HasProperty("_BeachMaximumElevation")
                    || !grassMaterial.HasProperty("_SandPatchNoiseWorldSize")
                    || !grassMaterial.HasProperty("_RockBoundaryNoiseStrength")
                    || !grassMaterial.HasProperty("_SnowMacroNoiseMetres"))
                {
                    throw new InvalidOperationException(
                        "The terrain grass shader is missing its required properties.");
                }
                var grassPatchNoise = CreateGrassPatchNoiseTexture();
                try
                {
                    grassMaterial.SetTexture("_GrassPatchNoise", grassPatchNoise);
                    if (grassPatchNoise.width != GrassPatchNoiseDimension
                        || grassPatchNoise.height != GrassPatchNoiseDimension)
                    {
                        throw new InvalidOperationException(
                            "The grass patch noise texture has invalid dimensions.");
                    }
                }
                finally
                {
                    DestroyImmediate(grassPatchNoise);
                }
            }
            finally
            {
                DestroyImmediate(grassMaterial);
            }

            const float lod0ParentResolution = 64f;
            var area = new MotuNative.ExportArea(
                24f / lod0ParentResolution,
                24f / lod0ParentResolution,
                25f / lod0ParentResolution,
                25f / lod0ParentResolution);
            MotuNative.CreateMeshGrid(handle, ref area, 0, 8, 0, out var grid);
            try
            {
                if (grid.handle == IntPtr.Zero || grid.length != 64)
                {
                    throw new InvalidOperationException("Native render-grid layout is invalid.");
                }
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < grid.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(grid.data, index * exportSize));
                    if (nativeMesh.vertices.length == 0
                        || nativeMesh.triangles.length == 0
                        || nativeMesh.uv.length != nativeMesh.vertices.length
                        || nativeMesh.material.length != nativeMesh.vertices.length)
                    {
                        throw new InvalidOperationException("A render tile has invalid geometry or UVs.");
                    }
                    var renderMesh = CopyTerrainMesh(nativeMesh, 0, ValidationWorldSize);
                    DestroyImmediate(renderMesh);
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref grid);
            }

            var riverArea = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
            const int riverResolution = TerrainTileStreamer.Lod1Resolution;
            MotuNative.CreateRiverMeshGrid(
                handle,
                ref riverArea,
                riverResolution,
                out var riverGrid);
            try
            {
                if (riverGrid.handle == IntPtr.Zero
                    || riverGrid.data == IntPtr.Zero
                    || riverGrid.length != riverResolution * riverResolution)
                {
                    throw new InvalidOperationException("Native river-grid layout is invalid.");
                }
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                var foundRiverGeometry = false;
                var minimumRiverV = float.PositiveInfinity;
                var maximumRiverV = float.NegativeInfinity;
                for (var index = 0; index < riverGrid.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(riverGrid.data, index * exportSize));
                    if (nativeMesh.triangles.length == 0)
                    {
                        continue;
                    }
                    foundRiverGeometry = true;
                    if (nativeMesh.uv.length != nativeMesh.vertices.length)
                    {
                        throw new InvalidOperationException(
                            "A sliced river tile has invalid UV coordinates.");
                    }
                    foreach (var riverUv in CopyVector2Array(nativeMesh.uv))
                    {
                        if (!IsFinite(riverUv.x) || !IsFinite(riverUv.y))
                        {
                            throw new InvalidOperationException(
                                "A sliced river tile has invalid flow coordinates.");
                        }
                        minimumRiverV = Mathf.Min(minimumRiverV, riverUv.y);
                        maximumRiverV = Mathf.Max(maximumRiverV, riverUv.y);
                    }
                }
                if (!foundRiverGeometry || maximumRiverV - minimumRiverV < 0.01f)
                {
                    throw new InvalidOperationException(
                        "Native river grid is empty or lacks downstream UV progression.");
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref riverGrid);
            }

            MotuNative.CreateRiverRockMeshGrid(
                handle,
                ref riverArea,
                riverResolution,
                out var riverRockGrid);
            try
            {
                if (riverRockGrid.handle == IntPtr.Zero
                    || riverRockGrid.data == IntPtr.Zero
                    || riverRockGrid.length != riverResolution * riverResolution)
                {
                    throw new InvalidOperationException(
                        "Native river-rock grid layout is invalid.");
                }
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < riverRockGrid.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(riverRockGrid.data, index * exportSize));
                    if (nativeMesh.triangles.length == 0)
                    {
                        continue;
                    }
                    if (nativeMesh.normals.length != nativeMesh.vertices.length)
                    {
                        throw new InvalidOperationException(
                            "A sliced river-rock tile has invalid normals.");
                    }
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref riverRockGrid);
            }

            MotuNative.CreateRiverEmitters(handle, 35f, 2f, out var riverEmitters);
            try
            {
                if (riverEmitters.handle == IntPtr.Zero
                    || riverEmitters.length < 0
                    || (riverEmitters.length > 0 && riverEmitters.data == IntPtr.Zero))
                {
                    throw new InvalidOperationException(
                        "Native rough-water emitter ownership is invalid.");
                }
                var emitterSize = Marshal.SizeOf<MotuNative.RiverEmitterExport>();
                if (emitterSize != sizeof(float) * 7)
                {
                    throw new InvalidOperationException(
                        "Native rough-water emitter record layout is invalid.");
                }
                for (var index = 0; index < riverEmitters.length; index++)
                {
                    var emitter = Marshal.PtrToStructure<MotuNative.RiverEmitterExport>(
                        IntPtr.Add(riverEmitters.data, index * emitterSize));
                    var directionLengthSquared = emitter.direction.x * emitter.direction.x
                        + emitter.direction.y * emitter.direction.y
                        + emitter.direction.z * emitter.direction.z;
                    if (!IsFinite(emitter.position.x)
                        || !IsFinite(emitter.position.y)
                        || !IsFinite(emitter.position.z)
                        || emitter.position.x < 0f
                        || emitter.position.x > 1f
                        || emitter.position.y < 0f
                        || emitter.position.y > 1f
                        || !IsFinite(directionLengthSquared)
                        || Mathf.Abs(directionLengthSquared - 1f) > 0.001f
                        || !IsFinite(emitter.strength)
                        || emitter.strength < 0f
                        || emitter.strength > 1f)
                    {
                        throw new InvalidOperationException(
                            "A native rough-water emitter record is invalid.");
                    }
                }
            }
            finally
            {
                MotuNative.ReleaseRiverEmitters(ref riverEmitters);
            }
            if (riverEmitters.handle != IntPtr.Zero
                || riverEmitters.data != IntPtr.Zero
                || riverEmitters.length != 0)
            {
                throw new InvalidOperationException(
                    "Native rough-water emitter release did not clear ownership.");
            }

            if (Marshal.SizeOf<MotuNative.ExportDecoration>()
                != Marshal.SizeOf<MotuNative.Vector3Array>() * 3
                    + Marshal.SizeOf<MotuNative.UInt32Array>())
            {
                throw new InvalidOperationException(
                    "Native decoration export layout is invalid.");
            }
            MotuNative.GetDecoration(handle, out var nativeDecoration);
            ValidateBorrowedArray(nativeDecoration.trees, "tree");
            ValidateBorrowedArray(nativeDecoration.bushes, "bush");
            ValidateBorrowedArray(nativeDecoration.rocks, "rock");
            ValidateBorrowedArray(
                nativeDecoration.rockAppearanceIds,
                "rock appearance ID");
            var preparedRocks = PrepareRockDecorations(
                handle,
                validationMaps,
                2018,
                ValidationWorldSize);
            var repeatedRocks = PrepareRockDecorations(
                handle,
                validationMaps,
                2018,
                ValidationWorldSize);
            if (preparedRocks.Length == 0
                || preparedRocks.Length != nativeDecoration.rocks.length
                || nativeDecoration.rockAppearanceIds.length
                    != nativeDecoration.rocks.length
                || repeatedRocks.Length != preparedRocks.Length)
            {
                throw new InvalidOperationException(
                    "Native rock decorations are empty or changed while borrowed.");
            }
            for (var index = 0; index < preparedRocks.Length; index++)
            {
                var first = preparedRocks[index];
                var second = repeatedRocks[index];
                if (first.sourceIndex != index
                    || first.sourceIndex != second.sourceIndex
                    || first.appearanceId != second.appearanceId
                    || first.prototypeIndex != second.prototypeIndex
                    || first.isBoulder != second.isBoulder
                    || first.position != second.position
                    || first.normal != second.normal
                    || first.scale != second.scale
                    || first.rotation != second.rotation
                    || first.tint != second.tint
                    || first.embedDepth != second.embedDepth)
                {
                    throw new InvalidOperationException(
                        "Prepared rock decoration appearance is not deterministic.");
                }
            }

            var rockIndex = new RockDecorationIndex(preparedRocks, ValidationWorldSize);
            var seenRocks = new bool[preparedRocks.Length];
            for (var y = 0; y < RockDecorationIndex.Resolution; y++)
            {
                for (var x = 0; x < RockDecorationIndex.Resolution; x++)
                {
                    rockIndex.GetCellRange(x, y, out var start, out var end);
                    for (var order = start; order < end; order++)
                    {
                        var candidateIndex = rockIndex.CandidateIndexAt(order);
                        if (candidateIndex < 0
                            || candidateIndex >= seenRocks.Length
                            || seenRocks[candidateIndex])
                        {
                            throw new InvalidOperationException(
                                "The rock packed index contains an invalid entry.");
                        }
                        seenRocks[candidateIndex] = true;
                    }
                }
            }
            if (Array.Exists(seenRocks, value => !value))
            {
                throw new InvalidOperationException(
                    "The rock packed index does not contain every candidate.");
            }

            using (var prototypes = new RockPrototypeLibrary())
            {
                if (prototypes.Count != RockPrototypeLibrary.PrototypeCount)
                {
                    throw new InvalidOperationException(
                        "The rock prototype library has an invalid size.");
                }
                for (var index = 0; index < prototypes.Count; index++)
                {
                    var mesh = prototypes.MeshAt(index);
                    if (mesh.vertexCount != RockPrototypeLibrary.ExpectedVertexCount
                        || mesh.GetIndexCount(0) / 3
                            != RockPrototypeLibrary.ExpectedTriangleCount)
                    {
                        throw new InvalidOperationException(
                            "A rock prototype has invalid topology.");
                    }
                }
                var seated = prototypes.SeatPosition(preparedRocks[0]);
                if (!IsFinite(seated.x) || !IsFinite(seated.y) || !IsFinite(seated.z))
                {
                    throw new InvalidOperationException(
                        "A prepared rock could not be seated against its surface normal.");
                }
            }

            var indexCandidates = new[]
            {
                new IslandPreparedRiverEmitter(
                    new Vector3(-999.9f, 1f, -999.9f),
                    Vector3.up,
                    0.25f),
                new IslandPreparedRiverEmitter(Vector3.zero, Vector3.forward, 0.5f),
                new IslandPreparedRiverEmitter(
                    new Vector3(999.9f, 2f, 999.9f),
                    Vector3.right,
                    1f),
            };
            var emitterIndex = new RiverEmitterIndex(indexCandidates, ValidationWorldSize);
            var seen = new bool[indexCandidates.Length];
            for (var y = 0; y < RiverEmitterIndex.Resolution; y++)
            {
                for (var x = 0; x < RiverEmitterIndex.Resolution; x++)
                {
                    emitterIndex.GetCellRange(x, y, out var start, out var end);
                    for (var order = start; order < end; order++)
                    {
                        var candidateIndex = emitterIndex.CandidateIndexAt(order);
                        if (candidateIndex < 0
                            || candidateIndex >= seen.Length
                            || seen[candidateIndex])
                        {
                            throw new InvalidOperationException(
                                "The rough-water packed index contains an invalid entry.");
                        }
                        seen[candidateIndex] = true;
                    }
                }
            }
            if (Array.Exists(seen, value => !value)
                || emitterIndex.CellAt(indexCandidates[0].position) != Vector2Int.zero
                || emitterIndex.CellAt(indexCandidates[2].position)
                    != new Vector2Int(
                        RiverEmitterIndex.Resolution - 1,
                        RiverEmitterIndex.Resolution - 1))
            {
                throw new InvalidOperationException(
                    "The rough-water packed index does not cover the world bounds.");
            }

            var particleRoot = new GameObject("Rough water pool validation");
            try
            {
                var pool = particleRoot.AddComponent<RiverParticlePool>();
                pool.Initialize(indexCandidates, ValidationWorldSize, true);
                if (pool.PoolCount != 32 || pool.CreatedSystemCount != 32)
                {
                    throw new InvalidOperationException(
                        "The rough-water particle pool is not fixed at 32 systems.");
                }
                pool.ClearPlayerFocus();
                pool.DisposePool();
            }
            finally
            {
                DestroyImmediate(particleRoot);
            }

            const int validationSamplesPerTile = TerrainTileStreamer.ColliderSamplesPerTile;
            var heightMapPointer = MotuNative.CreateTerrainColliderHeightMap(
                handle,
                validationSamplesPerTile);
            try
            {
                if (heightMapPointer == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "Native terrain-collider height-map export is null.");
                }
                var nativeHeightMap = Marshal.PtrToStructure<
                    MotuNative.ExportHeightMapWithSeaLevel>(heightMapPointer);
                var expectedDimension = TerrainTileStreamer.Lod1Resolution
                    * (validationSamplesPerTile - 1)
                    + 1;
                if (nativeHeightMap.width != expectedDimension
                    || nativeHeightMap.height != expectedDimension
                    || nativeHeightMap.data == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "Native terrain-collider height-map dimensions are invalid.");
                }
                var heights = new float[checked(expectedDimension * expectedDimension)];
                Marshal.Copy(nativeHeightMap.data, heights, 0, heights.Length);
                var preparedHeightMap = new IslandPreparedColliderHeightMap(
                    expectedDimension,
                    validationSamplesPerTile,
                    heights,
                    ValidationWorldSize);
                var leftTile = preparedHeightMap.CopyTileHeights(Vector2Int.zero);
                var rightTile = preparedHeightMap.CopyTileHeights(new Vector2Int(1, 0));
                for (var row = 0; row < validationSamplesPerTile; row++)
                {
                    if (leftTile[row, validationSamplesPerTile - 1] != rightTile[row, 0])
                    {
                        throw new InvalidOperationException(
                            "Adjacent terrain-collider height maps do not share an identical edge.");
                    }
                }

                var validationTerrainData = new TerrainData
                {
                    heightmapResolution = validationSamplesPerTile,
                    size = new Vector3(
                        ValidationWorldSize / TerrainTileStreamer.Lod1Resolution,
                        preparedHeightMap.verticalSize,
                        ValidationWorldSize / TerrainTileStreamer.Lod1Resolution),
                };
                var validationTerrainObject = new GameObject(
                    "Terrain collider validation");
                try
                {
                    validationTerrainData.SetHeights(0, 0, leftTile);
                    validationTerrainObject.transform.position = new Vector3(
                        -ValidationWorldSize * 0.5f,
                        preparedHeightMap.verticalOrigin,
                        -ValidationWorldSize * 0.5f);
                    var hiddenTerrain = validationTerrainObject.AddComponent<Terrain>();
                    hiddenTerrain.terrainData = validationTerrainData;
                    hiddenTerrain.drawHeightmap = false;
                    hiddenTerrain.enabled = false;
                    var terrainCollider = validationTerrainObject.AddComponent<TerrainCollider>();
                    terrainCollider.terrainData = validationTerrainData;
                    Physics.SyncTransforms();
                    var tileSize = ValidationWorldSize / TerrainTileStreamer.Lod1Resolution;
                    var ray = new Ray(
                        validationTerrainObject.transform.position
                            + new Vector3(tileSize * 0.5f, ValidationWorldSize, tileSize * 0.5f),
                        Vector3.down);
                    if (!terrainCollider.Raycast(ray, out _, ValidationWorldSize * 2f))
                    {
                        throw new InvalidOperationException(
                            "The hidden Unity TerrainCollider did not hit its prepared heightfield.");
                    }
                }
                finally
                {
                    DestroyImmediate(validationTerrainObject);
                    DestroyImmediate(validationTerrainData);
                }

                var streamingValidationObject = new GameObject(
                    "Terrain collider neighbourhood validation");
                try
                {
                    streamingValidationObject
                        .AddComponent<TerrainTileStreamer>()
                        .ValidateColliderStreaming(preparedHeightMap, ValidationWorldSize);
                }
                finally
                {
                    DestroyImmediate(streamingValidationObject);
                }
            }
            finally
            {
                MotuNative.ReleaseTerrainColliderHeightMap(heightMapPointer);
            }
        }
        finally
        {
            MotuNative.ReleaseMotu(handle);
        }
        Debug.Log("Motu native mesh, terrain collider, and material validation passed.");
    }

    private static bool IsFinite(float value)
    {
        return !float.IsNaN(value) && !float.IsInfinity(value);
    }
#endif
}
