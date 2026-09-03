using System;
using System.Collections.Generic;
using UnityEngine;

internal enum IslandRuntimeState
{
    Installing,
    Active,
    Dormant,
    Failed,
    Unloading,
    Disposed,
}

public sealed class IslandRuntime : MonoBehaviour, IDisposable
{
    private readonly List<Material> ownedMaterials = new List<Material>();
    private readonly List<Texture> ownedTextures = new List<Texture>();
    private NativeIslandHandle nativeHandle;
    private TerrainMaterialTextureArrays terrainTextureArrays;
    private Material textureArrayTerrainMaterial;
    private Material textureArrayGrassMaterial;
    private WorldEnvironmentController coastalWaveEnvironment;
    private Texture coastalWaveMask;
    private float coastalWaveWorldSize;
    private bool coastalWaveMaskRegistered;
    private bool resourcesReleased;

    internal IslandDescriptor Descriptor { get; private set; }
    internal IslandRuntimeState State { get; private set; }
    internal TerrainTileStreamer TerrainStreamer { get; private set; }
    internal GameObject CoastalWaterObject { get; private set; }
    internal NativeIslandHandle NativeHandle => nativeHandle;

    internal static IslandRuntime Create(
        IslandDescriptor descriptor,
        Transform parent)
    {
        if (parent == null) throw new ArgumentNullException(nameof(parent));
        var root = new GameObject($"Island Runtime [{descriptor.IslandId}]");
        root.transform.SetParent(parent, false);
        root.SetActive(false);
        var runtime = root.AddComponent<IslandRuntime>();
        runtime.Descriptor = descriptor;
        runtime.State = IslandRuntimeState.Installing;
        return runtime;
    }

    internal void AdoptNativeHandle(NativeIslandHandle value)
    {
        RequireInstalling();
        if (value == null || !value.IsValid)
        {
            throw new ArgumentException("An island runtime requires a valid native handle.", nameof(value));
        }
        if (nativeHandle != null)
        {
            throw new InvalidOperationException("The island runtime already owns a native handle.");
        }
        nativeHandle = value;
    }

    internal void OwnMaterial(Material value)
    {
        RequireInstalling();
        if (value != null && !ownedMaterials.Contains(value))
        {
            ownedMaterials.Add(value);
        }
    }

    internal void OwnTexture(Texture value)
    {
        RequireInstalling();
        if (value != null && !ownedTextures.Contains(value))
        {
            ownedTextures.Add(value);
        }
    }

    internal void OwnTerrainTextureArrays(
        TerrainMaterialTextureArrays value,
        Material terrainMaterial,
        Material grassMaterial)
    {
        RequireInstalling();
        if (terrainTextureArrays != null)
        {
            throw new InvalidOperationException(
                "The island runtime already owns terrain texture arrays.");
        }
        terrainTextureArrays = value
            ?? throw new ArgumentNullException(nameof(value));
        textureArrayTerrainMaterial = terrainMaterial;
        textureArrayGrassMaterial = grassMaterial;
    }

    internal void SetTerrainStreamer(TerrainTileStreamer value)
    {
        RequireInstalling();
        TerrainStreamer = value
            ?? throw new ArgumentNullException(nameof(value));
        if (!value.transform.IsChildOf(transform))
        {
            throw new InvalidOperationException(
                "The terrain streamer must be installed below its island runtime.");
        }
    }

    internal void SetCoastalWaterObject(GameObject value)
    {
        RequireInstalling();
        CoastalWaterObject = value
            ?? throw new ArgumentNullException(nameof(value));
        if (!value.transform.IsChildOf(transform))
        {
            throw new InvalidOperationException(
                "The coastal overlay must be installed below its island runtime.");
        }
    }

    internal void SetCoastalWaveMask(
        WorldEnvironmentController environment,
        Texture mask,
        float worldSize)
    {
        RequireInstalling();
        coastalWaveEnvironment = environment
            ?? throw new ArgumentNullException(nameof(environment));
        coastalWaveMask = mask
            ?? throw new ArgumentNullException(nameof(mask));
        coastalWaveWorldSize = Mathf.Max(worldSize, 1f);
    }

    internal void Activate()
    {
        RequireInstalling();
        if (nativeHandle == null
            || TerrainStreamer == null
            || CoastalWaterObject == null
            || terrainTextureArrays == null
            || ownedMaterials.Count == 0)
        {
            throw new InvalidOperationException(
                "The island runtime cannot activate before required resources are installed.");
        }
        gameObject.SetActive(true);
        State = IslandRuntimeState.Active;
        RegisterCoastalWaveMask();
    }

    internal void SetDormant(bool dormant)
    {
        if (State != IslandRuntimeState.Active && State != IslandRuntimeState.Dormant)
        {
            throw new InvalidOperationException(
                "Only an active island runtime can change dormancy.");
        }
        if (dormant)
        {
            UnregisterCoastalWaveMask();
        }
        State = dormant ? IslandRuntimeState.Dormant : IslandRuntimeState.Active;
        gameObject.SetActive(!dormant);
        if (!dormant)
        {
            RegisterCoastalWaveMask();
        }
    }

    internal void MarkFailed()
    {
        if (State != IslandRuntimeState.Disposed)
        {
            State = IslandRuntimeState.Failed;
        }
    }

    public void Dispose()
    {
        ReleaseResources(true);
    }

    private void OnDestroy()
    {
        ReleaseResources(false);
    }

    private void ReleaseResources(bool destroyRoot)
    {
        if (resourcesReleased)
        {
            return;
        }
        resourcesReleased = true;
        State = IslandRuntimeState.Unloading;
        UnregisterCoastalWaveMask();
        if (destroyRoot)
        {
            gameObject.SetActive(false);
        }

        TerrainStreamer?.Dispose();
        TerrainStreamer = null;
        CoastalWaterObject = null;
        coastalWaveEnvironment = null;
        coastalWaveMask = null;
        coastalWaveWorldSize = 0f;

        terrainTextureArrays?.Unbind(
            textureArrayTerrainMaterial,
            textureArrayGrassMaterial);
        terrainTextureArrays?.Dispose();
        terrainTextureArrays = null;
        textureArrayTerrainMaterial = null;
        textureArrayGrassMaterial = null;

        nativeHandle?.Dispose();
        nativeHandle = null;

        foreach (var material in ownedMaterials)
        {
            DestroyUnityObject(material);
        }
        ownedMaterials.Clear();
        foreach (var texture in ownedTextures)
        {
            DestroyUnityObject(texture);
        }
        ownedTextures.Clear();

        State = IslandRuntimeState.Disposed;
        if (destroyRoot)
        {
            DestroyUnityObject(gameObject);
        }
    }

    private void RequireInstalling()
    {
        if (State != IslandRuntimeState.Installing || resourcesReleased)
        {
            throw new InvalidOperationException(
                "Island resources can only be installed during the installing state.");
        }
    }

    private void RegisterCoastalWaveMask()
    {
        if (coastalWaveMaskRegistered
            || coastalWaveEnvironment == null
            || coastalWaveMask == null)
        {
            return;
        }
        coastalWaveEnvironment.RegisterCoastalWaveMask(
            this,
            coastalWaveMask,
            transform,
            coastalWaveWorldSize);
        coastalWaveMaskRegistered = true;
    }

    private void UnregisterCoastalWaveMask()
    {
        if (!coastalWaveMaskRegistered)
        {
            return;
        }
        coastalWaveEnvironment?.UnregisterCoastalWaveMask(this);
        coastalWaveMaskRegistered = false;
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null) return;
        if (Application.isPlaying) Destroy(value);
        else DestroyImmediate(value);
    }

#if UNITY_EDITOR
    internal static void ValidateOwnershipContract()
    {
        var firstParent = new GameObject("First island runtime validation parent");
        var secondParent = new GameObject("Second island runtime validation parent");
        firstParent.transform.position = new Vector3(1200f, 0f, -600f);
        secondParent.transform.position = new Vector3(-900f, 0f, 1700f);
        var first = Create(
            new IslandDescriptor(
                "validation-a",
                Vector2Int.zero,
                1200d,
                -600d,
                11,
                0f,
                1000f,
                1),
            firstParent.transform);
        var second = Create(
            new IslandDescriptor(
                "validation-b",
                Vector2Int.one,
                -900d,
                1700d,
                12,
                0f,
                1000f,
                1),
            secondParent.transform);
        var shader = Shader.Find("Motu/Coastal Water Overlay")
            ?? throw new InvalidOperationException(
                "The coastal shader is unavailable for island-runtime validation.");
        var firstMaterial = new Material(shader);
        var secondMaterial = new Material(shader);
        var firstMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        var secondMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        var firstRoot = first.gameObject;
        var secondRoot = second.gameObject;
        try
        {
            firstMaterial.SetMatrix("_IslandWorldToLocal", first.transform.worldToLocalMatrix);
            secondMaterial.SetMatrix("_IslandWorldToLocal", second.transform.worldToLocalMatrix);
            firstMaterial.SetTexture("_SeaMask", firstMask);
            secondMaterial.SetTexture("_SeaMask", secondMask);
            first.OwnMaterial(firstMaterial);
            second.OwnMaterial(secondMaterial);
            first.OwnTexture(firstMask);
            second.OwnTexture(secondMask);
            if (first.transform.position == second.transform.position
                || firstMaterial.GetMatrix("_IslandWorldToLocal")
                    == secondMaterial.GetMatrix("_IslandWorldToLocal")
                || firstMaterial.GetTexture("_SeaMask")
                    == secondMaterial.GetTexture("_SeaMask"))
            {
                throw new InvalidOperationException(
                    "Two island runtimes contaminated their transforms or coast masks.");
            }
        }
        finally
        {
            first.Dispose();
            second.Dispose();
            DestroyUnityObject(firstParent);
            DestroyUnityObject(secondParent);
        }
        if (firstRoot != null
            || secondRoot != null
            || firstMaterial != null
            || secondMaterial != null
            || firstMask != null
            || secondMask != null)
        {
            throw new InvalidOperationException(
                "Island runtime disposal did not release its owned Unity objects.");
        }
    }
#endif
}
