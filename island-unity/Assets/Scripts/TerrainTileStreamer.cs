using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Rendering;

public sealed class TerrainTileStreamer : MonoBehaviour
{
    private static readonly int WorldNormalWeightId = Shader.PropertyToID("_WorldNormalWeight");
    private static readonly int GrassEnabledId = Shader.PropertyToID("_GrassEnabled");
    private static readonly int GrassHeightId = Shader.PropertyToID("_GrassHeight");
    private static readonly int GrassPlayerPositionId = Shader.PropertyToID("_GrassPlayerPosition");
    private static readonly int GrassRadiusId = Shader.PropertyToID("_GrassRadius");
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
        internal readonly Mesh edgeMesh;
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

    private IntPtr islandHandle;
    private Material terrainMaterial;
    private Material grassMaterial;
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
    private RiverParticlePool riverParticlePool;
    private Vector2Int currentLod2 = new Vector2Int(-1, -1);
    private Vector2Int currentLod1 = new Vector2Int(-1, -1);

    public int BaseVertexCount { get; private set; }
    public int BaseTriangleCount { get; private set; }
    public int RiverEmitterCandidateCount => riverParticlePool?.CandidateCount ?? 0;
    internal int Lod1GroupCount => lod1Groups.Count;
    internal int Lod0GroupCount => lod0Groups.Count;
    internal int TerrainColliderCount => colliderTiles.Count;
    internal bool HasCurrentCollider => colliderTiles.Count != 0;
    internal byte Lod1ClampSidesAt(Vector2Int key) => lod1Groups[key].clampSides;
    internal byte Lod0ClampSidesAt(Vector2Int key) => lod0Groups[key].clampSides;

    internal async Task InitializeAsync(
        IntPtr handle,
        Material sharedTerrainMaterial,
        Material sharedGrassMaterial,
        Material sharedRockMaterial,
        Material waterMaterial,
        Material sharedMeshEdgeMaterial,
        float terrainWorldSize,
        IslandPreparedMesh[] overviewTiles,
        IslandPreparedMesh[] riverTiles,
        IslandPreparedMesh[] riverRockTiles,
        IslandPreparedRiverEmitter[] riverEmitters,
        IslandPreparedColliderHeightMap preparedColliderHeightMap,
        bool showRivers,
        bool showGrass,
        bool showRocks,
        CancellationToken cancellationToken)
    {
        islandHandle = handle;
        terrainMaterial = sharedTerrainMaterial;
        grassMaterial = sharedGrassMaterial;
        terrainMaterial.SetFloat(GrassEnabledId, 0f);
        grassMaterial.SetFloat(GrassEnabledId, 0f);
        grassBoundsRadius = grassMaterial.GetFloat(GrassRadiusId)
            + grassMaterial.GetFloat(GrassHeightId);
        lod0MaterialProperties = new MaterialPropertyBlock();
        lod0MaterialProperties.SetFloat(WorldNormalWeightId, 0f);
        riverMaterial = waterMaterial;
        rockMaterial = sharedRockMaterial;
        meshEdgeMaterial = sharedMeshEdgeMaterial;
        preparedRiverTiles = riverTiles;
        preparedRiverRockTiles = riverRockTiles;
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
        var particleRoot = new GameObject("Rough Water Particle Pool");
        particleRoot.layer = riverRoot.layer;
        particleRoot.transform.SetParent(riverRoot.transform, false);
        riverParticlePool = particleRoot.AddComponent<RiverParticlePool>();
        riverParticlePool.Initialize(riverEmitters, worldSize, showRivers);
        riverRoot.SetActive(showRivers);
        riverRockRoot = new GameObject("Stones and Boulders");
        riverRockRoot.transform.SetParent(transform, false);
        riverRockRoot.SetActive(showRocks);
        lod2Group = await CreatePreparedGroupAsync(
            2,
            Vector2Int.zero,
            overviewTiles,
            cancellationToken);
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
        var localPosition = transform.InverseTransformPoint(worldPosition);
        terrainMaterial.SetVector(GrassPlayerPositionId, worldPosition);
        terrainMaterial.SetFloat(GrassEnabledId, grassVisible ? 1f : 0f);
        grassMaterial.SetVector(GrassPlayerPositionId, worldPosition);
        grassMaterial.SetFloat(GrassEnabledId, grassVisible ? 1f : 0f);
        var lod2 = LocalCell(localPosition, Lod2Resolution);
        var lod1 = LocalCell(localPosition, Lod1Resolution);

        if (lod1 != currentLod1)
        {
            // Collision must be live before any potentially expensive render
            // refinement caused by the same movement.
            UpdateColliderNeighborhood(lod1);
        }
        if (lod2 != currentLod2)
        {
            UpdateLod1Neighborhood(lod2);
            currentLod2 = lod2;
        }
        if (lod1 != currentLod1)
        {
            UpdateLod0Neighborhood(lod1);
            currentLod1 = lod1;
        }
        if (grassVisible)
        {
            UpdateGrassTiles(localPosition);
        }
        riverParticlePool?.SetPlayerPosition(localPosition, lod2);
    }

    public void ClearPlayerFocus()
    {
        terrainMaterial?.SetFloat(GrassEnabledId, 0f);
        grassMaterial?.SetFloat(GrassEnabledId, 0f);
        lastGrassPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
        riverParticlePool?.ClearPlayerFocus();
        RemoveAllColliderTiles();
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
        foreach (var entry in lod1Groups)
        {
            SetLod2TileActive(entry.Key, true);
            DestroyGroup(entry.Value);
        }
        lod1Groups.Clear();
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
        riverParticlePool?.DisposePool();
        riverParticlePool = null;
        DestroyUnityObject(riverRoot);
        riverRoot = null;
        DestroyUnityObject(riverRockRoot);
        riverRockRoot = null;
        DestroyUnityObject(colliderRoot);
        colliderRoot = null;
        preparedRiverTiles = null;
        preparedRiverRockTiles = null;
        colliderHeightMap = null;
        if (lod2Group != null)
        {
            DestroyGroup(lod2Group);
            lod2Group = null;
        }
        islandHandle = IntPtr.Zero;
    }

    public void SetRiversVisible(bool visible)
    {
        riverParticlePool?.SetRiversVisible(visible);
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

    public void SetRiverEmitterDebug(bool visible)
    {
        riverParticlePool?.SetDebugDraw(visible);
    }

    public void SetRocksVisible(bool visible)
    {
        riverRockRoot?.SetActive(visible);
    }

    public void SetMeshEdgesVisible(bool visible)
    {
        meshEdgesVisible = visible;
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
            if (tile?.edgeMesh == null || !tile.gameObject.activeInHierarchy)
            {
                continue;
            }
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

    private void UpdateLod1Neighborhood(Vector2Int center)
    {
        CollectOutsideNeighborhood(lod1Groups, center, Lod2Resolution);
        foreach (var key in removalScratch)
        {
            DestroyGroup(lod1Groups[key]);
            lod1Groups.Remove(key);
            SetLod2TileActive(key, true);
            SetRiverGroupActive(key, false);
            SetRiverRockGroupActive(key, false);
        }

        ForEachNeighbour(center, Lod2Resolution, key =>
        {
            var clampSides = ClampSidesFor(key, center, Lod2Resolution);
            if (lod1Groups.TryGetValue(key, out var group)
                && group.clampSides != clampSides)
            {
                DestroyGroup(group);
                lod1Groups.Remove(key);
            }
            if (!lod1Groups.ContainsKey(key))
            {
                lod1Groups.Add(key, CreateGroup(1, key, Lod2Resolution, clampSides));
            }
            SetLod2TileActive(key, false);
            SetRiverGroupActive(key, true);
            SetRiverRockGroupActive(key, true);
        });
    }

    private void UpdateLod0Neighborhood(Vector2Int center)
    {
        CollectOutsideNeighborhood(lod0Groups, center, Lod1Resolution);
        foreach (var key in removalScratch)
        {
            DestroyGroup(lod0Groups[key]);
            lod0Groups.Remove(key);
            SetLod1TileActive(key, true);
        }

        ForEachNeighbour(center, Lod1Resolution, key =>
        {
            var clampSides = ClampSidesFor(key, center, Lod1Resolution);
            if (lod0Groups.TryGetValue(key, out var group)
                && group.clampSides != clampSides)
            {
                DestroyGroup(group);
                lod0Groups.Remove(key);
            }
            if (!lod0Groups.ContainsKey(key))
            {
                lod0Groups.Add(key, CreateGroup(0, key, Lod1Resolution, clampSides));
                grassTilesDirty = true;
            }
            SetLod1TileActive(key, false);
        });
    }

    private void CollectOutsideNeighborhood(
        Dictionary<Vector2Int, TileGroup> groups,
        Vector2Int center,
        int resolution)
    {
        removalScratch.Clear();
        foreach (var key in groups.Keys)
        {
            if (key.x < 0
                || key.y < 0
                || key.x >= resolution
                || key.y >= resolution
                || Mathf.Abs(key.x - center.x) > NearbyRadius
                || Mathf.Abs(key.y - center.y) > NearbyRadius)
            {
                removalScratch.Add(key);
            }
        }
    }

    private static void ForEachNeighbour(Vector2Int center, int resolution, Action<Vector2Int> action)
    {
        var minimumX = Mathf.Max(center.x - NearbyRadius, 0);
        var maximumX = Mathf.Min(center.x + NearbyRadius, resolution - 1);
        var minimumY = Mathf.Max(center.y - NearbyRadius, 0);
        var maximumY = Mathf.Min(center.y + NearbyRadius, resolution - 1);
        for (var y = minimumY; y <= maximumY; y++)
        {
            for (var x = minimumX; x <= maximumX; x++)
            {
                action(new Vector2Int(x, y));
            }
        }
    }

    internal static byte ClampSidesFor(Vector2Int key, Vector2Int center, int resolution)
    {
        var minimumX = Mathf.Max(center.x - NearbyRadius, 0);
        var maximumX = Mathf.Min(center.x + NearbyRadius, resolution - 1);
        var minimumY = Mathf.Max(center.y - NearbyRadius, 0);
        var maximumY = Mathf.Min(center.y + NearbyRadius, resolution - 1);
        byte sides = 0;
        if (key.y == maximumY && maximumY < resolution - 1)
        {
            sides |= ClampTop;
        }
        if (key.x == minimumX && minimumX > 0)
        {
            sides |= ClampLeft;
        }
        if (key.y == minimumY && minimumY > 0)
        {
            sides |= ClampBottom;
        }
        if (key.x == maximumX && maximumX < resolution - 1)
        {
            sides |= ClampRight;
        }
        return sides;
    }

    private async Task<TileGroup> CreatePreparedGroupAsync(
        int lod,
        Vector2Int parent,
        IslandPreparedMesh[] preparedMeshes,
        CancellationToken cancellationToken)
    {
        if (preparedMeshes == null || preparedMeshes.Length != Divisions * Divisions)
        {
            throw new InvalidOperationException("The prepared overview tile batch is invalid.");
        }

        GameObject root = null;
        var tiles = new Tile[preparedMeshes.Length];
        try
        {
            root = new GameObject($"LOD {lod} group {parent.x},{parent.y}");
            root.transform.SetParent(transform, false);
            for (var index = 0; index < preparedMeshes.Length; index++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var preparedMesh = preparedMeshes[index];
                if (preparedMesh != null)
                {
                    var mesh = IslandGenerator.CreateTerrainMesh(preparedMesh, lod);
                    var localX = index % Divisions;
                    var localY = index / Divisions;
                    var tileObject = new GameObject($"LOD {lod} tile {localX},{localY}");
                    tileObject.transform.SetParent(root.transform, false);
                    tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                    ConfigureTerrainRenderer(tileObject, lod);
                    tiles[index] = new Tile(tileObject, mesh, CreateEdgeMesh(mesh));
                }

                // Mesh and tangent uploads must use Unity's main thread. Spreading
                // them across frames avoids replacing one long native stall with
                // a large render-resource upload hitch.
                if ((index & 3) == 3)
                {
                    await Task.Yield();
                }
            }
            return new TileGroup(root, tiles, 0);
        }
        catch
        {
            DestroyGroup(new TileGroup(root, tiles, 0));
            throw;
        }
    }

    private TileGroup CreateGroup(
        int lod,
        Vector2Int parent,
        int parentResolution,
        byte clampSides)
    {
        var divisions = lod == 0 ? Lod0Divisions : Divisions;
        var inverseResolution = 1f / parentResolution;
        var area = new MotuNative.ExportArea(
            parent.x * inverseResolution,
            parent.y * inverseResolution,
            (parent.x + 1) * inverseResolution,
            (parent.y + 1) * inverseResolution);
        MotuNative.CreateMeshGrid(
            islandHandle,
            ref area,
            lod,
            divisions,
            clampSides,
            out var export);
        GameObject root = null;
        var tiles = Array.Empty<Tile>();
        try
        {
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != divisions * divisions)
            {
                throw new InvalidOperationException("The Rust grid slicer returned an invalid tile batch.");
            }

            root = new GameObject($"LOD {lod} group {parent.x},{parent.y}");
            root.transform.SetParent(transform, false);
            tiles = new Tile[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle == IntPtr.Zero || nativeMesh.triangles.length == 0)
                {
                    continue;
                }
                var mesh = IslandGenerator.CopyTerrainMesh(nativeMesh, lod, worldSize);
                var localX = index % divisions;
                var localY = index / divisions;
                var globalX = parent.x * divisions + localX;
                var globalY = parent.y * divisions + localY;
                var tileObject = new GameObject($"LOD {lod} tile {globalX},{globalY}");
                tileObject.transform.SetParent(root.transform, false);
                tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                ConfigureTerrainRenderer(tileObject, lod);
                tiles[index] = new Tile(tileObject, mesh, CreateEdgeMesh(mesh));
            }
            return new TileGroup(root, tiles, clampSides);
        }
        catch
        {
            DestroyGroup(new TileGroup(root, tiles, clampSides));
            throw;
        }
        finally
        {
            MotuNative.ReleaseMeshGrid(ref export);
        }
    }

    private void ConfigureTerrainRenderer(GameObject tileObject, int lod)
    {
        var renderer = tileObject.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = terrainMaterial;
        if (lod == 0)
        {
            renderer.SetPropertyBlock(lod0MaterialProperties);
        }
    }

    internal static Mesh CreateEdgeMesh(Mesh source)
    {
        var triangles = source.GetIndices(0);
        var uniqueEdges = new HashSet<ulong>(triangles.Length);
        var lineIndices = new List<int>(triangles.Length);
        for (var index = 0; index + 2 < triangles.Length; index += 3)
        {
            AddUniqueEdge(
                uniqueEdges,
                lineIndices,
                triangles[index],
                triangles[index + 1]);
            AddUniqueEdge(
                uniqueEdges,
                lineIndices,
                triangles[index + 1],
                triangles[index + 2]);
            AddUniqueEdge(
                uniqueEdges,
                lineIndices,
                triangles[index + 2],
                triangles[index]);
        }

        var edgeMesh = new Mesh
        {
            name = $"{source.name} Edges",
            indexFormat = source.indexFormat,
            vertices = source.vertices,
            bounds = source.bounds,
        };
        edgeMesh.SetIndices(lineIndices, MeshTopology.Lines, 0, false);
        edgeMesh.UploadMeshData(true);
        return edgeMesh;
    }

    private static void AddUniqueEdge(
        HashSet<ulong> uniqueEdges,
        List<int> lineIndices,
        int first,
        int second)
    {
        if (first == second)
        {
            return;
        }
        var minimum = Mathf.Min(first, second);
        var maximum = Mathf.Max(first, second);
        var key = ((ulong)(uint)minimum << 32) | (uint)maximum;
        if (!uniqueEdges.Add(key))
        {
            return;
        }
        lineIndices.Add(minimum);
        lineIndices.Add(maximum);
    }

    private void UpdateGrassTiles(Vector3 playerPosition)
    {
        var movement = playerPosition - lastGrassPosition;
        movement.y = 0f;
        if (!grassTilesDirty
            && movement.sqrMagnitude
                < GrassPositionUpdateDistance * GrassPositionUpdateDistance)
        {
            return;
        }

        foreach (var group in lod0Groups.Values)
        {
            foreach (var tile in group.tiles)
            {
                if (tile != null)
                {
                    SetGrassActive(tile, IntersectsGrassRadius(tile.mesh.bounds, playerPosition));
                }
            }
        }
        lastGrassPosition = playerPosition;
        grassTilesDirty = false;
    }

    private bool IntersectsGrassRadius(Bounds bounds, Vector3 playerPosition)
    {
        var xDistance = Mathf.Max(
            Mathf.Abs(playerPosition.x - bounds.center.x) - bounds.extents.x,
            0f);
        var zDistance = Mathf.Max(
            Mathf.Abs(playerPosition.z - bounds.center.z) - bounds.extents.z,
            0f);
        return xDistance * xDistance + zDistance * zDistance
            <= grassBoundsRadius * grassBoundsRadius;
    }

    private void SetGrassActive(Tile tile, bool active)
    {
        if (!active)
        {
            tile.grassObject?.SetActive(false);
            return;
        }
        if (tile.grassObject == null)
        {
            tile.grassObject = new GameObject("Grass shells");
            tile.grassObject.transform.SetParent(tile.gameObject.transform, false);
            tile.grassObject.AddComponent<MeshFilter>().sharedMesh = tile.mesh;
            var renderer = tile.grassObject.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = grassMaterial;
            renderer.shadowCastingMode = UnityEngine.Rendering.ShadowCastingMode.Off;
            renderer.receiveShadows = true;
        }
        tile.grassObject.SetActive(true);
    }

    private static void SetAllGrassInactive(TileGroup group)
    {
        if (group == null)
        {
            return;
        }
        foreach (var tile in group.tiles)
        {
            tile?.grassObject?.SetActive(false);
        }
    }

    private TileGroup CreateRiverGroup(Vector2Int parent)
    {
        return CreatePreparedFeatureGroup(
            parent,
            preparedRiverTiles,
            riverRoot,
            riverMaterial,
            "River");
    }

    private TileGroup CreateRiverRockGroup(Vector2Int parent)
    {
        return CreatePreparedFeatureGroup(
            parent,
            preparedRiverRockTiles,
            riverRockRoot,
            rockMaterial,
            "River rock");
    }

    private static TileGroup CreatePreparedFeatureGroup(
        Vector2Int parent,
        IslandPreparedMesh[] preparedTiles,
        GameObject featureRoot,
        Material material,
        string label)
    {
        GameObject root = null;
        var tiles = new Tile[Divisions * Divisions];
        try
        {
            root = new GameObject($"{label} group {parent.x},{parent.y}");
            root.layer = featureRoot.layer;
            root.transform.SetParent(featureRoot.transform, false);
            for (var localY = 0; localY < Divisions; localY++)
            {
                for (var localX = 0; localX < Divisions; localX++)
                {
                    var globalX = parent.x * Divisions + localX;
                    var globalY = parent.y * Divisions + localY;
                    var preparedIndex = globalY * Lod1Resolution + globalX;
                    var preparedMesh = preparedTiles[preparedIndex];
                    if (preparedMesh == null)
                    {
                        continue;
                    }

                    var mesh = IslandGenerator.CreateRiverMesh(preparedMesh);
                    preparedTiles[preparedIndex] = null;
                    var tileIndex = localY * Divisions + localX;
                    var tileObject = new GameObject($"{label} LOD 1 tile {globalX},{globalY}");
                    tileObject.layer = featureRoot.layer;
                    tileObject.transform.SetParent(root.transform, false);
                    tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                    tileObject.AddComponent<MeshRenderer>().sharedMaterial = material;
                    tiles[tileIndex] = new Tile(tileObject, mesh);
                }
            }
            return new TileGroup(root, tiles, 0);
        }
        catch
        {
            DestroyGroup(new TileGroup(root, tiles, 0));
            throw;
        }
    }

    private void SetRiverGroupActive(Vector2Int key, bool active)
    {
        if (!riverGroups.TryGetValue(key, out var group))
        {
            if (!active)
            {
                return;
            }
            group = CreateRiverGroup(key);
            riverGroups.Add(key, group);
        }
        group.root?.SetActive(active);
    }

    private void SetRiverRockGroupActive(Vector2Int key, bool active)
    {
        if (!riverRockGroups.TryGetValue(key, out var group))
        {
            if (!active)
            {
                return;
            }
            group = CreateRiverRockGroup(key);
            riverRockGroups.Add(key, group);
        }
        group.root?.SetActive(active);
    }

    private void UpdateColliderNeighborhood(Vector2Int center)
    {
        var created = new List<KeyValuePair<Vector2Int, ColliderTile>>(9);
        try
        {
            var minimumX = Mathf.Max(center.x - NearbyRadius, 0);
            var maximumX = Mathf.Min(center.x + NearbyRadius, Lod1Resolution - 1);
            var minimumY = Mathf.Max(center.y - NearbyRadius, 0);
            var maximumY = Mathf.Min(center.y + NearbyRadius, Lod1Resolution - 1);
            for (var y = minimumY; y <= maximumY; y++)
            {
                for (var x = minimumX; x <= maximumX; x++)
                {
                    var key = new Vector2Int(x, y);
                    if (!colliderTiles.ContainsKey(key))
                    {
                        created.Add(new KeyValuePair<Vector2Int, ColliderTile>(
                            key,
                            CreateColliderTile(key)));
                    }
                }
            }
        }
        catch
        {
            foreach (var entry in created)
            {
                DestroyColliderTile(entry.Value);
            }
            throw;
        }

        // Install the complete incoming set before retiring any old collider.
        foreach (var entry in created)
        {
            colliderTiles.Add(entry.Key, entry.Value);
        }

        removalScratch.Clear();
        foreach (var key in colliderTiles.Keys)
        {
            if (Mathf.Abs(key.x - center.x) > NearbyRadius
                || Mathf.Abs(key.y - center.y) > NearbyRadius)
            {
                removalScratch.Add(key);
            }
        }
        foreach (var key in removalScratch)
        {
            var tile = colliderTiles[key];
            tile.collider.enabled = false;
            colliderTiles.Remove(key);
            DestroyColliderTile(tile);
        }
    }

    private ColliderTile CreateColliderTile(Vector2Int key)
    {
        var terrainData = new TerrainData
        {
            name = $"LOD 1 collision heightfield {key.x},{key.y}",
            heightmapResolution = ColliderSamplesPerTile,
        };
        GameObject tileObject = null;
        try
        {
            if (terrainData.heightmapResolution != ColliderSamplesPerTile)
            {
                throw new InvalidOperationException(
                    $"Unity rejected terrain heightmap resolution {ColliderSamplesPerTile}.");
            }

            var tileSize = worldSize / Lod1Resolution;
            terrainData.size = new Vector3(
                tileSize,
                colliderHeightMap.verticalSize,
                tileSize);
            terrainData.SetHeights(0, 0, colliderHeightMap.CopyTileHeights(key));

            tileObject = new GameObject($"LOD 1 terrain collider {key.x},{key.y}");
            tileObject.transform.SetParent(colliderRoot.transform, false);
            tileObject.transform.localPosition = new Vector3(
                -worldSize * 0.5f + key.x * tileSize,
                colliderHeightMap.verticalOrigin,
                -worldSize * 0.5f + key.y * tileSize);

            var hiddenTerrain = tileObject.AddComponent<Terrain>();
            hiddenTerrain.terrainData = terrainData;
            hiddenTerrain.allowAutoConnect = false;
            hiddenTerrain.drawHeightmap = false;
            hiddenTerrain.drawTreesAndFoliage = false;
            hiddenTerrain.enabled = false;

            var terrainCollider = tileObject.AddComponent<TerrainCollider>();
            terrainCollider.terrainData = terrainData;
            terrainCollider.enabled = true;
            return new ColliderTile(tileObject, terrainData, terrainCollider);
        }
        catch
        {
            DestroyUnityObject(tileObject);
            DestroyUnityObject(terrainData);
            throw;
        }
    }

    private void RemoveAllColliderTiles()
    {
        foreach (var tile in colliderTiles.Values)
        {
            tile.collider.enabled = false;
            DestroyColliderTile(tile);
        }
        colliderTiles.Clear();
    }

    private static void DestroyColliderTile(ColliderTile tile)
    {
        if (tile == null)
        {
            return;
        }
        DestroyUnityObject(tile.gameObject);
        DestroyUnityObject(tile.terrainData);
    }

    private void SetLod2TileActive(Vector2Int key, bool active)
    {
        var tile = lod2Group?.tiles[key.y * Divisions + key.x];
        if (tile != null)
        {
            tile.gameObject.SetActive(active);
        }
    }

    private void SetLod1TileActive(Vector2Int key, bool active)
    {
        var parent = new Vector2Int(key.x / Divisions, key.y / Divisions);
        if (!lod1Groups.TryGetValue(parent, out var group))
        {
            return;
        }
        var tile = group.tiles[(key.y % Divisions) * Divisions + key.x % Divisions];
        if (tile != null)
        {
            tile.gameObject.SetActive(active);
        }
    }

    private Vector2Int LocalCell(Vector3 position, int resolution)
    {
        var normalizedX = Mathf.Clamp01(position.x / worldSize + 0.5f);
        var normalizedY = Mathf.Clamp01(position.z / worldSize + 0.5f);
        return new Vector2Int(
            Mathf.Min(Mathf.FloorToInt(normalizedX * resolution), resolution - 1),
            Mathf.Min(Mathf.FloorToInt(normalizedY * resolution), resolution - 1));
    }

#if UNITY_EDITOR
    internal void ValidateColliderStreaming(
        IslandPreparedColliderHeightMap preparedHeightMap,
        float terrainWorldSize)
    {
        colliderHeightMap = preparedHeightMap;
        worldSize = terrainWorldSize;
        colliderRoot = new GameObject("Terrain collider streaming validation");
        colliderRoot.transform.SetParent(transform, false);
        try
        {
            var firstCenter = new Vector2Int(10, 10);
            UpdateColliderNeighborhood(firstCenter);
            if (colliderTiles.Count != 9
                || !colliderTiles.TryGetValue(firstCenter, out var retainedCenter))
            {
                throw new InvalidOperationException(
                    "The initial terrain-collider neighbourhood is incomplete.");
            }

            var nextCenter = new Vector2Int(11, 10);
            UpdateColliderNeighborhood(nextCenter);
            if (colliderTiles.Count != 9
                || !colliderTiles.ContainsKey(nextCenter)
                || !colliderTiles.TryGetValue(firstCenter, out var sharedTile)
                || !ReferenceEquals(retainedCenter, sharedTile))
            {
                throw new InvalidOperationException(
                    "Terrain-collider transition coverage or tile reuse is invalid.");
            }

            Physics.SyncTransforms();
            var tileSize = worldSize / Lod1Resolution;
            var point = new Vector3(
                -worldSize * 0.5f + (nextCenter.x + 0.5f) * tileSize,
                0f,
                -worldSize * 0.5f + (nextCenter.y + 0.5f) * tileSize);
            if (!TrySnapToCurrentCollider(point, out _))
            {
                throw new InvalidOperationException(
                    "The transitioned terrain-collider neighbourhood cannot be raycast.");
            }
        }
        finally
        {
            RemoveAllColliderTiles();
            DestroyUnityObject(colliderRoot);
            colliderRoot = null;
            colliderHeightMap = null;
        }
    }
#endif

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
