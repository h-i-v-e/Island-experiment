using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;

// Fern rosettes are complete owner units in the same finest 64x64 grid as
// terrain LOD0. At most the player's 3x3 neighborhood is uploaded.
internal sealed class FernTileStreamer : IDisposable
{
    internal const int Resolution = 64;
    internal const int TileCount = Resolution * Resolution;
    private const int NearbyRadius = 1;

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

    private readonly Dictionary<Vector2Int, Tile> active =
        new Dictionary<Vector2Int, Tile>();
    private readonly List<Vector2Int> removal = new List<Vector2Int>(9);
    private IslandPreparedMesh[] prepared;
    private Material material;
    private GameObject root;

    internal int ActiveTileCount => active.Count;

    internal void Initialize(
        Transform parent,
        Material sharedMaterial,
        IslandPreparedMesh[] preparedTiles,
        bool showFerns)
    {
        if (parent == null) throw new ArgumentNullException(nameof(parent));
        material = sharedMaterial
            ?? throw new ArgumentNullException(nameof(sharedMaterial));
        if (preparedTiles == null || preparedTiles.Length != TileCount)
        {
            throw new ArgumentException(
                $"The fern owner grid must contain {TileCount} tiles.",
                nameof(preparedTiles));
        }
        prepared = preparedTiles;
        root = new GameObject("Tree Trunk Ferns (LOD 0 Only)");
        root.transform.SetParent(parent, false);
        root.SetActive(showFerns);
    }

    internal void SetVisible(bool value)
    {
        root?.SetActive(value);
    }

    internal void UpdateLod0Neighborhood(Vector2Int center)
    {
        if (root == null) return;
        var wanted = new HashSet<Vector2Int>();
        for (var y = center.y - NearbyRadius; y <= center.y + NearbyRadius; y++)
        {
            for (var x = center.x - NearbyRadius; x <= center.x + NearbyRadius; x++)
            {
                if (x < 0 || y < 0 || x >= Resolution || y >= Resolution) continue;
                var key = new Vector2Int(x, y);
                wanted.Add(key);
                if (!active.ContainsKey(key))
                {
                    var tile = CreateTile(key);
                    if (tile != null) active.Add(key, tile);
                }
            }
        }

        removal.Clear();
        foreach (var key in active.Keys)
        {
            if (!wanted.Contains(key)) removal.Add(key);
        }
        foreach (var key in removal)
        {
            DestroyTile(active[key]);
            active.Remove(key);
        }
    }

    internal void ClearPlayerFocus()
    {
        foreach (var tile in active.Values) DestroyTile(tile);
        active.Clear();
    }

    private Tile CreateTile(Vector2Int key)
    {
        var source = prepared[key.y * Resolution + key.x];
        if (source == null || source.triangles.Length == 0) return null;
        var mesh = IslandGenerator.CreateGeneratedMesh(source);
        mesh.name = $"Tree trunk ferns {key.x},{key.y}";
        var gameObject = new GameObject(mesh.name);
        gameObject.transform.SetParent(root.transform, false);
        var filter = gameObject.AddComponent<MeshFilter>();
        filter.sharedMesh = mesh;
        var renderer = gameObject.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = material;
        renderer.shadowCastingMode = ShadowCastingMode.On;
        renderer.receiveShadows = true;
        return new Tile(gameObject, mesh);
    }

    private static void DestroyTile(Tile tile)
    {
        if (tile == null) return;
        DestroyUnityObject(tile.gameObject);
        DestroyUnityObject(tile.mesh);
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null) return;
        if (Application.isPlaying) UnityEngine.Object.Destroy(value);
        else UnityEngine.Object.DestroyImmediate(value);
    }

    public void Dispose()
    {
        ClearPlayerFocus();
        DestroyUnityObject(root);
        root = null;
        prepared = null;
        material = null;
    }
}
