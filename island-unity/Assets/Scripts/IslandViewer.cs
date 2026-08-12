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
    private const float MaximumRiverSourceThreshold = 16f;
    private const int Lod0SurfaceMapDimension = 2048;
    private const int Lod1SurfaceMapDimension = 1024;
    private const int Lod2SurfaceMapDimension = 512;
    private const float ClickDragTolerance = 6f;

    private IntPtr islandHandle;
    private TerrainTileStreamer terrainStreamer;
    private GameObject seaObject;
    private Camera viewerCamera;
    private FirstPersonController firstPersonController;
    private readonly Material[] terrainMaterials = new Material[3];
    private readonly Texture2D[] terrainNormalTextures = new Texture2D[3];
    private readonly Texture2D[] terrainOcclusionTextures = new Texture2D[3];
    private Material riverMaterial;
    private Material seaMaterial;
    private string seedText = "666";
    private int seed = 666;
    private float maxHeight = 0.2f;
    private float waterRatio = MinimumWaterRatio;
    private float slopeMultiplier = 1.3f;
    private float coastalSlopeMultiplier = 1f;
    private float noiseMultiplier = 0.0005f;
    private float coastalErosionStrength = 1f;
    private float beachFormationStrength = 1f;
    private float hydraulicErosionStrength = 1f;
    private float hydraulicDepositionStrength = 1.5f;
    private float hydraulicDepositionSlopeDegrees = 12f;
    private float riverLod2SourceThreshold = 0.35f;
    private float riverLod1SourceThreshold = 0.65f;
    private float riverBroadSourceThreshold = 1f;
    private float riverLandSourceThreshold = 1.3f;
    private float riverFinalSourceThreshold = 1.6f;
    private string status = "Ready";
    private bool showRivers = true;
    private bool showSea = true;
    private bool showMeshEdges;
    private bool useRenderCollider = true;
    private bool clickCandidate;
    private Vector2 clickStart;
    private CancellationTokenSource generationCancellation;
    private Stopwatch generationTimer;
    private bool generationInProgress;
    private bool isDestroyed;

    internal sealed class PreparedMesh
    {
        internal readonly Vector3[] vertices;
        internal readonly Vector3[] normals;
        internal readonly int[] triangles;
        internal readonly Vector2[] uv;

        internal PreparedMesh(
            Vector3[] vertices,
            Vector3[] normals,
            int[] triangles,
            Vector2[] uv)
        {
            this.vertices = vertices;
            this.normals = normals;
            this.triangles = triangles;
            this.uv = uv;
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

    private sealed class PreparedIsland : IDisposable
    {
        internal IntPtr handle;
        internal readonly PreparedSurfaceMaps[] surfaceMaps;
        internal readonly PreparedMesh[] overviewTiles;
        internal readonly PreparedMesh[] riverTiles;

        internal PreparedIsland(
            IntPtr handle,
            PreparedSurfaceMaps[] surfaceMaps,
            PreparedMesh[] overviewTiles,
            PreparedMesh[] riverTiles)
        {
            this.handle = handle;
            this.surfaceMaps = surfaceMaps;
            this.overviewTiles = overviewTiles;
            this.riverTiles = riverTiles;
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
        generationCancellation?.Cancel();
        firstPersonController?.Exit();
        ClearGeneratedContent();
        for (var lod = 0; lod < terrainMaterials.Length; lod++)
        {
            DestroyUnityObject(terrainMaterials[lod]);
        }
        DestroyUnityObject(riverMaterial);
        DestroyUnityObject(seaMaterial);
    }

    private void BuildEnvironment()
    {
        RenderSettings.ambientLight = new Color(0.42f, 0.46f, 0.52f);

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
        viewerCamera.clearFlags = CameraClearFlags.SolidColor;
        viewerCamera.backgroundColor = new Color(0.49f, 0.68f, 0.82f);
        viewerCamera.nearClipPlane = 0.05f;
        viewerCamera.farClipPlane = TerrainScale * 8f;
        var orbitCamera = cameraObject.AddComponent<OrbitCamera>();
        orbitCamera.Configure(
            new Vector3(0f, maxHeight * TerrainScale * 0.3f, 0f),
            TerrainScale * 1.15f);
        firstPersonController = cameraObject.AddComponent<FirstPersonController>();
        firstPersonController.Configure(orbitCamera);

        terrainMaterials[0] = CreateMaterial(
            "Motu/Terrain Occlusion",
            new Color(0.35f, 0.58f, 0.22f));
        for (var lod = 1; lod <= 2; lod++)
        {
            terrainMaterials[lod] = CreateMaterial(
                "Motu/Terrain Detail",
                new Color(0.35f, 0.58f, 0.22f));
        }
        riverMaterial = CreateMaterial("Motu/Water", new Color(0.05f, 0.36f, 0.78f, 0.92f));
        seaMaterial = CreateMaterial("Motu/Water", new Color(0.03f, 0.28f, 0.55f, 0.62f));
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
                noiseMultiplier = noiseMultiplier,
                coastalErosionStrength = coastalErosionStrength,
                beachFormationStrength = beachFormationStrength,
                hydraulicErosionStrength = hydraulicErosionStrength,
                hydraulicDepositionStrength = hydraulicDepositionStrength,
                hydraulicDepositionSlopeDegrees = hydraulicDepositionSlopeDegrees,
                riverLod2SourceThreshold = riverLod2SourceThreshold,
                riverLod1SourceThreshold = riverLod1SourceThreshold,
                riverBroadSourceThreshold = riverBroadSourceThreshold,
                riverLandSourceThreshold = riverLandSourceThreshold,
                riverFinalSourceThreshold = riverFinalSourceThreshold,
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

            for (var lod = 0; lod < prepared.surfaceMaps.Length; lod++)
            {
                CreateSurfaceTextures(lod, prepared.surfaceMaps[lod]);
                await Task.Yield();
                cancellation.Token.ThrowIfCancellationRequested();
            }

            var terrainRoot = new GameObject("Terrain Tiles");
            terrainRoot.transform.SetParent(transform, false);
            terrainStreamer = terrainRoot.AddComponent<TerrainTileStreamer>();
            await terrainStreamer.InitializeAsync(
                islandHandle,
                terrainMaterials,
                riverMaterial,
                TerrainScale,
                prepared.overviewTiles,
                prepared.riverTiles,
                showRivers,
                cancellation.Token);
            terrainStreamer.UseRenderCollider = useRenderCollider;
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
            status += " | maps: 2048 LOD 0, 1024 LOD 1, 512 LOD 2";
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
            var surfaceMaps = new PreparedSurfaceMaps[3];
            surfaceMaps[0] = PrepareSurfaceMaps(handle, 0, Lod0SurfaceMapDimension, false);
            cancellationToken.ThrowIfCancellationRequested();
            surfaceMaps[1] = PrepareSurfaceMaps(handle, 1, Lod1SurfaceMapDimension, true);
            cancellationToken.ThrowIfCancellationRequested();
            surfaceMaps[2] = PrepareSurfaceMaps(handle, 2, Lod2SurfaceMapDimension, true);
            cancellationToken.ThrowIfCancellationRequested();
            var overviewTiles = TerrainTileStreamer.PrepareOverviewTiles(handle);
            cancellationToken.ThrowIfCancellationRequested();
            var riverTiles = PrepareRiverTiles(handle);
            cancellationToken.ThrowIfCancellationRequested();
            var result = new PreparedIsland(handle, surfaceMaps, overviewTiles, riverTiles);
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
        int lod,
        int dimension,
        bool includeDetailNormal)
    {
        MotuNative.CreateSurfaceMaps(handle, lod, dimension, out var surfaceMaps);
        try
        {
            if (surfaceMaps.handle == IntPtr.Zero
                || surfaceMaps.occlusion == IntPtr.Zero
                || (includeDetailNormal && surfaceMaps.normalRgb == IntPtr.Zero)
                || surfaceMaps.width != dimension
                || surfaceMaps.height != dimension)
            {
                throw new InvalidOperationException(
                    "The Rust generator returned invalid terrain surface maps.");
            }

            var pixelCount = checked(dimension * dimension);
            var occlusionBytes = new byte[pixelCount];
            Marshal.Copy(surfaceMaps.occlusion, occlusionBytes, 0, occlusionBytes.Length);
            byte[] normalBytes = null;
            if (includeDetailNormal)
            {
                normalBytes = new byte[checked(pixelCount * 3)];
                Marshal.Copy(surfaceMaps.normalRgb, normalBytes, 0, normalBytes.Length);
            }
            return new PreparedSurfaceMaps(dimension, normalBytes, occlusionBytes);
        }
        finally
        {
            MotuNative.ReleaseSurfaceMaps(ref surfaceMaps);
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

    private void CreateSurfaceTextures(int lod, PreparedSurfaceMaps surfaceMaps)
    {
        terrainOcclusionTextures[lod] = CreateSurfaceTexture(
            $"Motu LOD {lod} Terrain Occlusion",
            surfaceMaps.dimension,
            TextureFormat.R8,
            surfaceMaps.occlusion);
        if (surfaceMaps.normalRgb != null)
        {
            terrainNormalTextures[lod] = CreateSurfaceTexture(
                $"Motu LOD {lod} Detail Normal",
                surfaceMaps.dimension,
                TextureFormat.RGB24,
                surfaceMaps.normalRgb);
            terrainMaterials[lod].SetTexture("_DetailNormal", terrainNormalTextures[lod]);
        }
        else if (!terrainMaterials[lod].HasProperty("_Occlusion"))
        {
            throw new InvalidOperationException(
                "The LOD 0 terrain shader does not expose its baked occlusion texture.");
        }
        terrainMaterials[lod].SetTexture("_Occlusion", terrainOcclusionTextures[lod]);
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
            true);
    }

    internal static Mesh CopyTerrainMesh(MotuNative.ExportMesh source, int lod)
    {
        return CreateTerrainMesh(CopyTerrainMeshData(source, lod), lod);
    }

    internal static Mesh CreateTerrainMesh(PreparedMesh source, int lod)
    {
        return CreateMesh(source, lod != 0);
    }

    internal static PreparedMesh CopyRiverMeshData(MotuNative.ExportMesh source)
    {
        return CopyMeshData(
            source.vertices,
            source.normals,
            source.triangles,
            source.uv,
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

        return new PreparedMesh(vertices, normals, triangles, uv);
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
        for (var lod = 0; lod < terrainMaterials.Length; lod++)
        {
            terrainMaterials[lod]?.SetTexture("_DetailNormal", null);
            terrainMaterials[lod]?.SetTexture("_Occlusion", null);
            DestroyUnityObject(terrainNormalTextures[lod]);
            DestroyUnityObject(terrainOcclusionTextures[lod]);
            terrainNormalTextures[lod] = null;
            terrainOcclusionTextures[lod] = null;
        }

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
            GUILayout.BeginArea(new Rect(16f, 16f, 430f, 82f), GUI.skin.box);
            GUILayout.Label("First person: WASD move | Shift run | Space jump | Mouse look");
            GUILayout.Label("M: toggle mesh edges");
            GUILayout.Label("Escape: return to island overview");
            GUILayout.EndArea();
            GUI.Label(
                new Rect(Screen.width * 0.5f - 5f, Screen.height * 0.5f - 10f, 20f, 20f),
                "+");
            return;
        }

        GUILayout.BeginArea(new Rect(16f, 16f, 500f, 736f), GUI.skin.box);
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
        noiseMultiplier = OptionSlider("Terrain noise", noiseMultiplier, 0f, 0.005f, "F5");
        coastalErosionStrength = OptionSlider(
            "Coastal erosion",
            coastalErosionStrength,
            0f,
            4f,
            "F2");
        beachFormationStrength = OptionSlider(
            "Beach formation",
            beachFormationStrength,
            0f,
            4f,
            "F2");
        GUILayout.Label("Erosion cuts exposed rock; beaches settle sediment in shelter.");
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
        GUILayout.Label("River source thresholds (SD; higher = fewer rivers)");
        riverLod2SourceThreshold = OptionSlider(
            "Coarse (LOD 2)",
            riverLod2SourceThreshold,
            0f,
            MaximumRiverSourceThreshold,
            "F2");
        riverLod1SourceThreshold = OptionSlider(
            "Medium (LOD 1)",
            riverLod1SourceThreshold,
            0f,
            MaximumRiverSourceThreshold,
            "F2");
        riverBroadSourceThreshold = OptionSlider(
            "Broad LOD 0",
            riverBroadSourceThreshold,
            0f,
            MaximumRiverSourceThreshold,
            "F2");
        riverLandSourceThreshold = OptionSlider(
            "Land-refined LOD 0",
            riverLandSourceThreshold,
            0f,
            MaximumRiverSourceThreshold,
            "F2");
        riverFinalSourceThreshold = OptionSlider(
            "Final detail",
            riverFinalSourceThreshold,
            0f,
            MaximumRiverSourceThreshold,
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
        var nextRivers = GUILayout.Toggle(showRivers, "Show carved river surfaces");
        var nextSea = GUILayout.Toggle(showSea, "Show sea surface");
        if (nextRivers != showRivers)
        {
            showRivers = nextRivers;
            terrainStreamer?.SetRiversVisible(showRivers);
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
        waterRatio = MinimumWaterRatio;
        slopeMultiplier = 1.3f;
        coastalSlopeMultiplier = 1f;
        noiseMultiplier = 0.0005f;
        coastalErosionStrength = 1f;
        beachFormationStrength = 1f;
        hydraulicErosionStrength = 1f;
        hydraulicDepositionStrength = 1.5f;
        hydraulicDepositionSlopeDegrees = 12f;
        riverLod2SourceThreshold = 0.35f;
        riverLod1SourceThreshold = 0.65f;
        riverBroadSourceThreshold = 1f;
        riverLandSourceThreshold = 1.3f;
        riverFinalSourceThreshold = 1.6f;
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
            noiseMultiplier = 0.0005f,
            coastalErosionStrength = 1f,
            beachFormationStrength = 1f,
            hydraulicErosionStrength = 1f,
            hydraulicDepositionStrength = 1.5f,
            hydraulicDepositionSlopeDegrees = 12f,
            riverLod2SourceThreshold = 0.35f,
            riverLod1SourceThreshold = 0.65f,
            riverBroadSourceThreshold = 1f,
            riverLandSourceThreshold = 1.3f,
            riverFinalSourceThreshold = 1.6f,
        };
        var handle = MotuNative.CreateMotu(2018, ref options);
        if (handle == IntPtr.Zero)
        {
            throw new InvalidOperationException("Native validation could not generate an island.");
        }

        try
        {
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
                        || nativeMesh.uv.length != nativeMesh.vertices.length)
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
                }
                if (!foundRiverGeometry)
                {
                    throw new InvalidOperationException("Native river grid is unexpectedly empty.");
                }
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref riverGrid);
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
                    || support.uv.length != support.vertices.length)
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
        Debug.Log("Motu native render/support mesh validation passed.");
    }
#endif
}
