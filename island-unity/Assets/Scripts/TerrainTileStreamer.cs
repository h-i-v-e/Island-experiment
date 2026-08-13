using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

public sealed class TerrainTileStreamer : MonoBehaviour
{
    private static readonly int WorldNormalWeightId = Shader.PropertyToID("_WorldNormalWeight");
    private const int Divisions = 8;
    internal const byte ClampTop = 1;
    internal const byte ClampLeft = 2;
    internal const byte ClampBottom = 4;
    internal const byte ClampRight = 8;
    private const int NearbyRadius = 1;
    private const int Lod2Resolution = Divisions;
    internal const int Lod1Resolution = Divisions * Divisions;
    private const int Lod0Resolution = Divisions * Divisions * Divisions;

    private sealed class Tile
    {
        internal readonly GameObject gameObject;
        internal readonly Mesh mesh;

        internal Tile(GameObject gameObject, Mesh mesh)
        {
            this.gameObject = gameObject;
            this.mesh = mesh;
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

    private readonly Dictionary<Vector2Int, TileGroup> lod1Groups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly Dictionary<Vector2Int, TileGroup> lod0Groups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly Dictionary<Vector2Int, TileGroup> riverGroups =
        new Dictionary<Vector2Int, TileGroup>();
    private readonly List<Vector2Int> removalScratch = new List<Vector2Int>(9);

    private IntPtr islandHandle;
    private Material terrainMaterial;
    private MaterialPropertyBlock lod0MaterialProperties;
    private Material riverMaterial;
    private IslandViewer.PreparedMesh[] preparedRiverTiles;
    private float worldSize;
    private TileGroup lod2Group;
    private GameObject riverRoot;
    private MeshCollider currentCollider;
    private Mesh currentColliderMesh;
    private Vector2Int currentLod2 = new Vector2Int(-1, -1);
    private Vector2Int currentLod1 = new Vector2Int(-1, -1);
    private Vector2Int currentLod0 = new Vector2Int(-1, -1);

    public int BaseVertexCount { get; private set; }
    public int BaseTriangleCount { get; private set; }
    public bool UseRenderCollider { get; set; } = true;
    internal int Lod1GroupCount => lod1Groups.Count;
    internal int Lod0GroupCount => lod0Groups.Count;
    internal bool HasCurrentCollider => currentCollider != null;
    internal byte Lod1ClampSidesAt(Vector2Int key) => lod1Groups[key].clampSides;
    internal byte Lod0ClampSidesAt(Vector2Int key) => lod0Groups[key].clampSides;

    internal async Task InitializeAsync(
        IntPtr handle,
        Material sharedTerrainMaterial,
        Material waterMaterial,
        float terrainWorldSize,
        IslandViewer.PreparedMesh[] overviewTiles,
        IslandViewer.PreparedMesh[] riverTiles,
        bool showRivers,
        CancellationToken cancellationToken)
    {
        islandHandle = handle;
        terrainMaterial = sharedTerrainMaterial;
        lod0MaterialProperties = new MaterialPropertyBlock();
        lod0MaterialProperties.SetFloat(WorldNormalWeightId, 0f);
        riverMaterial = waterMaterial;
        preparedRiverTiles = riverTiles;
        worldSize = terrainWorldSize;
        if (preparedRiverTiles == null
            || preparedRiverTiles.Length != Lod1Resolution * Lod1Resolution)
        {
            throw new InvalidOperationException("The prepared river tile batch is invalid.");
        }
        riverRoot = new GameObject("Rivers");
        riverRoot.transform.SetParent(transform, false);
        riverRoot.transform.localPosition = Vector3.up * 0.025f;
        riverRoot.SetActive(showRivers);
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

    internal static IslandViewer.PreparedMesh[] PrepareOverviewTiles(IntPtr handle)
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

            var result = new IslandViewer.PreparedMesh[export.length];
            var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < export.length; index++)
            {
                var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                    IntPtr.Add(export.data, index * exportSize));
                if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                {
                    result[index] = IslandViewer.CopyTerrainMeshData(nativeMesh, 2);
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
        var lod2 = WorldCell(worldPosition, Lod2Resolution);
        var lod1 = WorldCell(worldPosition, Lod1Resolution);
        var lod0 = WorldCell(worldPosition, Lod0Resolution);

        if (lod2 != currentLod2)
        {
            RemoveCollider();
            UpdateLod1Neighborhood(lod2);
            currentLod2 = lod2;
        }
        if (lod1 != currentLod1)
        {
            RemoveCollider();
            UpdateLod0Neighborhood(lod1);
            currentLod1 = lod1;
        }
        if (lod0 != currentLod0)
        {
            MoveCollider(lod0);
            currentLod0 = lod0;
        }
    }

    public void ClearPlayerFocus()
    {
        RemoveCollider();
        foreach (var group in riverGroups.Values)
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
        currentLod0 = new Vector2Int(-1, -1);
    }

    public bool TryRaycastOverview(Ray ray, out Vector3 point)
    {
        var nearest = float.PositiveInfinity;
        point = default;
        if (lod2Group == null)
        {
            return false;
        }

        foreach (var tile in lod2Group.tiles)
        {
            if (tile == null || !tile.mesh.bounds.IntersectRay(ray))
            {
                continue;
            }
            var vertices = tile.mesh.vertices;
            var triangles = tile.mesh.triangles;
            for (var index = 0; index + 2 < triangles.Length; index += 3)
            {
                if (RayTriangle(
                    ray,
                    vertices[triangles[index]],
                    vertices[triangles[index + 1]],
                    vertices[triangles[index + 2]],
                    out var distance)
                    && distance < nearest)
                {
                    nearest = distance;
                    point = ray.GetPoint(distance);
                }
            }
        }
        return nearest < float.PositiveInfinity;
    }

    public bool TrySnapToCurrentCollider(Vector3 approximatePoint, out Vector3 point)
    {
        point = approximatePoint;
        if (currentCollider == null)
        {
            return false;
        }
        var origin = new Vector3(approximatePoint.x, worldSize, approximatePoint.z);
        if (!currentCollider.Raycast(new Ray(origin, Vector3.down), out var hit, worldSize * 2f))
        {
            return false;
        }
        point = hit.point;
        return true;
    }

    public void Dispose()
    {
        ClearPlayerFocus();
        foreach (var group in riverGroups.Values)
        {
            DestroyGroup(group);
        }
        riverGroups.Clear();
        DestroyUnityObject(riverRoot);
        riverRoot = null;
        preparedRiverTiles = null;
        if (lod2Group != null)
        {
            DestroyGroup(lod2Group);
            lod2Group = null;
        }
        islandHandle = IntPtr.Zero;
    }

    public void SetRiversVisible(bool visible)
    {
        riverRoot?.SetActive(visible);
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
        IslandViewer.PreparedMesh[] preparedMeshes,
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
                    var mesh = IslandViewer.CreateTerrainMesh(preparedMesh, lod);
                    var localX = index % Divisions;
                    var localY = index / Divisions;
                    var tileObject = new GameObject($"LOD {lod} tile {localX},{localY}");
                    tileObject.transform.SetParent(root.transform, false);
                    tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                    ConfigureTerrainRenderer(tileObject, lod);
                    tiles[index] = new Tile(tileObject, mesh);
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
            Divisions,
            clampSides,
            out var export);
        GameObject root = null;
        var tiles = Array.Empty<Tile>();
        try
        {
            if (export.handle == IntPtr.Zero
                || export.data == IntPtr.Zero
                || export.length != Divisions * Divisions)
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
                var mesh = IslandViewer.CopyTerrainMesh(nativeMesh, lod);
                var localX = index % Divisions;
                var localY = index / Divisions;
                var globalX = parent.x * Divisions + localX;
                var globalY = parent.y * Divisions + localY;
                var tileObject = new GameObject($"LOD {lod} tile {globalX},{globalY}");
                tileObject.transform.SetParent(root.transform, false);
                tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                ConfigureTerrainRenderer(tileObject, lod);
                tiles[index] = new Tile(tileObject, mesh);
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

    private TileGroup CreateRiverGroup(Vector2Int parent)
    {
        GameObject root = null;
        var tiles = new Tile[Divisions * Divisions];
        try
        {
            root = new GameObject($"River group {parent.x},{parent.y}");
            root.transform.SetParent(riverRoot.transform, false);
            for (var localY = 0; localY < Divisions; localY++)
            {
                for (var localX = 0; localX < Divisions; localX++)
                {
                    var globalX = parent.x * Divisions + localX;
                    var globalY = parent.y * Divisions + localY;
                    var preparedIndex = globalY * Lod1Resolution + globalX;
                    var preparedMesh = preparedRiverTiles[preparedIndex];
                    if (preparedMesh == null)
                    {
                        continue;
                    }

                    var mesh = IslandViewer.CreateRiverMesh(preparedMesh);
                    preparedRiverTiles[preparedIndex] = null;
                    var tileIndex = localY * Divisions + localX;
                    var tileObject = new GameObject($"River LOD 1 tile {globalX},{globalY}");
                    tileObject.transform.SetParent(root.transform, false);
                    tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                    tileObject.AddComponent<MeshRenderer>().sharedMaterial = riverMaterial;
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

    private void MoveCollider(Vector2Int lod0Cell)
    {
        RemoveCollider();
        var parent = new Vector2Int(lod0Cell.x / Divisions, lod0Cell.y / Divisions);
        if (!lod0Groups.TryGetValue(parent, out var group))
        {
            return;
        }
        var localX = lod0Cell.x % Divisions;
        var localY = lod0Cell.y % Divisions;
        var tile = group.tiles[localY * Divisions + localX];
        if (tile == null)
        {
            return;
        }
        if (UseRenderCollider)
        {
            try
            {
                Physics.BakeMesh(tile.mesh.GetEntityId(), false);
                currentCollider = tile.gameObject.AddComponent<MeshCollider>();
                currentCollider.sharedMesh = tile.mesh;
                return;
            }
            catch (Exception exception)
            {
                Debug.LogWarning($"Render collider cooking failed; using support terrain: {exception.Message}");
            }
        }

        var inverseResolution = 1f / Lod0Resolution;
        var area = new MotuNative.ExportArea(
            lod0Cell.x * inverseResolution,
            lod0Cell.y * inverseResolution,
            (lod0Cell.x + 1) * inverseResolution,
            (lod0Cell.y + 1) * inverseResolution);
        var supportExport = new MotuNative.ExportMesh();
        var colliderMesh = tile.mesh;
        try
        {
            MotuNative.CreateSupportMesh(islandHandle, ref area, 0, out supportExport);
            if (supportExport.handle != IntPtr.Zero && supportExport.triangles.length != 0)
            {
                currentColliderMesh = IslandViewer.CopyTerrainMesh(supportExport, 0);
                colliderMesh = currentColliderMesh;
            }
        }
        catch (Exception exception)
        {
            Debug.LogWarning($"Support collider export failed; using the render tile: {exception.Message}");
            DestroyUnityObject(currentColliderMesh);
            currentColliderMesh = null;
        }
        finally
        {
            MotuNative.ReleaseMesh(ref supportExport);
        }

        currentCollider = tile.gameObject.AddComponent<MeshCollider>();
        currentCollider.sharedMesh = colliderMesh;
    }

    private void RemoveCollider()
    {
        if (currentCollider == null)
        {
            return;
        }
        currentCollider.enabled = false;
        DestroyUnityObject(currentCollider);
        currentCollider = null;
        DestroyUnityObject(currentColliderMesh);
        currentColliderMesh = null;
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

    private Vector2Int WorldCell(Vector3 position, int resolution)
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
