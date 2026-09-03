using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using Unity.Profiling;
using UnityEngine.Rendering;

public sealed partial class TerrainTileStreamer : MonoBehaviour
{
    private static readonly ProfilerMarker PlayerPositionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.SetPlayerPosition");
    private static readonly ProfilerMarker ColliderTransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.Colliders");
    private static readonly ProfilerMarker Lod1TransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.LOD1");
    private static readonly ProfilerMarker Lod0TransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.LOD0");
    private static readonly ProfilerMarker ForestLod1TransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.ForestLOD1");
    private static readonly ProfilerMarker ForestLod0TransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.ForestLOD0");
    private static readonly ProfilerMarker ReedTransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.Reeds");
    private static readonly ProfilerMarker FernTransitionMarker =
        new ProfilerMarker("Motu.TerrainStreaming.Ferns");
    private static readonly Vector2Int InvalidCell = new Vector2Int(-1, -1);
    private static readonly int WorldNormalWeightId = Shader.PropertyToID("_WorldNormalWeight");
    private static readonly int GrassEnabledId = Shader.PropertyToID("_GrassEnabled");
    private static readonly int GrassHeightId = Shader.PropertyToID("_GrassHeight");
    private static readonly int GrassPlayerPositionId = Shader.PropertyToID("_GrassPlayerPosition");
    private static readonly int GrassRadiusId = Shader.PropertyToID("_GrassRadius");
    private static readonly int GrassFadeWidthId = Shader.PropertyToID("_GrassFadeWidth");
    private const float GrassPositionUpdateDistance = 0.25f;
    private const int Divisions = 8;
    private const int Lod0Divisions = 1;
    internal const byte ClampTop = 1;
    internal const byte ClampLeft = 2;
    internal const byte ClampBottom = 4;
    internal const byte ClampRight = 8;
    private const int NearbyRadius = 1;
    private const int Lod2Resolution = Divisions;
    internal const int Lod1Resolution = Divisions * Divisions;
    internal const int ColliderSamplesPerTile = 129;

    private sealed class Tile
    {
        internal readonly GameObject gameObject;
        internal readonly Mesh mesh;
        internal Mesh edgeMesh;
        internal int[] batchIndices;
        internal GameObject grassObject;

        internal Tile(GameObject gameObject, Mesh mesh, Mesh edgeMesh = null)
        {
            this.gameObject = gameObject;
            this.mesh = mesh;
            this.edgeMesh = edgeMesh;
        }
    }

    private sealed class TileGroup
    {
        internal readonly GameObject root;
        internal readonly Tile[] tiles;
        internal readonly byte clampSides;
        // Coarse tiles retain independent active flags for LOD transitions and
        // debug edges, but share one vertex buffer and renderer. Refinement
        // only rebuilds this index list; it never recombines vertex data.
        internal readonly List<int> activeBatchIndices = new List<int>();
        internal GameObject batchObject;
        internal Mesh batchMesh;
        internal bool batchDirty;

        internal TileGroup(GameObject root, Tile[] tiles, byte clampSides)
        {
            this.root = root;
            this.tiles = tiles;
            this.clampSides = clampSides;
        }
    }

    private sealed class ColliderTile
    {
        internal readonly GameObject gameObject;
        internal readonly TerrainData terrainData;
        internal readonly TerrainCollider collider;

        internal ColliderTile(
            GameObject gameObject,
            TerrainData terrainData,
            TerrainCollider collider)
        {
            this.gameObject = gameObject;
            this.terrainData = terrainData;
            this.collider = collider;
        }
    }

    private readonly Dictionary<Vector2Int, TileGroup> lod1Groups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly Dictionary<Vector2Int, TileGroup> lod0Groups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly Dictionary<Vector2Int, TileGroup> riverGroups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly Dictionary<Vector2Int, TileGroup> riverRockGroups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly Dictionary<Vector2Int, ColliderTile> colliderTiles =
        new Dictionary<Vector2Int, ColliderTile>();
    private readonly List<Vector2Int> removalScratch = new List<Vector2Int>(9);
    private readonly List<TileGroup> pendingTransitionGroups = new List<TileGroup>(9);

    private IntPtr islandHandle;
    private Material terrainMaterial;
    private Material terrainLod1Material;
    private Material terrainLod2Material;
    private Material grassMaterial;
    private Material treeWoodMaterial;
    private Material treeLod1WoodMaterial;
    private Material treeFoliageMaterial;
    private Material treeLod0FoliageMaterial;
    private Material reedMaterial;
    private Material fernMaterial;
    private MaterialPropertyBlock lod0MaterialProperties;
    private Material riverMaterial;
    private Material rockMaterial;
    private Material meshEdgeMaterial;
    private IslandPreparedMesh[] preparedRiverTiles;
    private IslandPreparedMesh[] preparedRiverRockTiles;
    private IslandPreparedColliderHeightMap colliderHeightMap;
    private float worldSize;
    private float grassBoundsRadius;
    private Vector3 lastGrassPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
    private bool grassTilesDirty;
    private bool grassVisible;
    private bool meshEdgesVisible;
    private TileGroup lod2Group;
    private GameObject riverRoot;
    private GameObject riverRockRoot;
    private GameObject colliderRoot;
    private WaterfallMistPool waterfallMistPool;
    private ForestTileStreamer forestStreamer;
    private ReedTileStreamer reedStreamer;
    private FernTileStreamer fernStreamer;
    private Vector2Int currentLod2 = InvalidCell;
    private Vector2Int currentLod1 = InvalidCell;
    private Vector2Int requestedLod2 = InvalidCell;
    private Vector2Int requestedLod1 = InvalidCell;
    private Coroutine transitionCoroutine;

    public int BaseVertexCount { get; private set; }
    public int BaseTriangleCount { get; private set; }
    public int WaterfallFootCount => waterfallMistPool?.FootCount ?? 0;
    internal int Lod1GroupCount => lod1Groups.Count;
    internal int Lod0GroupCount => lod0Groups.Count;
    internal int TerrainColliderCount => colliderTiles.Count;
    internal bool HasCurrentCollider => colliderTiles.Count != 0;
    internal int ForestLod2TileCount => forestStreamer?.ActiveLod2TileCount ?? 0;
    internal int ForestLod1GroupCount => forestStreamer?.Lod1GroupCount ?? 0;
    internal int ForestLod0GroupCount => forestStreamer?.Lod0GroupCount ?? 0;
    internal byte Lod1ClampSidesAt(Vector2Int key) => lod1Groups[key].clampSides;
    internal byte Lod0ClampSidesAt(Vector2Int key) => lod0Groups[key].clampSides;

    internal async Task InitializeAsync(
        IntPtr handle,
        Material sharedTerrainMaterial,
        Material sharedTerrainLod1Material,
        Material sharedTerrainLod2Material,
        Material sharedGrassMaterial,
        Material sharedRockMaterial,
        Material sharedTreeWoodMaterial,
        Material sharedTreeLod1WoodMaterial,
        Material sharedTreeFoliageMaterial,
        Material sharedTreeLod0FoliageMaterial,
        Material sharedReedMaterial,
        Material sharedFernMaterial,
        Material waterMaterial,
        Material sharedMeshEdgeMaterial,
        float terrainWorldSize,
        IslandPreparedMesh[] overviewTiles,
        IslandPreparedMesh[] riverTiles,
        IslandPreparedMesh[] riverRockTiles,
        IslandPreparedForestData preparedForest,
        IslandPreparedMesh[] preparedReedTiles,
        IslandPreparedMesh[] preparedFernTiles,
        IslandPreparedWaterfallFoot[] waterfallFeet,
        IslandPreparedColliderHeightMap preparedColliderHeightMap,
        bool showRivers,
        bool showGrass,
        bool showRocks,
        bool showForests,
        bool showReeds,
        bool showFerns,
        CancellationToken cancellationToken,
        UnityFrameBudget installationBudget)
    {
        islandHandle = handle;
        terrainMaterial = sharedTerrainMaterial;
        terrainLod1Material = sharedTerrainLod1Material;
        terrainLod2Material = sharedTerrainLod2Material;
        grassMaterial = sharedGrassMaterial;
        treeWoodMaterial = sharedTreeWoodMaterial;
        treeLod1WoodMaterial = sharedTreeLod1WoodMaterial;
        treeFoliageMaterial = sharedTreeFoliageMaterial;
        treeLod0FoliageMaterial = sharedTreeLod0FoliageMaterial;
        reedMaterial = sharedReedMaterial;
        fernMaterial = sharedFernMaterial;
        terrainMaterial.SetFloat(GrassEnabledId, 0f);
        grassMaterial.SetFloat(GrassEnabledId, 0f);
        if (treeFoliageMaterial != null)
        {
            treeFoliageMaterial.SetFloat(
                GrassRadiusId,
                grassMaterial.GetFloat(GrassRadiusId));
            treeFoliageMaterial.SetFloat(
                GrassFadeWidthId,
                grassMaterial.GetFloat(GrassFadeWidthId));
        }
        if (treeLod0FoliageMaterial != null)
        {
            treeLod0FoliageMaterial.SetFloat(
                GrassRadiusId,
                grassMaterial.GetFloat(GrassRadiusId));
            treeLod0FoliageMaterial.SetFloat(
                GrassFadeWidthId,
                grassMaterial.GetFloat(GrassFadeWidthId));
        }
        grassBoundsRadius = grassMaterial.GetFloat(GrassRadiusId)
            + grassMaterial.GetFloat(GrassHeightId);
        lod0MaterialProperties = new MaterialPropertyBlock();
        lod0MaterialProperties.SetFloat(WorldNormalWeightId, 0f);
        riverMaterial = waterMaterial;
        rockMaterial = sharedRockMaterial;
        meshEdgeMaterial = sharedMeshEdgeMaterial;
        preparedRiverTiles = riverTiles;
        preparedRiverRockTiles = riverRockTiles;
        if (preparedForest == null)
        {
            throw new InvalidOperationException("The prepared forest batch is missing.");
        }
        colliderHeightMap = preparedColliderHeightMap;
        worldSize = terrainWorldSize;
        grassVisible = showGrass;
        if (preparedRiverTiles == null
            || preparedRiverTiles.Length != Lod1Resolution * Lod1Resolution)
        {
            throw new InvalidOperationException("The prepared river tile batch is invalid.");
        }
        if (preparedRiverRockTiles == null
            || preparedRiverRockTiles.Length != Lod1Resolution * Lod1Resolution)
        {
            throw new InvalidOperationException("The prepared river-rock tile batch is invalid.");
        }
        if (colliderHeightMap == null
            || colliderHeightMap.samplesPerTile != ColliderSamplesPerTile)
        {
            throw new InvalidOperationException(
                "The prepared terrain-collider height map is invalid.");
        }
        colliderRoot = new GameObject("Terrain Colliders (Collision Only)");
        colliderRoot.transform.SetParent(transform, false);
        riverRoot = new GameObject("Rivers");
        var waterLayer = LayerMask.NameToLayer("Water");
        if (waterLayer >= 0)
        {
            riverRoot.layer = waterLayer;
        }
        riverRoot.transform.SetParent(transform, false);
        riverRoot.transform.localPosition = Vector3.up * 0.025f;
        var mistRoot = new GameObject("Waterfall Foot Fog Pool");
        mistRoot.layer = riverRoot.layer;
        mistRoot.transform.SetParent(riverRoot.transform, false);
        waterfallMistPool = mistRoot.AddComponent<WaterfallMistPool>();
        waterfallMistPool.Initialize(waterfallFeet, worldSize, showRivers);
        riverRoot.SetActive(showRivers);
        riverRockRoot = new GameObject("Stones and Boulders");
        riverRockRoot.transform.SetParent(transform, false);
        riverRockRoot.SetActive(showRocks);
        forestStreamer = new ForestTileStreamer();
        forestStreamer.Initialize(
            transform,
            treeFoliageMaterial,
            treeLod0FoliageMaterial,
            treeWoodMaterial,
            treeLod1WoodMaterial,
            meshEdgeMaterial,
            preparedForest,
            showForests);
        reedStreamer = new ReedTileStreamer();
        reedStreamer.Initialize(
            transform,
            sharedReedMaterial,
            preparedReedTiles,
            showReeds);
        fernStreamer = new FernTileStreamer();
        fernStreamer.Initialize(
            transform,
            sharedFernMaterial,
            preparedFernTiles,
            showFerns);
        lod2Group = await CreatePreparedGroupAsync(
            2,
            Vector2Int.zero,
            overviewTiles,
            cancellationToken,
            installationBudget);
        for (var index = 0; index < lod2Group.tiles.Length; index++)
        {
            var tile = lod2Group.tiles[index];
            if (tile == null)
            {
                continue;
            }
            BaseVertexCount += tile.mesh.vertexCount;
            BaseTriangleCount += (int)tile.mesh.GetIndexCount(0) / 3;
        }
    }

    internal static IslandPreparedMesh[] PrepareOverviewTiles(
        IntPtr handle,
        float terrainWorldSize)
    {
        var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
        MotuNative.CreateMeshGrid(handle, ref area, 2, Divisions, 0, out var export);
        try
        {
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != Divisions * Divisions)
            {
                throw new InvalidOperationException(
                    "The Rust grid slicer returned an invalid overview tile batch.");
            }

            var result = new IslandPreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = IslandGenerator.CopyTerrainMeshData(
                        nativeMesh,
                        2,
                        terrainWorldSize);
                }
            }
            return result;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    public void SetPlayerPosition(Vector3 worldPosition)
    {
        using (PlayerPositionMarker.Auto())
        {
            var localPosition = transform.InverseTransformPoint(worldPosition);
            terrainMaterial.SetVector(GrassPlayerPositionId, worldPosition);
            terrainMaterial.SetFloat(GrassEnabledId, grassVisible ? 1f : 0f);
            grassMaterial.SetVector(GrassPlayerPositionId, worldPosition);
            grassMaterial.SetFloat(GrassEnabledId, grassVisible ? 1f : 0f);
            treeFoliageMaterial?.SetVector(GrassPlayerPositionId, worldPosition);
            treeLod0FoliageMaterial?.SetVector(GrassPlayerPositionId, worldPosition);
            reedMaterial?.SetVector(GrassPlayerPositionId, worldPosition);
            fernMaterial?.SetVector(GrassPlayerPositionId, worldPosition);
            requestedLod2 = LocalCell(localPosition, Lod2Resolution);
            requestedLod1 = LocalCell(localPosition, Lod1Resolution);

            if (currentLod1 == InvalidCell || currentLod2 == InvalidCell)
            {
                ApplyInitialPlayerFocus(requestedLod2, requestedLod1);
            }
            else if (requestedLod1 != currentLod1
                || requestedLod2 != currentLod2)
            {
                EnsureCriticalCollider(requestedLod1);
                if (transitionCoroutine == null)
                {
                    transitionCoroutine = StartCoroutine(ApplyRequestedNeighborhoods());
                }
            }
            if (grassVisible)
            {
                UpdateGrassTiles(localPosition);
            }
            waterfallMistPool?.SetPlayerPosition(localPosition, requestedLod1);
        }
    }

    private void ApplyInitialPlayerFocus(Vector2Int lod2, Vector2Int lod1)
    {
        using (ColliderTransitionMarker.Auto())
        {
            UpdateColliderNeighborhood(lod1);
        }
        using (ForestLod1TransitionMarker.Auto())
        {
            forestStreamer?.UpdateLod1Neighborhood(lod2);
        }
        using (Lod1TransitionMarker.Auto())
        {
            UpdateLod1Neighborhood(lod2);
        }
        using (ForestLod0TransitionMarker.Auto())
        {
            forestStreamer?.UpdateLod0Neighborhood(lod1);
        }
        using (ReedTransitionMarker.Auto())
        {
            reedStreamer?.UpdateLod0Neighborhood(lod1);
        }
        using (FernTransitionMarker.Auto())
        {
            fernStreamer?.UpdateLod0Neighborhood(lod1);
        }
        using (Lod0TransitionMarker.Auto())
        {
            UpdateLod0Neighborhood(lod1);
        }
        currentLod2 = lod2;
        currentLod1 = lod1;
    }

    private IEnumerator ApplyRequestedNeighborhoods()
    {
        // Start after the movement frame which detected the boundary. The old
        // complete neighbourhood remains active while replacements are built.
        yield return null;
        try
        {
            while (requestedLod1 != currentLod1 || requestedLod2 != currentLod2)
            {
                var targetLod2 = requestedLod2;
                var targetLod1 = requestedLod1;

                if (targetLod1 != currentLod1)
                {
                    yield return UpdateColliderNeighborhoodIncremental(targetLod1);
                    if (targetLod1 != requestedLod1 || targetLod2 != requestedLod2)
                    {
                        continue;
                    }
                }

                if (targetLod2 != currentLod2)
                {
                    yield return forestStreamer?.UpdateLod1NeighborhoodIncremental(
                        targetLod2,
                        () => targetLod2 == requestedLod2);
                    if (targetLod2 != requestedLod2)
                    {
                        continue;
                    }

                    yield return UpdateLod1NeighborhoodIncremental(targetLod2);
                    if (targetLod2 != requestedLod2)
                    {
                        continue;
                    }
                    currentLod2 = targetLod2;
                    yield return null;
                }

                if (targetLod1 != currentLod1)
                {
                    yield return forestStreamer?.UpdateLod0NeighborhoodIncremental(
                        targetLod1,
                        () => targetLod1 == requestedLod1);
                    if (targetLod1 != requestedLod1)
                    {
                        continue;
                    }

                    yield return reedStreamer?.UpdateLod0NeighborhoodIncremental(
                        targetLod1,
                        () => targetLod1 == requestedLod1);
                    if (targetLod1 != requestedLod1)
                    {
                        continue;
                    }

                    yield return fernStreamer?.UpdateLod0NeighborhoodIncremental(
                        targetLod1,
                        () => targetLod1 == requestedLod1);
                    if (targetLod1 != requestedLod1)
                    {
                        continue;
                    }

                    yield return UpdateLod0NeighborhoodIncremental(targetLod1);
                    if (targetLod1 != requestedLod1)
                    {
                        continue;
                    }
                    currentLod1 = targetLod1;
                    grassTilesDirty = true;
                }
            }
        }
        finally
        {
            transitionCoroutine = null;
        }
    }

    public void ClearPlayerFocus()
    {
        CancelPendingTransition();
        forestStreamer?.ClearPlayerFocus();
        reedStreamer?.ClearPlayerFocus();
        fernStreamer?.ClearPlayerFocus();
        terrainMaterial?.SetFloat(GrassEnabledId, 0f);
        grassMaterial?.SetFloat(GrassEnabledId, 0f);
        lastGrassPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
        waterfallMistPool?.ClearPlayerFocus();
        RemoveAllColliderTiles();
        currentLod2 = InvalidCell;
        currentLod1 = InvalidCell;
        requestedLod2 = InvalidCell;
        requestedLod1 = InvalidCell;
        foreach (var group in riverGroups.Values)
        {
            group.root?.SetActive(false);
        }
        foreach (var group in riverRockGroups.Values)
        {
            group.root?.SetActive(false);
        }
        foreach (var entry in lod0Groups)
        {
            SetLod1TileActive(entry.Key, true);
            DestroyGroup(entry.Value);
        }
        lod0Groups.Clear();
        RebuildDirtyLod1Batches();
        foreach (var entry in lod1Groups)
        {
            SetLod2TileActive(entry.Key, true);
            DestroyGroup(entry.Value);
        }
        lod1Groups.Clear();
        RebuildTerrainBatchIfDirty(lod2Group);
        currentLod2 = new Vector2Int(-1, -1);
        currentLod1 = new Vector2Int(-1, -1);
    }

    public bool TryRaycastOverview(Ray ray, out Vector3 point)
    {
        var nearest = float.PositiveInfinity;
        point = default;
        if (lod2Group == null)
        {
            return false;
        }

        var localRay = new Ray(
            transform.InverseTransformPoint(ray.origin),
            transform.InverseTransformDirection(ray.direction));
        foreach (var tile in lod2Group.tiles)
        {
            if (tile == null || !tile.mesh.bounds.IntersectRay(localRay))
            {
                continue;
            }
            var vertices = tile.mesh.vertices;
            var triangles = tile.mesh.triangles;
            for (var index = 0; index + 2 < triangles.Length; index += 3)
            {
                if (RayTriangle(
                    localRay,
                    vertices[triangles[index]],
                    vertices[triangles[index + 1]],
                    vertices[triangles[index + 2]],
                    out var distance)
                    && distance < nearest)
                {
                    nearest = distance;
                    point = transform.TransformPoint(localRay.GetPoint(distance));
                }
            }
        }
        return nearest < float.PositiveInfinity;
    }

    public bool TrySnapToCurrentCollider(Vector3 approximatePoint, out Vector3 point)
    {
        point = approximatePoint;
        if (colliderTiles.Count == 0)
        {
            return false;
        }
        var localPoint = transform.InverseTransformPoint(approximatePoint);
        var origin = transform.TransformPoint(
            new Vector3(localPoint.x, worldSize, localPoint.z));
        var ray = new Ray(origin, transform.TransformDirection(Vector3.down));
        var center = LocalCell(localPoint, Lod1Resolution);
        var found = false;
        var highest = float.NegativeInfinity;
        var minimumX = Mathf.Max(center.x - 1, 0);
        var maximumX = Mathf.Min(center.x + 1, Lod1Resolution - 1);
        var minimumY = Mathf.Max(center.y - 1, 0);
        var maximumY = Mathf.Min(center.y + 1, Lod1Resolution - 1);
        for (var y = minimumY; y <= maximumY; y++)
        {
            for (var x = minimumX; x <= maximumX; x++)
            {
                if (!colliderTiles.TryGetValue(new Vector2Int(x, y), out var tile)
                    || !tile.collider.enabled
                    || !tile.collider.Raycast(ray, out var hit, worldSize * 2f)
                    || hit.point.y <= highest)
                {
                    continue;
                }
                found = true;
                highest = hit.point.y;
                point = hit.point;
            }
        }
        return found;
    }

    public void Dispose()
    {
        ClearPlayerFocus();
        foreach (var group in riverGroups.Values)
        {
            DestroyGroup(group);
        }
        riverGroups.Clear();
        foreach (var group in riverRockGroups.Values)
        {
            DestroyGroup(group);
        }
        riverRockGroups.Clear();
        waterfallMistPool?.DisposePool();
        waterfallMistPool = null;
        forestStreamer?.Dispose();
        forestStreamer = null;
        reedStreamer?.Dispose();
        reedStreamer = null;
        fernStreamer?.Dispose();
        fernStreamer = null;
        DestroyUnityObject(riverRoot);
        riverRoot = null;
        DestroyUnityObject(riverRockRoot);
        riverRockRoot = null;
        DestroyUnityObject(colliderRoot);
        colliderRoot = null;
        preparedRiverTiles = null;
        preparedRiverRockTiles = null;
        colliderHeightMap = null;
        treeWoodMaterial = null;
        treeLod1WoodMaterial = null;
        treeFoliageMaterial = null;
        treeLod0FoliageMaterial = null;
        reedMaterial = null;
        fernMaterial = null;
        terrainLod1Material = null;
        terrainLod2Material = null;
        if (lod2Group != null)
        {
            DestroyGroup(lod2Group);
            lod2Group = null;
        }
        islandHandle = IntPtr.Zero;
    }

    private void CancelPendingTransition()
    {
        if (transitionCoroutine != null)
        {
            StopCoroutine(transitionCoroutine);
            transitionCoroutine = null;
        }
        foreach (var group in pendingTransitionGroups)
        {
            DestroyGroup(group);
        }
        pendingTransitionGroups.Clear();
    }

    public void SetRiversVisible(bool visible)
    {
        waterfallMistPool?.SetRiversVisible(visible);
        riverRoot?.SetActive(visible);
    }

    public void SetGrassVisible(bool visible)
    {
        if (grassVisible == visible)
        {
            return;
        }
        grassVisible = visible;
        terrainMaterial?.SetFloat(GrassEnabledId, visible ? 1f : 0f);
        grassMaterial?.SetFloat(GrassEnabledId, visible ? 1f : 0f);
        if (!visible)
        {
            SetAllGrassInactive(lod2Group);
            foreach (var group in lod1Groups.Values) SetAllGrassInactive(group);
            foreach (var group in lod0Groups.Values) SetAllGrassInactive(group);
            return;
        }
        grassTilesDirty = true;
    }

    public void SetWaterfallFootDebug(bool visible)
    {
        waterfallMistPool?.SetDebugDraw(visible);
    }

    public void SetRocksVisible(bool visible)
    {
        riverRockRoot?.SetActive(visible);
    }

    public void SetForestsVisible(bool visible)
    {
        forestStreamer?.SetVisible(visible);
    }

    public void SetReedsVisible(bool visible)
    {
        reedStreamer?.SetVisible(visible);
    }

    public void SetFernsVisible(bool visible)
    {
        fernStreamer?.SetVisible(visible);
    }

    public void SetMeshEdgesVisible(bool visible)
    {
        meshEdgesVisible = visible;
    }

    public void SetTreeMeshEdgesVisible(bool visible)
    {
        forestStreamer?.SetMeshEdgesVisible(visible);
    }

    private void LateUpdate()
    {
        if (!meshEdgesVisible || meshEdgeMaterial == null)
        {
            return;
        }
        DrawGroupEdges(lod2Group);
        foreach (var group in lod1Groups.Values) DrawGroupEdges(group);
        foreach (var group in lod0Groups.Values) DrawGroupEdges(group);
    }

    private void DrawGroupEdges(TileGroup group)
    {
        if (group?.root == null || !group.root.activeInHierarchy)
        {
            return;
        }
        foreach (var tile in group.tiles)
        {
            if (tile == null || !tile.gameObject.activeInHierarchy)
            {
                continue;
            }
            // Terrain wireframes are a debug-only feature. Building them for
            // every streamed tile made ordinary LOD transitions deduplicate
            // every triangle edge and upload a second mesh unnecessarily.
            tile.edgeMesh ??= CreateEdgeMesh(tile.mesh);
            Graphics.DrawMesh(
                tile.edgeMesh,
                tile.gameObject.transform.localToWorldMatrix,
                meshEdgeMaterial,
                tile.gameObject.layer,
                null,
                0,
                null,
                ShadowCastingMode.Off,
                false,
                null,
                LightProbeUsage.Off);
        }
    }

    private void SetLod2TileActive(Vector2Int key, bool active)
    {
        SetBatchedTileActive(
            lod2Group,
            key.y * Divisions + key.x,
            active);
    }

    private void SetLod1TileActive(Vector2Int key, bool active)
    {
        var parent = new Vector2Int(key.x / Divisions, key.y / Divisions);
        if (!lod1Groups.TryGetValue(parent, out var group))
        {
            return;
        }
        SetBatchedTileActive(
            group,
            (key.y % Divisions) * Divisions + key.x % Divisions,
            active);
    }

    private Vector2Int LocalCell(Vector3 position, int resolution)
    {
        var normalizedX = Mathf.Clamp01(position.x / worldSize + 0.5f);
        var normalizedY = Mathf.Clamp01(position.z / worldSize + 0.5f);
        return new Vector2Int(
            Mathf.Min(Mathf.FloorToInt(normalizedX * resolution), resolution - 1),
            Mathf.Min(Mathf.FloorToInt(normalizedY * resolution), resolution - 1));
    }


    private static bool RayTriangle(
        Ray ray,
        Vector3 a,
        Vector3 b,
        Vector3 c,
        out float distance)
    {
        const float epsilon = 0.000001f;
        var edge1 = b - a;
        var edge2 = c - a;
        var p = Vector3.Cross(ray.direction, edge2);
        var determinant = Vector3.Dot(edge1, p);
        if (Mathf.Abs(determinant) < epsilon)
        {
            distance = 0f;
            return false;
        }
        var inverse = 1f / determinant;
        var t = ray.origin - a;
        var u = Vector3.Dot(t, p) * inverse;
        if (u < 0f || u > 1f)
        {
            distance = 0f;
            return false;
        }
        var q = Vector3.Cross(t, edge1);
        var v = Vector3.Dot(ray.direction, q) * inverse;
        if (v < 0f || u + v > 1f)
        {
            distance = 0f;
            return false;
        }
        distance = Vector3.Dot(edge2, q) * inverse;
        return distance >= 0f;
    }

    private static void DestroyGroup(TileGroup group)
    {
        if (group == null)
        {
            return;
        }
        foreach (var tile in group.tiles)
        {
            if (tile != null)
            {
                DestroyUnityObject(tile.edgeMesh);
                DestroyUnityObject(tile.mesh);
            }
        }
        DestroyUnityObject(group.batchMesh);
        if (group.root != null)
        {
            DestroyUnityObject(group.root);
        }
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(value);
        }
        else
        {
            DestroyImmediate(value);
        }
    }
}
