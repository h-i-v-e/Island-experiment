using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using Unity.Profiling;
using UnityEngine.Rendering;

public sealed partial class TerrainTileStreamer
{
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
        RebuildTerrainBatchIfDirty(lod2Group);
    }

    private IEnumerator UpdateLod1NeighborhoodIncremental(Vector2Int center)
    {
        var replacements = new List<KeyValuePair<Vector2Int, TileGroup>>(9);
        var desired = NeighbourKeys(center, Lod2Resolution);
        try
        {
            foreach (var key in desired)
            {
                var clampSides = ClampSidesFor(key, center, Lod2Resolution);
                if (lod1Groups.TryGetValue(key, out var existing)
                    && existing.clampSides == clampSides)
                {
                    continue;
                }
                TileGroup replacement;
                using (Lod1TransitionMarker.Auto())
                {
                    replacement = CreateGroup(1, key, Lod2Resolution, clampSides);
                }
                replacement.root.SetActive(false);
                pendingTransitionGroups.Add(replacement);
                replacements.Add(
                    new KeyValuePair<Vector2Int, TileGroup>(key, replacement));
                yield return null;
                if (center != requestedLod2)
                {
                    yield break;
                }
            }

            var retired = new List<TileGroup>(lod1Groups.Count);
            foreach (var replacement in replacements)
            {
                if (lod1Groups.TryGetValue(replacement.Key, out var existing))
                {
                    lod1Groups.Remove(replacement.Key);
                    retired.Add(existing);
                }
                lod1Groups.Add(replacement.Key, replacement.Value);
                pendingTransitionGroups.Remove(replacement.Value);
                replacement.Value.root.SetActive(true);
            }

            var desiredSet = new HashSet<Vector2Int>(desired);
            removalScratch.Clear();
            foreach (var key in lod1Groups.Keys)
            {
                if (!desiredSet.Contains(key))
                {
                    removalScratch.Add(key);
                }
            }
            foreach (var key in removalScratch)
            {
                retired.Add(lod1Groups[key]);
                lod1Groups.Remove(key);
                SetLod2TileActive(key, true);
                SetRiverGroupActive(key, false);
                SetRiverRockGroupActive(key, false);
            }

            foreach (var key in desired)
            {
                SetLod2TileActive(key, false);
                SetRiverGroupActive(key, true);
                SetRiverRockGroupActive(key, true);
            }
            RebuildTerrainBatchIfDirty(lod2Group);

            foreach (var group in retired)
            {
                DestroyGroup(group);
                yield return null;
            }
        }
        finally
        {
            foreach (var pending in replacements)
            {
                if (!lod1Groups.TryGetValue(pending.Key, out var installed)
                    || !ReferenceEquals(installed, pending.Value))
                {
                    pendingTransitionGroups.Remove(pending.Value);
                    DestroyGroup(pending.Value);
                }
                else
                {
                    pendingTransitionGroups.Remove(pending.Value);
                }
            }
        }
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
        RebuildDirtyLod1Batches();
    }

    private IEnumerator UpdateLod0NeighborhoodIncremental(Vector2Int center)
    {
        var replacements = new List<KeyValuePair<Vector2Int, TileGroup>>(9);
        var desired = NeighbourKeys(center, Lod1Resolution);
        try
        {
            foreach (var key in desired)
            {
                var clampSides = ClampSidesFor(key, center, Lod1Resolution);
                if (lod0Groups.TryGetValue(key, out var existing)
                    && existing.clampSides == clampSides)
                {
                    continue;
                }
                TileGroup replacement;
                using (Lod0TransitionMarker.Auto())
                {
                    replacement = CreateGroup(0, key, Lod1Resolution, clampSides);
                }
                replacement.root.SetActive(false);
                pendingTransitionGroups.Add(replacement);
                replacements.Add(
                    new KeyValuePair<Vector2Int, TileGroup>(key, replacement));
                yield return null;
                if (center != requestedLod1)
                {
                    yield break;
                }
            }

            var retired = new List<TileGroup>(lod0Groups.Count);
            foreach (var replacement in replacements)
            {
                if (lod0Groups.TryGetValue(replacement.Key, out var existing))
                {
                    lod0Groups.Remove(replacement.Key);
                    retired.Add(existing);
                }
                lod0Groups.Add(replacement.Key, replacement.Value);
                pendingTransitionGroups.Remove(replacement.Value);
                replacement.Value.root.SetActive(true);
            }

            var desiredSet = new HashSet<Vector2Int>(desired);
            removalScratch.Clear();
            foreach (var key in lod0Groups.Keys)
            {
                if (!desiredSet.Contains(key))
                {
                    removalScratch.Add(key);
                }
            }
            foreach (var key in removalScratch)
            {
                retired.Add(lod0Groups[key]);
                lod0Groups.Remove(key);
                SetLod1TileActive(key, true);
            }
            foreach (var key in desired)
            {
                SetLod1TileActive(key, false);
            }
            RebuildDirtyLod1Batches();
            grassTilesDirty = true;

            foreach (var group in retired)
            {
                DestroyGroup(group);
                yield return null;
            }
        }
        finally
        {
            foreach (var pending in replacements)
            {
                if (!lod0Groups.TryGetValue(pending.Key, out var installed)
                    || !ReferenceEquals(installed, pending.Value))
                {
                    pendingTransitionGroups.Remove(pending.Value);
                    DestroyGroup(pending.Value);
                }
                else
                {
                    pendingTransitionGroups.Remove(pending.Value);
                }
            }
        }
    }

    private static List<Vector2Int> NeighbourKeys(Vector2Int center, int resolution)
    {
        var result = new List<Vector2Int>(9);
        ForEachNeighbour(center, resolution, result.Add);
        return result;
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
        CancellationToken cancellationToken,
        UnityFrameBudget installationBudget)
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
                    var mesh = IslandMeshInterop.CreateTerrainMesh(preparedMesh, lod);
                    var localX = index % Divisions;
                    var localY = index / Divisions;
                    var tileObject = new GameObject($"LOD {lod} tile {localX},{localY}");
                    tileObject.transform.SetParent(root.transform, false);
                    tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                    tiles[index] = new Tile(tileObject, mesh);
                }

                // Mesh and tangent uploads must use Unity's main thread. Yield
                // according to elapsed upload time rather than a fixed mesh count,
                // because generated tile complexity varies considerably.
                await installationBudget.YieldIfExceededAsync(cancellationToken);
            }
            var group = new TileGroup(root, tiles, 0);
            ConfigureTerrainBatch(group, lod);
            return group;
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
                var mesh = IslandMeshInterop.CopyTerrainMesh(
                    nativeMesh,
                    lod,
                    worldSize);
                var localX = index % divisions;
                var localY = index / divisions;
                var globalX = parent.x * divisions + localX;
                var globalY = parent.y * divisions + localY;
                var tileObject = new GameObject($"LOD {lod} tile {globalX},{globalY}");
                tileObject.transform.SetParent(root.transform, false);
                tileObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                if (lod == 0)
                {
                    ConfigureTerrainRenderer(tileObject);
                }
                tiles[index] = new Tile(tileObject, mesh);
            }
            var group = new TileGroup(root, tiles, clampSides);
            ConfigureTerrainBatch(group, lod);
            return group;
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

    private void ConfigureTerrainRenderer(GameObject tileObject)
    {
        var renderer = tileObject.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = terrainMaterial;
        renderer.SetPropertyBlock(lod0MaterialProperties);
    }

    private void ConfigureTerrainBatch(TileGroup group, int lod)
    {
        if (group == null || lod == 0)
        {
            return;
        }

        var instances = new List<CombineInstance>(group.tiles.Length);
        var vertexOffset = 0;
        var totalIndexCount = 0;
        foreach (var tile in group.tiles)
        {
            if (tile == null)
            {
                continue;
            }
            var sourceIndices = tile.mesh.GetIndices(0);
            tile.batchIndices = new int[sourceIndices.Length];
            for (var index = 0; index < sourceIndices.Length; index++)
            {
                tile.batchIndices[index] = sourceIndices[index] + vertexOffset;
            }
            totalIndexCount = checked(totalIndexCount + sourceIndices.Length);
            instances.Add(new CombineInstance
            {
                mesh = tile.mesh,
                subMeshIndex = 0,
                transform = Matrix4x4.identity,
            });
            vertexOffset = checked(vertexOffset + tile.mesh.vertexCount);
        }

        if (instances.Count == 0)
        {
            return;
        }

        GameObject batchObject = null;
        Mesh batchMesh = null;
        try
        {
            batchMesh = new Mesh
            {
                name = $"Terrain LOD {lod} combined batch",
                indexFormat = vertexOffset > ushort.MaxValue
                    ? IndexFormat.UInt32
                    : IndexFormat.UInt16,
            };
            batchMesh.CombineMeshes(instances.ToArray(), true, false, false);
            batchMesh.UploadMeshData(false);

            batchObject = new GameObject($"Terrain LOD {lod} combined batch");
            batchObject.layer = group.root.layer;
            batchObject.transform.SetParent(group.root.transform, false);
            batchObject.AddComponent<MeshFilter>().sharedMesh = batchMesh;
            batchObject.AddComponent<MeshRenderer>().sharedMaterial = lod == 1
                ? terrainLod1Material
                : terrainLod2Material;

            group.batchObject = batchObject;
            group.batchMesh = batchMesh;
            group.activeBatchIndices.Capacity = totalIndexCount;
            group.batchDirty = true;
            RebuildTerrainBatchIfDirty(group);
        }
        catch
        {
            DestroyUnityObject(batchObject);
            DestroyUnityObject(batchMesh);
            throw;
        }
    }

    private static void SetBatchedTileActive(
        TileGroup group,
        int tileIndex,
        bool active)
    {
        if (group == null
            || tileIndex < 0
            || tileIndex >= group.tiles.Length)
        {
            return;
        }
        var tile = group.tiles[tileIndex];
        if (tile == null || tile.gameObject.activeSelf == active)
        {
            return;
        }
        tile.gameObject.SetActive(active);
        if (group.batchMesh != null)
        {
            group.batchDirty = true;
        }
    }

    private static void RebuildTerrainBatchIfDirty(TileGroup group)
    {
        if (group?.batchMesh == null || !group.batchDirty)
        {
            return;
        }
        group.activeBatchIndices.Clear();
        foreach (var tile in group.tiles)
        {
            if (tile?.batchIndices != null && tile.gameObject.activeSelf)
            {
                group.activeBatchIndices.AddRange(tile.batchIndices);
            }
        }
        group.batchMesh.SetIndices(
            group.activeBatchIndices,
            MeshTopology.Triangles,
            0,
            false);
        group.batchObject.SetActive(group.activeBatchIndices.Count != 0);
        group.batchDirty = false;
    }

    private void RebuildDirtyLod1Batches()
    {
        foreach (var group in lod1Groups.Values)
        {
            RebuildTerrainBatchIfDirty(group);
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

                    var mesh = IslandMeshInterop.CreateRiverMesh(preparedMesh);
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

}
