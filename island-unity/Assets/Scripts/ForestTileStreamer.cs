using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;

// Forest geometry is already spatially batched by Rust without splitting a
// placed tree's wood or a shared foliage cluster. This helper only owns the
// Unity lifetime and visual-LOD state; it never creates a GameObject or
// renderer for an individual tree or cluster.
internal sealed class ForestTileStreamer : IDisposable
{
    internal const int Lod2Resolution = 8;
    internal const int Lod1Resolution = 64;
    internal const int Lod2TileCount = Lod2Resolution * Lod2Resolution;
    internal const int Lod1TileCount = Lod1Resolution * Lod1Resolution;

    private const int Lod2TilePerLod1Region = Lod1Resolution / Lod2Resolution;
    private const int NearbyRadius = 1;

    private sealed class Tile
    {
        internal readonly GameObject foliageObject;
        internal readonly GameObject woodObject;
        internal readonly Mesh foliageMesh;
        internal readonly Mesh woodMesh;
        internal GameObject foliageEdgeObject;
        internal GameObject woodEdgeObject;
        internal Mesh foliageEdgeMesh;
        internal Mesh woodEdgeMesh;

        internal Tile(
            GameObject foliageObject,
            Mesh foliageMesh,
            GameObject woodObject,
            Mesh woodMesh)
        {
            this.foliageObject = foliageObject;
            this.foliageMesh = foliageMesh;
            this.woodObject = woodObject;
            this.woodMesh = woodMesh;
        }

        internal void SetActive(bool active)
        {
            foliageObject?.SetActive(active);
            woodObject?.SetActive(active);
        }
    }

    private sealed class Group
    {
        internal readonly GameObject foliageRoot;
        internal readonly GameObject woodRoot;
        internal readonly Dictionary<Vector2Int, Tile> tiles =
            new Dictionary<Vector2Int, Tile>();

        internal Group(GameObject foliageRoot, GameObject woodRoot)
        {
            this.foliageRoot = foliageRoot;
            this.woodRoot = woodRoot;
        }
    }

    private readonly Dictionary<Vector2Int, Tile> lod2Tiles =
        new Dictionary<Vector2Int, Tile>();
    private readonly Dictionary<Vector2Int, Group> lod1Groups =
        new Dictionary<Vector2Int, Group>();
    private readonly Dictionary<Vector2Int, Group> lod0Groups =
        new Dictionary<Vector2Int, Group>();
    private readonly List<Vector2Int> removalScratch = new List<Vector2Int>(64);
    private readonly List<Vector2Int> childRemovalScratch = new List<Vector2Int>(64);

    private GameObject forestRoot;
    private GameObject foliageRoot;
    private GameObject woodRoot;
    private GameObject canopyShadowRoot;
    private Material foliageMaterial;
    private Material lod0FoliageMaterial;
    private Material woodMaterial;
    private Material lod1WoodMaterial;
    private Material meshEdgeMaterial;
    private IslandPreparedMesh[] preparedLod2Foliage;
    private IslandPreparedMesh[] preparedLod1Foliage;
    private IslandPreparedMesh[] preparedLod1Wood;
    private IslandPreparedMesh[] preparedLod0Foliage;
    private IslandPreparedMesh[] preparedLod0Wood;
    private bool initialized;
    private bool meshEdgesVisible;

    internal int ActiveLod2TileCount => lod2Tiles.Count;
    internal int Lod1GroupCount => lod1Groups.Count;
    internal int Lod0GroupCount => lod0Groups.Count;
    internal GameObject Root => forestRoot;

    internal void Initialize(
        Transform parent,
        Material sharedFoliageMaterial,
        Material sharedLod0FoliageMaterial,
        Material sharedWoodMaterial,
        Material sharedLod1WoodMaterial,
        Material sharedMeshEdgeMaterial,
        IslandPreparedForestData prepared,
        bool showForests)
    {
        if (initialized)
        {
            throw new InvalidOperationException("The forest tile streamer is already initialized.");
        }
        if (parent == null)
        {
            throw new ArgumentNullException(nameof(parent));
        }
        if (prepared == null)
        {
            throw new ArgumentNullException(nameof(prepared));
        }
        if (sharedFoliageMaterial == null
            || sharedLod0FoliageMaterial == null
            || sharedWoodMaterial == null
            || sharedLod1WoodMaterial == null)
        {
            throw new InvalidOperationException(
                "Forest wood and foliage materials must be available before streaming.");
        }
        if (sharedMeshEdgeMaterial == null)
        {
            throw new InvalidOperationException(
                "The tree mesh-edge material must be available before streaming.");
        }

        foliageMaterial = sharedFoliageMaterial;
        lod0FoliageMaterial = sharedLod0FoliageMaterial;
        woodMaterial = sharedWoodMaterial;
        lod1WoodMaterial = sharedLod1WoodMaterial;
        meshEdgeMaterial = sharedMeshEdgeMaterial;
        preparedLod2Foliage = prepared.lod2FoliageTiles;
        preparedLod1Foliage = prepared.lod1FoliageTiles;
        preparedLod1Wood = prepared.lod1WoodTiles;
        preparedLod0Foliage = prepared.lod0FoliageTiles;
        preparedLod0Wood = prepared.lod0WoodTiles;

        try
        {
            forestRoot = new GameObject("Forests");
            forestRoot.transform.SetParent(parent, false);
            foliageRoot = new GameObject("Forest Foliage");
            foliageRoot.transform.SetParent(forestRoot.transform, false);
            woodRoot = new GameObject("Forest Wood");
            woodRoot.transform.SetParent(forestRoot.transform, false);
            canopyShadowRoot = new GameObject("Low-Poly Canopy Shadows");
            canopyShadowRoot.transform.SetParent(forestRoot.transform, false);

            CreateInitialLod2Tiles();
            forestRoot.SetActive(showForests);
            initialized = true;
        }
        catch
        {
            Dispose();
            throw;
        }
    }

    internal void SetVisible(bool visible)
    {
        forestRoot?.SetActive(visible);
    }

    internal void SetMeshEdgesVisible(bool visible)
    {
        if (meshEdgesVisible == visible)
        {
            return;
        }
        meshEdgesVisible = visible;
        foreach (var tile in lod2Tiles.Values)
        {
            SetTileMeshEdgesVisible(tile, visible);
        }
        SetGroupMeshEdgesVisible(lod1Groups, visible);
        SetGroupMeshEdgesVisible(lod0Groups, visible);
    }

    internal void UpdateLod1Neighborhood(Vector2Int center)
    {
        if (!initialized)
        {
            return;
        }

        var incoming = NeighbourKeys(center, Lod2Resolution);
        var incomingSet = new HashSet<Vector2Int>(incoming);
        var created = new List<Vector2Int>(incoming.Count);
        try
        {
            // Upload every incoming low-poly tile before hiding the overview
            // tile. This keeps a camera crossing from exposing a forest hole.
            foreach (var key in incoming)
            {
                if (!lod1Groups.ContainsKey(key))
                {
                    lod1Groups.Add(key, CreateLod1Group(key));
                    created.Add(key);
                }
            }
        }
        catch
        {
            foreach (var key in created)
            {
                if (lod1Groups.TryGetValue(key, out var group))
                {
                    DestroyGroup(group);
                    lod1Groups.Remove(key);
                }
            }
            throw;
        }

        foreach (var key in incoming)
        {
            SetLod2TileActive(key, false);
        }

        removalScratch.Clear();
        foreach (var key in lod1Groups.Keys)
        {
            if (!incomingSet.Contains(key))
            {
                removalScratch.Add(key);
            }
        }
        foreach (var key in removalScratch)
        {
            // A coarse transition can retire a low-poly parent while a
            // previous fine group is still active. Re-enable its low tile and
            // destroy that full-LOD group first so no tree is double-rendered.
            RemoveLod0GroupsOwnedBy(key);
            SetLod2TileActive(key, true);
            DestroyGroup(lod1Groups[key]);
            lod1Groups.Remove(key);
        }
    }

    internal void UpdateLod0Neighborhood(Vector2Int center)
    {
        if (!initialized)
        {
            return;
        }

        var incoming = NeighbourKeys(center, Lod1Resolution);
        var incomingSet = new HashSet<Vector2Int>(incoming);
        var created = new List<Vector2Int>(incoming.Count);
        try
        {
            // As with the coarse transition, prepare all full-LOD incoming
            // cells before disabling their low-poly owner tiles.
            foreach (var key in incoming)
            {
                if (!lod0Groups.ContainsKey(key))
                {
                    lod0Groups.Add(key, CreateLod0Group(key));
                    created.Add(key);
                }
            }
        }
        catch
        {
            foreach (var key in created)
            {
                if (lod0Groups.TryGetValue(key, out var group))
                {
                    DestroyGroup(group);
                    lod0Groups.Remove(key);
                }
            }
            throw;
        }

        foreach (var key in incoming)
        {
            SetLod1TileActive(key, false);
        }

        removalScratch.Clear();
        foreach (var key in lod0Groups.Keys)
        {
            if (!incomingSet.Contains(key))
            {
                removalScratch.Add(key);
            }
        }
        foreach (var key in removalScratch)
        {
            SetLod1TileActive(key, true);
            DestroyGroup(lod0Groups[key]);
            lod0Groups.Remove(key);
        }
    }

    internal void ClearPlayerFocus()
    {
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

        foreach (var tile in lod2Tiles.Values)
        {
            tile.SetActive(true);
        }
    }

    public void Dispose()
    {
        ClearPlayerFocus();
        foreach (var tile in lod2Tiles.Values)
        {
            DestroyTile(tile);
        }
        lod2Tiles.Clear();
        DestroyUnityObject(forestRoot);
        forestRoot = null;
        foliageRoot = null;
        woodRoot = null;
        canopyShadowRoot = null;
        foliageMaterial = null;
        lod0FoliageMaterial = null;
        woodMaterial = null;
        lod1WoodMaterial = null;
        meshEdgeMaterial = null;
        preparedLod2Foliage = null;
        preparedLod1Foliage = null;
        preparedLod1Wood = null;
        preparedLod0Foliage = null;
        preparedLod0Wood = null;
        initialized = false;
        meshEdgesVisible = false;
    }

    private void CreateInitialLod2Tiles()
    {
        for (var y = 0; y < Lod2Resolution; y++)
        {
            for (var x = 0; x < Lod2Resolution; x++)
            {
                var key = new Vector2Int(x, y);
                var prepared = preparedLod2Foliage[y * Lod2Resolution + x];
                if (prepared == null)
                {
                    continue;
                }
                var tile = CreateTile(
                    prepared,
                    null,
                    foliageMaterial,
                    null,
                    foliageRoot.transform,
                    woodRoot.transform,
                    $"Forest LOD 2 tile {x},{y}");
                lod2Tiles.Add(key, tile);
                CreateRendererObject(
                    $"Forest canopy shadow tile {x},{y}",
                    canopyShadowRoot.transform,
                    tile.foliageMesh,
                    foliageMaterial,
                    ShadowCastingMode.ShadowsOnly);
            }
        }
    }

    private Group CreateLod1Group(Vector2Int parent)
    {
        var group = CreateGroupRoots($"Forest LOD 1 group {parent.x},{parent.y}");
        try
        {
            for (var localY = 0; localY < Lod2TilePerLod1Region; localY++)
            {
                for (var localX = 0; localX < Lod2TilePerLod1Region; localX++)
                {
                    var key = new Vector2Int(
                        parent.x * Lod2TilePerLod1Region + localX,
                        parent.y * Lod2TilePerLod1Region + localY);
                    var index = key.y * Lod1Resolution + key.x;
                    var foliage = preparedLod1Foliage[index];
                    var wood = preparedLod1Wood[index];
                    if (foliage == null && wood == null)
                    {
                        continue;
                    }
                    group.tiles.Add(
                        key,
                        CreateTile(
                            foliage,
                            wood,
                            foliageMaterial,
                            lod1WoodMaterial,
                            group.foliageRoot.transform,
                            group.woodRoot.transform,
                            $"Forest LOD 1 tile {key.x},{key.y}"));
                }
            }
            return group;
        }
        catch
        {
            DestroyGroup(group);
            throw;
        }
    }

    private Group CreateLod0Group(Vector2Int key)
    {
        var group = CreateGroupRoots($"Forest LOD 0 group {key.x},{key.y}");
        try
        {
            var index = key.y * Lod1Resolution + key.x;
            var foliage = preparedLod0Foliage[index];
            var wood = preparedLod0Wood[index];
            if (foliage != null || wood != null)
            {
                group.tiles.Add(
                    key,
                    CreateTile(
                        foliage,
                        wood,
                        lod0FoliageMaterial,
                        woodMaterial,
                        group.foliageRoot.transform,
                        group.woodRoot.transform,
                        $"Forest LOD 0 tile {key.x},{key.y}"));
            }
            return group;
        }
        catch
        {
            DestroyGroup(group);
            throw;
        }
    }

    private Group CreateGroupRoots(string name)
    {
        var groupFoliageRoot = new GameObject($"{name} foliage");
        groupFoliageRoot.transform.SetParent(foliageRoot.transform, false);
        try
        {
            var groupWoodRoot = new GameObject($"{name} wood");
            groupWoodRoot.transform.SetParent(woodRoot.transform, false);
            return new Group(groupFoliageRoot, groupWoodRoot);
        }
        catch
        {
            DestroyUnityObject(groupFoliageRoot);
            throw;
        }
    }

    private Tile CreateTile(
        IslandPreparedMesh foliage,
        IslandPreparedMesh wood,
        Material tileFoliageMaterial,
        Material tileWoodMaterial,
        Transform foliageParent,
        Transform woodParent,
        string name)
    {
        Mesh foliageMesh = null;
        Mesh woodMesh = null;
        GameObject foliageObject = null;
        GameObject woodObject = null;
        Tile tile = null;
        try
        {
            if (foliage != null)
            {
                foliageMesh = CreateForestMesh(foliage, $"{name} foliage");
                ExpandFoliageBounds(foliageMesh, tileFoliageMaterial);
                ExpandWindBounds(foliageMesh, tileFoliageMaterial);
                foliageObject = CreateRendererObject(
                    $"{name} foliage",
                    foliageParent,
                    foliageMesh,
                    tileFoliageMaterial,
                    ShadowCastingMode.Off);
            }
            if (wood != null)
            {
                woodMesh = CreateForestMesh(wood, $"{name} wood");
                ExpandWindBounds(woodMesh, tileWoodMaterial);
                woodObject = CreateRendererObject(
                    $"{name} wood",
                    woodParent,
                    woodMesh,
                    tileWoodMaterial,
                    ShadowCastingMode.On);
            }
            tile = new Tile(foliageObject, foliageMesh, woodObject, woodMesh);
            SetTileMeshEdgesVisible(tile, meshEdgesVisible);
            return tile;
        }
        catch
        {
            if (tile != null)
            {
                DestroyTile(tile);
            }
            else
            {
                DestroyUnityObject(foliageObject);
                DestroyUnityObject(woodObject);
                DestroyUnityObject(foliageMesh);
                DestroyUnityObject(woodMesh);
            }
            throw;
        }
    }

    private static void ExpandFoliageBounds(Mesh mesh, Material material)
    {
        if (mesh == null
            || material == null
            || !material.HasProperty("_FoliageFurHeight"))
        {
            return;
        }
        var bounds = mesh.bounds;
        bounds.Expand(Mathf.Max(material.GetFloat("_FoliageFurHeight"), 0f) * 2f);
        mesh.bounds = bounds;
    }

    private static void ExpandWindBounds(Mesh mesh, Material material)
    {
        if (mesh == null
            || material == null
            || !material.HasProperty("_GrassWindStrength")
            || !material.HasProperty("_TreeWindStrengthMultiplier"))
        {
            return;
        }
        const float MaximumGrassWindStrength = 0.25f;
        var maximumOffset = Mathf.Max(
                material.GetFloat("_GrassWindStrength"),
                MaximumGrassWindStrength)
            * Mathf.Max(material.GetFloat("_TreeWindStrengthMultiplier"), 0f);
        if (maximumOffset <= 0f)
        {
            return;
        }
        var bounds = mesh.bounds;
        bounds.Expand(new Vector3(maximumOffset * 2f, 0f, maximumOffset * 2f));
        mesh.bounds = bounds;
    }

    private static GameObject CreateRendererObject(
        string name,
        Transform parent,
        Mesh mesh,
        Material material,
        ShadowCastingMode shadowCastingMode)
    {
        var gameObject = new GameObject(name);
        gameObject.transform.SetParent(parent, false);
        try
        {
            gameObject.AddComponent<MeshFilter>().sharedMesh = mesh;
            var renderer = gameObject.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = material;
            renderer.shadowCastingMode = shadowCastingMode;
            renderer.receiveShadows = shadowCastingMode != ShadowCastingMode.ShadowsOnly;
            if (shadowCastingMode == ShadowCastingMode.ShadowsOnly)
            {
                renderer.lightProbeUsage = LightProbeUsage.Off;
                renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
            }
            return gameObject;
        }
        catch
        {
            DestroyUnityObject(gameObject);
            throw;
        }
    }

    private static GameObject CreateEdgeRendererObject(
        string name,
        GameObject sourceObject,
        Mesh edgeMesh,
        Material material)
    {
        var gameObject = new GameObject(name);
        gameObject.transform.SetParent(sourceObject.transform, false);
        try
        {
            gameObject.AddComponent<MeshFilter>().sharedMesh = edgeMesh;
            var renderer = gameObject.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = material;
            renderer.shadowCastingMode = ShadowCastingMode.Off;
            renderer.receiveShadows = false;
            return gameObject;
        }
        catch
        {
            DestroyUnityObject(gameObject);
            throw;
        }
    }

#if UNITY_EDITOR
    internal static void ValidateLowPolyCanopyShadowProxy(
        Material material,
        Material lod0Material)
    {
        var parent = new GameObject("Canopy shadow proxy validation");
        var streamer = new ForestTileStreamer();
        try
        {
            var lowPolyCanopy = new IslandPreparedMesh(
                new[]
                {
                    Vector3.zero,
                    Vector3.right,
                    Vector3.forward,
                },
                new[]
                {
                    Vector3.up,
                    Vector3.up,
                    Vector3.up,
                },
                new[] { 0, 1, 2 },
                Array.Empty<Vector2>(),
                Array.Empty<Color>(),
                Array.Empty<Vector2>());
            var lod2 = new IslandPreparedMesh[Lod2TileCount];
            lod2[0] = lowPolyCanopy;
            var lod1 = new IslandPreparedMesh[Lod1TileCount];
            lod1[0] = lowPolyCanopy;
            var lod0 = new IslandPreparedMesh[Lod1TileCount];
            lod0[0] = lowPolyCanopy;
            var prepared = new IslandPreparedForestData(
                lod2,
                lod1,
                new IslandPreparedMesh[Lod1TileCount],
                lod0,
                new IslandPreparedMesh[Lod1TileCount]);
            streamer.Initialize(
                parent.transform,
                material,
                lod0Material,
                material,
                material,
                material,
                prepared,
                true);
            var renderers = streamer.Root.GetComponentsInChildren<MeshRenderer>(true);
            if (renderers.Length != 2
                || Array.FindAll(
                    renderers,
                    renderer => renderer.shadowCastingMode == ShadowCastingMode.Off).Length != 1
                || Array.FindAll(
                    renderers,
                    renderer => renderer.shadowCastingMode == ShadowCastingMode.ShadowsOnly).Length
                    != 1
                || renderers[0].GetComponent<MeshFilter>().sharedMesh
                    != renderers[1].GetComponent<MeshFilter>().sharedMesh)
            {
                throw new InvalidOperationException(
                    "The forest did not create one shared low-poly canopy shadow proxy.");
            }
            streamer.UpdateLod1Neighborhood(Vector2Int.zero);
            streamer.UpdateLod0Neighborhood(Vector2Int.zero);
            var lod0Renderer = Array.Find(
                streamer.Root.GetComponentsInChildren<MeshRenderer>(true),
                renderer => renderer.gameObject.name == "Forest LOD 0 tile 0,0 foliage");
            if (lod0Renderer == null || lod0Renderer.sharedMaterial != lod0Material)
            {
                throw new InvalidOperationException(
                    "The forest did not assign its double-sided material to LOD0 foliage.");
            }
        }
        finally
        {
            streamer.Dispose();
            DestroyUnityObject(parent);
        }
    }
#endif

    private void SetTileMeshEdgesVisible(Tile tile, bool visible)
    {
        if (tile == null)
        {
            return;
        }
        if (visible)
        {
            EnsureEdgeRenderer(
                tile.foliageObject,
                tile.foliageMesh,
                ref tile.foliageEdgeObject,
                ref tile.foliageEdgeMesh);
            EnsureEdgeRenderer(
                tile.woodObject,
                tile.woodMesh,
                ref tile.woodEdgeObject,
                ref tile.woodEdgeMesh);
        }
        tile.foliageEdgeObject?.SetActive(visible);
        tile.woodEdgeObject?.SetActive(visible);
    }

    private void EnsureEdgeRenderer(
        GameObject sourceObject,
        Mesh sourceMesh,
        ref GameObject edgeObject,
        ref Mesh edgeMesh)
    {
        if (sourceObject == null || sourceMesh == null || edgeObject != null)
        {
            return;
        }
        var createdMesh = TerrainTileStreamer.CreateEdgeMesh(sourceMesh);
        try
        {
            var createdObject = CreateEdgeRendererObject(
                $"{sourceObject.name} wireframe",
                sourceObject,
                createdMesh,
                meshEdgeMaterial);
            edgeMesh = createdMesh;
            edgeObject = createdObject;
        }
        catch
        {
            DestroyUnityObject(createdMesh);
            throw;
        }
    }

    private void SetGroupMeshEdgesVisible(
        Dictionary<Vector2Int, Group> groups,
        bool visible)
    {
        foreach (var group in groups.Values)
        {
            foreach (var tile in group.tiles.Values)
            {
                SetTileMeshEdgesVisible(tile, visible);
            }
        }
    }

    private static Mesh CreateForestMesh(IslandPreparedMesh source, string name)
    {
        var mesh = IslandGenerator.CreateGeneratedMesh(source);
        mesh.name = name;
        return mesh;
    }

    private void RemoveLod0GroupsOwnedBy(Vector2Int parent)
    {
        // UpdateLod1Neighborhood calls this while iterating removalScratch, so
        // child removal needs independent storage.
        childRemovalScratch.Clear();
        foreach (var key in lod0Groups.Keys)
        {
            if (key.x / Lod2TilePerLod1Region == parent.x
                && key.y / Lod2TilePerLod1Region == parent.y)
            {
                childRemovalScratch.Add(key);
            }
        }
        foreach (var key in childRemovalScratch)
        {
            SetLod1TileActive(key, true);
            DestroyGroup(lod0Groups[key]);
            lod0Groups.Remove(key);
        }
    }

    private void SetLod2TileActive(Vector2Int key, bool active)
    {
        if (lod2Tiles.TryGetValue(key, out var tile))
        {
            tile.SetActive(active);
        }
    }

    private void SetLod1TileActive(Vector2Int key, bool active)
    {
        var parent = new Vector2Int(
            key.x / Lod2TilePerLod1Region,
            key.y / Lod2TilePerLod1Region);
        if (lod1Groups.TryGetValue(parent, out var group)
            && group.tiles.TryGetValue(key, out var tile))
        {
            tile.SetActive(active);
        }
    }

    private static List<Vector2Int> NeighbourKeys(Vector2Int center, int resolution)
    {
        var result = new List<Vector2Int>(9);
        var minimumX = Mathf.Max(center.x - NearbyRadius, 0);
        var maximumX = Mathf.Min(center.x + NearbyRadius, resolution - 1);
        var minimumY = Mathf.Max(center.y - NearbyRadius, 0);
        var maximumY = Mathf.Min(center.y + NearbyRadius, resolution - 1);
        for (var y = minimumY; y <= maximumY; y++)
        {
            for (var x = minimumX; x <= maximumX; x++)
            {
                result.Add(new Vector2Int(x, y));
            }
        }
        return result;
    }

    private static void DestroyGroup(Group group)
    {
        if (group == null)
        {
            return;
        }
        foreach (var tile in group.tiles.Values)
        {
            DestroyTile(tile);
        }
        DestroyUnityObject(group.foliageRoot);
        DestroyUnityObject(group.woodRoot);
    }

    private static void DestroyTile(Tile tile)
    {
        if (tile == null)
        {
            return;
        }
        DestroyUnityObject(tile.foliageObject);
        DestroyUnityObject(tile.woodObject);
        DestroyUnityObject(tile.foliageEdgeMesh);
        DestroyUnityObject(tile.woodEdgeMesh);
        DestroyUnityObject(tile.foliageMesh);
        DestroyUnityObject(tile.woodMesh);
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            UnityEngine.Object.Destroy(value);
        }
        else
        {
            UnityEngine.Object.DestroyImmediate(value);
        }
    }
}
