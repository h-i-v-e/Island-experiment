using System;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Rendering;
using Debug = UnityEngine.Debug;

public sealed class IslandViewer : MonoBehaviour
{
    private const float TerrainScale = 2000f;
    private const float SeaHeight = 0f;
    private const float MinimumWaterRatio = 0.6f;
    private const float DefaultWaterRatio = 0.95f;
    private const float MinimumRiverSourceCatchmentFraction = 0.0002f;
    private const float MaximumRiverSourceCatchmentFraction = 0.02f;
    private const int SurfaceMapDimension = 2048;
    private const int CliffNoiseDimension = 64;
    private const int CliffNoiseLatticePeriod = 16;
    private const int RiverNoiseDimension = 256;
    private const int RiverNoiseLatticePeriod = 32;
    private const int GrassPatchNoiseDimension = 256;
    private const int GrassPatchNoiseLatticePeriod = 64;
    private const float GrassPatchNoiseWorldSizeMetres = 32f;
    private const float RockPatchNoiseDetailScale = 8f;
    private const float SandMaximumElevationMetres = 3f;
    private const float SandPatchNoiseWorldSizeMetres = 32f;
    private const float ClickDragTolerance = 6f;
    private const float HazeStartDistance = 0f;
    private const float HazeEndDistance = 1000f;
    private const float RiverEmitterSharpnessDegrees = 35f;
    private const float RiverEmitterSpacingMetres = 1.5f;
    private const float EstuaryBlendHeightMetres = 2f;

    private IntPtr islandHandle;
    private TerrainTileStreamer terrainStreamer;
    private GameObject seaObject;
    private Camera viewerCamera;
    private FirstPersonController firstPersonController;
    private Material terrainMaterial;
    private Material grassMaterial;
    private Texture2D terrainNormalTexture;
    private Texture2D terrainOcclusionTexture;
    private Texture2D seaMaskTexture;
    private Texture3D cliffNoiseTexture;
    private Texture2D riverNoiseTexture;
    private Texture2D grassPatchNoiseTexture;
    private Material riverMaterial;
    private Material seaMaterial;
    private string seedText = "666";
    private int seed = 666;
    private float maxHeight = 0.2f;
    private float waterRatio = DefaultWaterRatio;
    private float slopeMultiplier = 1.3f;
    private float coastalSlopeMultiplier = 1f;
    private float hydraulicErosionStrength = 1f;
    private float hydraulicDepositionStrength = 1.5f;
    private float hydraulicDepositionSlopeDegrees = 12f;
    private float riverSourceCatchmentFraction = 0.002f;
    private float riverSourceSteepMultiplier = 4f;
    private float grassBrightness = 1.35f;
    private string status = "Ready";
    private bool showRivers = true;
    private bool showSea = true;
    private bool showMeshEdges;
    private bool useRenderCollider = true;
    private bool showRiverEmitterDebug;
    private bool clickCandidate;
    private Vector2 clickStart;
    private CancellationTokenSource generationCancellation;
    private Stopwatch generationTimer;
    private bool generationInProgress;
    private bool isDestroyed;
    private bool distanceHazeEnabled;

    internal sealed class PreparedMesh
    {
        internal readonly Vector3[] vertices;
        internal readonly Vector3[] normals;
        internal readonly int[] triangles;
        internal readonly Vector2[] uv;
        internal readonly Color[] material;

        internal PreparedMesh(
            Vector3[] vertices,
            Vector3[] normals,
            int[] triangles,
            Vector2[] uv,
            Color[] material)
        {
            this.vertices = vertices;
            this.normals = normals;
            this.triangles = triangles;
            this.uv = uv;
            this.material = material;
        }
    }

    internal readonly struct PreparedRiverEmitter
    {
        internal readonly Vector3 position;
        internal readonly Vector3 direction;
        internal readonly float strength;

        internal PreparedRiverEmitter(Vector3 position, Vector3 direction, float strength)
        {
            this.position = position;
            this.direction = direction;
            this.strength = strength;
        }
    }

    private sealed class PreparedSurfaceMaps
    {
        internal readonly int dimension;
        internal readonly byte[] normalRgb;
        internal readonly byte[] occlusion;

        internal PreparedSurfaceMaps(int dimension, byte[] normalRgb, byte[] occlusion)
        {
            this.dimension = dimension;
            this.normalRgb = normalRgb;
            this.occlusion = occlusion;
        }
    }

    private sealed class PreparedSeaMask
    {
        internal readonly int dimension;
        internal readonly byte[] rg;

        internal PreparedSeaMask(int dimension, byte[] rg)
        {
            this.dimension = dimension;
            this.rg = rg;
        }
    }

    private sealed class PreparedIsland : IDisposable
    {
        internal IntPtr handle;
        internal readonly PreparedSurfaceMaps surfaceMaps;
        internal readonly PreparedSeaMask seaMask;
        internal readonly PreparedMesh[] overviewTiles;
        internal readonly PreparedMesh[] riverTiles;
        internal readonly PreparedRiverEmitter[] riverEmitters;

        internal PreparedIsland(
            IntPtr handle,
            PreparedSurfaceMaps surfaceMaps,
            PreparedSeaMask seaMask,
            PreparedMesh[] overviewTiles,
            PreparedMesh[] riverTiles,
            PreparedRiverEmitter[] riverEmitters)
        {
            this.handle = handle;
            this.surfaceMaps = surfaceMaps;
            this.seaMask = seaMask;
            this.overviewTiles = overviewTiles;
            this.riverTiles = riverTiles;
            this.riverEmitters = riverEmitters;
        }

        internal IntPtr TakeHandle()
        {
            var result = handle;
            handle = IntPtr.Zero;
            return result;
        }

        public void Dispose()
        {
            if (handle == IntPtr.Zero)
            {
                return;
            }
            MotuNative.ReleaseMotu(handle);
            handle = IntPtr.Zero;
        }
    }

    [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
    private static void Bootstrap()
    {
        if (FindAnyObjectByType<IslandViewer>() != null)
        {
            return;
        }

        new GameObject("Island Viewer").AddComponent<IslandViewer>();
    }

    private void Start()
    {
        BuildEnvironment();
        Generate();
    }

    private void OnEnable()
    {
        Camera.onPreRender += BeginCameraRender;
        Camera.onPostRender += EndCameraRender;
    }

    private void OnDisable()
    {
        Camera.onPreRender -= BeginCameraRender;
        Camera.onPostRender -= EndCameraRender;
        GL.wireframe = false;
    }

    private void BeginCameraRender(Camera camera)
    {
        if (camera == viewerCamera)
        {
            GL.wireframe = showMeshEdges;
        }
    }

    private void EndCameraRender(Camera camera)
    {
        if (camera == viewerCamera)
        {
            GL.wireframe = false;
        }
    }

    private void Update()
    {
        SetDistanceHaze(
            firstPersonController != null && firstPersonController.IsActive);

        if (Input.GetKeyDown(KeyCode.M))
        {
            showMeshEdges = !showMeshEdges;
        }

        if (firstPersonController == null
            || firstPersonController.IsActive
            || terrainStreamer == null
            || viewerCamera == null)
        {
            return;
        }

        if (Input.GetMouseButtonDown(0))
        {
            clickStart = Input.mousePosition;
            var guiPosition = new Vector2(clickStart.x, Screen.height - clickStart.y);
            clickCandidate = !new Rect(16f, 16f, 500f, 684f).Contains(guiPosition);
        }

        if (!Input.GetMouseButtonUp(0) || !clickCandidate)
        {
            return;
        }

        clickCandidate = false;
        var releasedAt = (Vector2)Input.mousePosition;
        if ((releasedAt - clickStart).sqrMagnitude
            > ClickDragTolerance * ClickDragTolerance)
        {
            return;
        }

        var ray = viewerCamera.ScreenPointToRay(releasedAt);
        if (terrainStreamer.TryRaycastOverview(ray, out var groundPoint))
        {
            terrainStreamer.SetPlayerPosition(groundPoint);
            terrainStreamer.TrySnapToCurrentCollider(groundPoint, out groundPoint);
            firstPersonController.Enter(groundPoint);
        }
    }

    private void OnDestroy()
    {
        isDestroyed = true;
        SetDistanceHaze(false);
        generationCancellation?.Cancel();
        firstPersonController?.Exit();
        ClearGeneratedContent();
        DestroyUnityObject(terrainMaterial);
        DestroyUnityObject(grassMaterial);
        DestroyUnityObject(cliffNoiseTexture);
        DestroyUnityObject(riverNoiseTexture);
        DestroyUnityObject(grassPatchNoiseTexture);
        DestroyUnityObject(riverMaterial);
        DestroyUnityObject(seaMaterial);
    }

    private void BuildEnvironment()
    {
        var skyColor = new Color(0.49f, 0.68f, 0.82f);
        RenderSettings.ambientMode = AmbientMode.Flat;
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);
        RenderSettings.fog = false;
        RenderSettings.fogMode = FogMode.Linear;
        RenderSettings.fogColor = skyColor;
        RenderSettings.fogStartDistance = HazeStartDistance;
        RenderSettings.fogEndDistance = HazeEndDistance;

        var lightObject = new GameObject("Sun");
        lightObject.transform.SetParent(transform, false);
        lightObject.transform.rotation = Quaternion.Euler(50f, -35f, 0f);
        var sun = lightObject.AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.intensity = 1.25f;
        sun.color = new Color(1f, 0.94f, 0.82f);

        var cameraObject = new GameObject("Orbit Camera");
        cameraObject.transform.SetParent(transform, false);
        viewerCamera = cameraObject.AddComponent<Camera>();
        viewerCamera.depthTextureMode |= DepthTextureMode.Depth;
        viewerCamera.clearFlags = CameraClearFlags.SolidColor;
        viewerCamera.backgroundColor = skyColor;
        viewerCamera.nearClipPlane = 0.05f;
        viewerCamera.farClipPlane = TerrainScale * 8f;
        var orbitCamera = cameraObject.AddComponent<OrbitCamera>();
        orbitCamera.Configure(
            new Vector3(0f, maxHeight * TerrainScale * 0.3f, 0f),
            TerrainScale * 1.15f);
        firstPersonController = cameraObject.AddComponent<FirstPersonController>();
        firstPersonController.Configure(orbitCamera);

        terrainMaterial = CreateMaterial(
            "Motu/Terrain Unified",
            Color.white);
        cliffNoiseTexture = CreateCliffNoiseTexture();
        terrainMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        grassMaterial = CreateMaterial("Motu/Terrain Grass", Color.white);
        grassMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
        terrainMaterial.SetFloat(
            "_RockPatchNoiseDetailScale",
            RockPatchNoiseDetailScale);
        grassMaterial.SetFloat(
            "_RockPatchNoiseDetailScale",
            RockPatchNoiseDetailScale);
        terrainMaterial.SetFloat(
            "_BeachMaximumElevation",
            SandMaximumElevationMetres);
        grassMaterial.SetFloat(
            "_BeachMaximumElevation",
            SandMaximumElevationMetres);
        terrainMaterial.SetFloat(
            "_SandPatchNoiseWorldSize",
            SandPatchNoiseWorldSizeMetres);
        grassMaterial.SetFloat(
            "_SandPatchNoiseWorldSize",
            SandPatchNoiseWorldSizeMetres);
        grassPatchNoiseTexture = CreateGrassPatchNoiseTexture();
        terrainMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        terrainMaterial.SetFloat(
            "_GrassPatchNoiseWorldSize",
            GrassPatchNoiseWorldSizeMetres);
        grassMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
        grassMaterial.SetFloat(
            "_GrassPatchNoiseWorldSize",
            GrassPatchNoiseWorldSizeMetres);
        grassMaterial.SetColor(
            "_GrassRootColor",
            new Color(0.14f, 0.34f, 0.11f, 1f));
        grassMaterial.SetColor(
            "_GrassTipColor",
            new Color(0.26f, 0.62f, 0.21f, 1f));
        grassMaterial.SetFloat("_GrassBrightness", grassBrightness);
        grassMaterial.SetVector("_GrassLightDirection", -sun.transform.forward);
        grassMaterial.SetColor("_GrassLightColor", sun.color * sun.intensity);
        grassMaterial.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
        riverNoiseTexture = CreateRiverNoiseTexture();
        var waterColor = new Color(0.03f, 0.28f, 0.55f, 1f);
        var sandColor = new Color(0.62f, 0.57f, 0.34f, 1f);
        const float shallowWaterOpacity = 0.25f;
        const float fullOpacityDepth = 5f;
        riverMaterial = CreateMaterial("Motu/Water", waterColor);
        riverMaterial.renderQueue = (int)RenderQueue.Transparent + 10;
        riverMaterial.SetTexture("_NoiseTex", riverNoiseTexture);
        riverMaterial.SetFloat("_WhitewaterStrength", 0.9f);
        riverMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
        riverMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
        riverMaterial.SetFloat("_EstuaryStrength", 1f);
        riverMaterial.SetColor(
            "_EstuaryColor",
            Color.Lerp(sandColor, waterColor, 0.5f));
        riverMaterial.SetFloat("_EstuaryBlendHeight", EstuaryBlendHeightMetres);
        riverMaterial.SetFloat("_SeaLevel", SeaHeight);
        riverMaterial.SetColor("_ReflectionColor", skyColor);
        riverMaterial.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(skyColor, Color.white, 0.35f));
        riverMaterial.SetFloat("_ReflectionStrength", 0.45f);
        riverMaterial.SetFloat("_SunGlintStrength", 0.55f);
        seaMaterial = CreateMaterial("Motu/Water", waterColor);
        seaMaterial.renderQueue = (int)RenderQueue.Transparent;
        seaMaterial.SetFloat("_WhitewaterStrength", 0f);
        seaMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
        seaMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
        seaMaterial.SetFloat("_EstuaryStrength", 0f);
        seaMaterial.SetColor(
            "_EstuaryColor",
            Color.Lerp(sandColor, waterColor, 0.5f));
        seaMaterial.SetColor("_ReflectionColor", skyColor);
        seaMaterial.SetColor(
            "_ReflectionHorizonColor",
            Color.Lerp(skyColor, Color.white, 0.35f));
        seaMaterial.SetFloat("_ReflectionStrength", 0.65f);
        seaMaterial.SetFloat("_SunGlintStrength", 0.8f);
    }

    private void SetDistanceHaze(bool enabled)
    {
        if (distanceHazeEnabled == enabled && RenderSettings.fog == enabled)
        {
            return;
        }
        distanceHazeEnabled = enabled;
        RenderSettings.fog = enabled;
    }

    private async void Generate()
    {
        if (generationInProgress)
        {
            return;
        }
        if (!int.TryParse(seedText, NumberStyles.Integer, CultureInfo.InvariantCulture, out seed))
        {
            status = "Seed must be a whole number.";
            return;
        }

        status = "Generating island in background...";
        firstPersonController?.Exit();
        generationInProgress = true;
        generationTimer = Stopwatch.StartNew();
        var cancellation = new CancellationTokenSource();
        generationCancellation = cancellation;
        PreparedIsland prepared = null;

        try
        {
            var options = new MotuNative.Options
            {
                maxZ = maxHeight,
                waterRatio = waterRatio,
                slopeMultiplier = slopeMultiplier,
                coastalSlopeMultiplier = coastalSlopeMultiplier,
                hydraulicErosionStrength = hydraulicErosionStrength,
                hydraulicDepositionStrength = hydraulicDepositionStrength,
                hydraulicDepositionSlopeDegrees = hydraulicDepositionSlopeDegrees,
                riverSourceCatchmentFraction = riverSourceCatchmentFraction,
                riverSourceSteepMultiplier = riverSourceSteepMultiplier,
            };

            prepared = await Task.Run(
                () => PrepareIsland(seed, options, cancellation.Token),
                cancellation.Token);
            cancellation.Token.ThrowIfCancellationRequested();
            if (isDestroyed)
            {
                return;
            }

            status = "Uploading generated island...";
            ClearGeneratedContent();
            islandHandle = prepared.TakeHandle();

            CreateSurfaceTextures(prepared.surfaceMaps);
            CreateSeaMaskTexture(prepared.seaMask);
            await Task.Yield();
            cancellation.Token.ThrowIfCancellationRequested();

            var terrainRoot = new GameObject("Terrain Tiles");
            terrainRoot.transform.SetParent(transform, false);
            terrainStreamer = terrainRoot.AddComponent<TerrainTileStreamer>();
            await terrainStreamer.InitializeAsync(
                islandHandle,
                terrainMaterial,
                grassMaterial,
                riverMaterial,
                TerrainScale,
                prepared.overviewTiles,
                prepared.riverTiles,
                prepared.riverEmitters,
                showRivers,
                cancellation.Token);
            terrainStreamer.UseRenderCollider = useRenderCollider;
            terrainStreamer.SetRiverEmitterDebug(showRiverEmitterDebug);
            firstPersonController.SetTerrainStreamer(terrainStreamer);

            seaObject = GameObject.CreatePrimitive(PrimitiveType.Plane);
            seaObject.name = "Sea";
            seaObject.transform.SetParent(transform, false);
            seaObject.transform.position = Vector3.up * SeaHeight;
            seaObject.transform.localScale = Vector3.one * (TerrainScale / 10f);
            seaObject.GetComponent<MeshRenderer>().sharedMaterial = seaMaterial;
            DestroyUnityObject(seaObject.GetComponent<Collider>());
            seaObject.SetActive(showSea);

            generationTimer.Stop();
            status = string.Format(
                CultureInfo.InvariantCulture,
                "Seed {0} | 64 LOD 2 tiles | {1:N0} vertices | {2:N0} triangles | {3:F2}s",
                seed,
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
                " | current LOD 0 render collider (support fallback) | {0:F1} km square",
                TerrainScale / 1000f);
        }
        catch (OperationCanceledException)
        {
            if (!isDestroyed)
            {
                status = "Generation cancelled.";
            }
        }
        catch (Exception exception)
        {
            status = exception.Message;
            Debug.LogException(exception);
            ClearGeneratedContent();
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

    private static PreparedIsland PrepareIsland(
        int islandSeed,
        MotuNative.Options options,
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
            var overviewTiles = TerrainTileStreamer.PrepareOverviewTiles(handle);
            cancellationToken.ThrowIfCancellationRequested();
            var riverTiles = PrepareRiverTiles(handle);
            cancellationToken.ThrowIfCancellationRequested();
            var riverEmitters = PrepareRiverEmitters(handle);
            cancellationToken.ThrowIfCancellationRequested();
            var result = new PreparedIsland(
                handle,
                surfaceMaps,
                seaMask,
                overviewTiles,
                riverTiles,
                riverEmitters);
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

    private static PreparedSurfaceMaps PrepareSurfaceMaps(
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
            return new PreparedSurfaceMaps(dimension, normalBytes, occlusionBytes);
        }
        finally
        {
            MotuNative.ReleaseSurfaceMaps(ref surfaceMaps);
        }
    }

    private static PreparedSeaMask PrepareSeaMask(IntPtr handle, int dimension)
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
            return new PreparedSeaMask(dimension, rg);
        }
        finally
        {
            MotuNative.ReleaseSeaMask(ref seaMask);
        }
    }

    private static PreparedMesh[] PrepareRiverTiles(IntPtr handle)
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

            var result = new PreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = CopyRiverMeshData(nativeMesh);
                }
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private static PreparedRiverEmitter[] PrepareRiverEmitters(IntPtr handle)
    {
        MotuNative.CreateRiverEmitters(
            handle,
            RiverEmitterSharpnessDegrees,
            RiverEmitterSpacingMetres,
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
                return Array.Empty<PreparedRiverEmitter>();
            }
            if (export.data == IntPtr.Zero)
            {
                throw new InvalidOperationException(
                    "The Rust rough-water emitter data is missing.");
            }

            var result = new PreparedRiverEmitter[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.RiverEmitterExport>();
            for (var index = 0; index < export.length; index++)
            {
                var native = Marshal.PtrToStructure<MotuNative.RiverEmitterExport>(
                    IntPtr.Add(export.data, index * exportSize));
                var position = new Vector3(
                    (native.position.x - 0.5f) * TerrainScale,
                    native.position.z * TerrainScale,
                    (native.position.y - 0.5f) * TerrainScale);
                var direction = new Vector3(
                    native.direction.x,
                    native.direction.z,
                    native.direction.y).normalized;
                result[index] = new PreparedRiverEmitter(
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

    private void CreateSurfaceTextures(PreparedSurfaceMaps surfaceMaps)
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

    private void CreateSeaMaskTexture(PreparedSeaMask seaMask)
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

    internal static PreparedMesh CopyTerrainMeshData(MotuNative.ExportMesh source, int lod)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            true,
            true);
    }

    internal static Mesh CopyTerrainMesh(MotuNative.ExportMesh source, int lod)
    {
        return CreateTerrainMesh(CopyTerrainMeshData(source, lod), lod);
    }

    internal static Mesh CreateTerrainMesh(PreparedMesh source, int lod)
    {
        return CreateMesh(source, false);
    }

    internal static PreparedMesh CopyRiverMeshData(MotuNative.ExportMesh source)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
            source.material,
            false,
            false);
    }

    internal static Mesh CreateRiverMesh(PreparedMesh source)
    {
        return CreateMesh(source, false);
    }

    private static PreparedMesh CopyMeshData(
        MotuNative.Vector3Array sourceVertices,
        MotuNative.Vector3Array sourceNormals,
        MotuNative.TriangleArray sourceTriangles,
        MotuNative.Vector2Array sourceUv,
        MotuNative.Vector3Array sourceMaterial,
        bool requireMaterial,
        bool createSurfaceMapCoordinates)
    {
        if (sourceVertices.data == IntPtr.Zero || sourceVertices.length == 0)
        {
            throw new InvalidOperationException("The Rust generator returned an empty mesh.");
        }

        var vertices = CopyVector3Array(sourceVertices, true);
        var normals = CopyVector3Array(sourceNormals, false);
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
            uv = CreateTerrainUv(vertices);
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

        return new PreparedMesh(vertices, normals, triangles, uv, material);
    }

    private static Mesh CreateMesh(PreparedMesh source, bool createTangents)
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

    private static Vector2[] CreateTerrainUv(Vector3[] vertices)
    {
        var uv = new Vector2[vertices.Length];
        for (var index = 0; index < vertices.Length; index++)
        {
            uv[index] = new Vector2(
                vertices[index].x / TerrainScale + 0.5f,
                vertices[index].z / TerrainScale + 0.5f);
        }
        return uv;
    }

    private static Vector3[] CopyVector3Array(MotuNative.Vector3Array source, bool position)
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
                ? new Vector3((x - 0.5f) * TerrainScale, z * TerrainScale, (y - 0.5f) * TerrainScale)
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

    private void ClearGeneratedContent()
    {
        clickCandidate = false;
        firstPersonController?.SetTerrainStreamer(null);
        if (terrainStreamer != null)
        {
            terrainStreamer.Dispose();
            DestroyUnityObject(terrainStreamer.gameObject);
            terrainStreamer = null;
        }
        DestroyUnityObject(seaObject);
        seaObject = null;
        terrainMaterial?.SetTexture("_WorldNormal", null);
        terrainMaterial?.SetTexture("_Occlusion", null);
        seaMaterial?.SetTexture("_SeaMask", null);
        DestroyUnityObject(terrainNormalTexture);
        DestroyUnityObject(terrainOcclusionTexture);
        DestroyUnityObject(seaMaskTexture);
        terrainNormalTexture = null;
        terrainOcclusionTexture = null;
        seaMaskTexture = null;

        if (islandHandle != IntPtr.Zero)
        {
            MotuNative.ReleaseMotu(islandHandle);
            islandHandle = IntPtr.Zero;
        }
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
            name = "Grass soil-richness patch noise",
            filterMode = FilterMode.Trilinear,
            wrapMode = TextureWrapMode.Repeat,
            anisoLevel = 2,
        };
        var pixels = new Color[GrassPatchNoiseDimension * GrassPatchNoiseDimension];
        var latticeScale = GrassPatchNoiseLatticePeriod
            / (float)GrassPatchNoiseDimension;
        for (var y = 0; y < GrassPatchNoiseDimension; y++)
        {
            for (var x = 0; x < GrassPatchNoiseDimension; x++)
            {
                var sampleX = (x + 0.5f) * latticeScale;
                var sampleY = (y + 0.5f) * latticeScale;
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
                    0f,
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

    private static Material CreateMaterial(string shaderName, Color color)
    {
        var shader = Shader.Find(shaderName) ?? Shader.Find("Standard");
        if (shader == null)
        {
            throw new InvalidOperationException($"Could not find shader '{shaderName}'.");
        }

        var material = new Material(shader) { color = color };
        if (material.HasProperty("_BaseColor"))
        {
            material.SetColor("_BaseColor", color);
        }
        if (material.HasProperty("_WorldSize"))
        {
            material.SetFloat("_WorldSize", TerrainScale);
        }

        return material;
    }

    private void OnGUI()
    {
        if (firstPersonController != null && firstPersonController.IsActive)
        {
            GUILayout.BeginArea(new Rect(16f, 16f, 500f, 150f), GUI.skin.box);
            GUILayout.Label("First person: WASD move | Shift run | Space jump | Mouse look");
            GUILayout.Label("M: mesh edges | Tab: release/capture cursor for tuning");
            SetGrassBrightness(OptionSlider(
                "Grass brightness",
                grassBrightness,
                0.25f,
                3f,
                "F2"));
            var firstPersonEmitterDebug = GUILayout.Toggle(
                showRiverEmitterDebug,
                "Show rough-water emitter debug");
            if (firstPersonEmitterDebug != showRiverEmitterDebug)
            {
                showRiverEmitterDebug = firstPersonEmitterDebug;
                terrainStreamer?.SetRiverEmitterDebug(showRiverEmitterDebug);
            }
            GUILayout.Label("Escape: return to island overview");
            GUILayout.EndArea();
            GUI.Label(
                new Rect(Screen.width * 0.5f - 5f, Screen.height * 0.5f - 10f, 20f, 20f),
                "+");
            return;
        }

        GUILayout.BeginArea(new Rect(16f, 16f, 500f, 760f), GUI.skin.box);
        GUILayout.Label("Motu Rust Island Viewer");
        GUILayout.BeginHorizontal();
        GUILayout.Label("Seed", GUILayout.Width(42f));
        seedText = GUILayout.TextField(seedText, GUILayout.Width(110f));
        var guiWasEnabled = GUI.enabled;
        GUI.enabled = !generationInProgress;
        if (GUILayout.Button(
            generationInProgress ? "Generating..." : "Generate",
            GUILayout.Width(90f)))
        {
            Generate();
        }
        GUI.enabled = guiWasEnabled;
        GUILayout.EndHorizontal();

        GUILayout.Space(4f);
        GUILayout.Label("Island generation options");
        maxHeight = OptionSlider("Maximum height", maxHeight, 0.02f, 0.5f, "F3");
        waterRatio = OptionSlider("Water ratio", waterRatio, MinimumWaterRatio, 0.95f, "F2");
        slopeMultiplier = OptionSlider("Slope multiplier", slopeMultiplier, 0.2f, 4f, "F2");
        coastalSlopeMultiplier = OptionSlider(
            "Coastal slope",
            coastalSlopeMultiplier,
            0.1f,
            4f,
            "F2");
        hydraulicErosionStrength = OptionSlider(
            "Hydraulic erosion",
            hydraulicErosionStrength,
            0f,
            8f,
            "F2");
        hydraulicDepositionStrength = OptionSlider(
            "Sediment deposition",
            hydraulicDepositionStrength,
            0f,
            4f,
            "F2");
        hydraulicDepositionSlopeDegrees = OptionSlider(
            "Deposition slope (deg)",
            hydraulicDepositionSlopeDegrees,
            1f,
            45f,
            "F1");
        GUILayout.Space(4f);
        GUILayout.Label("River source selection (higher = fewer rivers)");
        riverSourceCatchmentFraction = OptionSlider(
            "Source catchment",
            riverSourceCatchmentFraction,
            MinimumRiverSourceCatchmentFraction,
            MaximumRiverSourceCatchmentFraction,
            "P2");
        riverSourceSteepMultiplier = OptionSlider(
            "Steep slope multiplier",
            riverSourceSteepMultiplier,
            1f,
            8f,
            "F2");

        GUILayout.BeginHorizontal();
        GUILayout.Space(142f);
        if (GUILayout.Button("Reset defaults", GUILayout.Width(110f)))
        {
            ResetOptions();
        }
        GUILayout.Label("Generate to apply", GUILayout.Width(120f));
        GUILayout.EndHorizontal();

        GUILayout.Space(4f);
        showMeshEdges = GUILayout.Toggle(showMeshEdges, "Show mesh edges (wireframe)");
        useRenderCollider = GUILayout.Toggle(
            useRenderCollider,
            "Use true 3D collider (support fallback)");
        if (terrainStreamer != null)
        {
            terrainStreamer.UseRenderCollider = useRenderCollider;
        }
        SetGrassBrightness(OptionSlider(
            "Grass brightness",
            grassBrightness,
            0.25f,
            3f,
            "F2"));
        GUILayout.Label("Grass brightness applies immediately; no regeneration required.");
        var nextRivers = GUILayout.Toggle(showRivers, "Show carved river surfaces");
        var nextEmitterDebug = GUILayout.Toggle(
            showRiverEmitterDebug,
            "Show rough-water emitter debug");
        var nextSea = GUILayout.Toggle(showSea, "Show sea surface");
        if (nextRivers != showRivers)
        {
            showRivers = nextRivers;
            terrainStreamer?.SetRiversVisible(showRivers);
        }
        if (nextEmitterDebug != showRiverEmitterDebug)
        {
            showRiverEmitterDebug = nextEmitterDebug;
            terrainStreamer?.SetRiverEmitterDebug(showRiverEmitterDebug);
        }
        if (nextSea != showSea)
        {
            showSea = nextSea;
            if (seaObject != null) seaObject.SetActive(showSea);
        }

        var displayedStatus = status;
        if (generationInProgress && generationTimer != null)
        {
            displayedStatus += string.Format(
                CultureInfo.InvariantCulture,
                " {0:F1}s",
                generationTimer.Elapsed.TotalSeconds);
        }
        GUILayout.Label(displayedStatus);
        GUILayout.Label("Click terrain: stream detail + walk   |   Drag: orbit   |   Wheel: zoom");
        GUILayout.EndArea();
    }

    private static float OptionSlider(
        string label,
        float value,
        float minimum,
        float maximum,
        string format)
    {
        GUILayout.BeginHorizontal();
        GUILayout.Label(label, GUILayout.Width(138f));
        value = GUILayout.HorizontalSlider(value, minimum, maximum, GUILayout.Width(255f));
        GUILayout.Label(
            value.ToString(format, CultureInfo.InvariantCulture),
            GUILayout.Width(66f));
        GUILayout.EndHorizontal();
        return value;
    }

    private void ResetOptions()
    {
        maxHeight = 0.2f;
        waterRatio = DefaultWaterRatio;
        slopeMultiplier = 1.3f;
        coastalSlopeMultiplier = 1f;
        hydraulicErosionStrength = 1f;
        hydraulicDepositionStrength = 1.5f;
        hydraulicDepositionSlopeDegrees = 12f;
        riverSourceCatchmentFraction = 0.002f;
        riverSourceSteepMultiplier = 4f;
        SetGrassBrightness(1.35f);
    }

    private void SetGrassBrightness(float value)
    {
        if (Mathf.Approximately(grassBrightness, value))
        {
            return;
        }
        grassBrightness = value;
        grassMaterial?.SetFloat("_GrassBrightness", value);
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
            riverSourceCatchmentFraction = 0.002f,
            riverSourceSteepMultiplier = 4f,
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
                    || !terrainMaterial.HasProperty("_GrassThickDepositColor")
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

            var waterShader = Shader.Find("Motu/Water");
            if (waterShader == null
                || !waterShader.isSupported
                || UnityEditor.ShaderUtil.ShaderHasError(waterShader))
            {
                throw new InvalidOperationException(
                    "The animated river-water shader is missing or unsupported.");
            }
            var waterMaterial = new Material(waterShader);
            try
            {
                if (!waterMaterial.HasProperty("_NoiseTex")
                    || !waterMaterial.HasProperty("_CoarseNoiseWorldSize")
                    || !waterMaterial.HasProperty("_FineNoiseWorldSize")
                    || !waterMaterial.HasProperty("_CoarseFlowSpeed")
                    || !waterMaterial.HasProperty("_FineFlowSpeed")
                    || !waterMaterial.HasProperty("_WorldSize")
                    || !waterMaterial.HasProperty("_SeaMask")
                    || !waterMaterial.HasProperty("_ShallowOpacity")
                    || !waterMaterial.HasProperty("_OpacityDepth")
                    || !waterMaterial.HasProperty("_EstuaryStrength")
                    || !waterMaterial.HasProperty("_EstuaryColor")
                    || !waterMaterial.HasProperty("_EstuaryBlendHeight")
                    || !waterMaterial.HasProperty("_SeaLevel")
                    || !waterMaterial.HasProperty("_ReflectionColor")
                    || !waterMaterial.HasProperty("_ReflectionHorizonColor")
                    || !waterMaterial.HasProperty("_ReflectionStrength")
                    || !waterMaterial.HasProperty("_ReflectionFresnelPower")
                    || !waterMaterial.HasProperty("_SunGlintStrength")
                    || !waterMaterial.HasProperty("_SunGlintSharpness")
                    || !waterMaterial.HasProperty("_WhitewaterStrength")
                    || !waterMaterial.HasProperty("_WhitewaterSlopeStart")
                    || !waterMaterial.HasProperty("_WhitewaterSlopeFull"))
                {
                    throw new InvalidOperationException(
                        "The river-water shader is missing flow or depth properties.");
                }
                var riverNoise = CreateRiverNoiseTexture();
                try
                {
                    waterMaterial.SetTexture("_NoiseTex", riverNoise);
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
                DestroyImmediate(waterMaterial);
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
                    || !grassMaterial.HasProperty("_RockPatchNoiseDetailScale")
                    || !grassMaterial.HasProperty("_GrassPatchNoise")
                    || !grassMaterial.HasProperty("_GrassPatchNoiseWorldSize")
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
                    var renderMesh = CopyTerrainMesh(nativeMesh, 0);
                    Physics.BakeMesh(renderMesh.GetEntityId(), false);
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

            var indexCandidates = new[]
            {
                new PreparedRiverEmitter(
                    new Vector3(-999.9f, 1f, -999.9f),
                    Vector3.up,
                    0.25f),
                new PreparedRiverEmitter(Vector3.zero, Vector3.forward, 0.5f),
                new PreparedRiverEmitter(
                    new Vector3(999.9f, 2f, 999.9f),
                    Vector3.right,
                    1f),
            };
            var emitterIndex = new RiverEmitterIndex(indexCandidates, TerrainScale);
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
                pool.Initialize(indexCandidates, TerrainScale, true);
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

            const float lod0Resolution = 512f;
            var colliderArea = new MotuNative.ExportArea(
                192f / lod0Resolution,
                192f / lod0Resolution,
                193f / lod0Resolution,
                193f / lod0Resolution);
            MotuNative.CreateSupportMesh(handle, ref colliderArea, 0, out var support);
            try
            {
                if (support.handle == IntPtr.Zero
                    || support.triangles.length == 0
                    || support.uv.length != support.vertices.length
                    || support.material.length != support.vertices.length)
                {
                    throw new InvalidOperationException("Native support-collider mesh is invalid.");
                }
                DestroyImmediate(CopyTerrainMesh(support, 0));
            }
            finally
            {
                MotuNative.ReleaseMesh(ref support);
            }
        }
        finally
        {
            MotuNative.ReleaseMotu(handle);
        }
        Debug.Log("Motu native mesh and unified terrain material validation passed.");
    }

    private static bool IsFinite(float value)
    {
        return !float.IsNaN(value) && !float.IsInfinity(value);
    }
#endif
}
