using System;
using System.Collections.Generic;
using UnityEngine;

[DefaultExecutionOrder(1100)]
[DisallowMultipleComponent]
internal sealed class OceanWaveMaskComposer : MonoBehaviour
{
    private sealed class CoastalBinding
    {
        internal IslandRuntime Owner;
        internal Texture Mask;
        internal Transform IslandTransform;
        internal float WorldSize;
    }

    private static readonly int SeaMaskId = Shader.PropertyToID("_SeaMask");
    private static readonly int IslandWorldToLocalId = Shader.PropertyToID(
        "_IslandWorldToLocal");
    private static readonly int IslandWorldSizeId = Shader.PropertyToID(
        "_IslandWorldSize");
    private static readonly int CompositionWorldRectId = Shader.PropertyToID(
        "_CompositionWorldRect");
    private static readonly int DepthAllowancePowerId = Shader.PropertyToID(
        "_DepthAllowancePower");
    private static readonly int DistanceAllowancePowerId = Shader.PropertyToID(
        "_DistanceAllowancePower");

    private readonly List<CoastalBinding> bindings = new List<CoastalBinding>();
    private OceanSurfaceController ocean;
    private OceanWaveRuntimeSettings settings;
    private Material compositionMaterial;
    private RenderTexture attenuationTexture;
    private Vector2 composedCentre;
    private bool dirty = true;
    private int compositionCount;
    private int lastOverlappingBindingCount;

    internal int CompositionCount => compositionCount;
    internal int BindingCount => bindings.Count;
    internal int LastOverlappingBindingCount => lastOverlappingBindingCount;
    internal RenderTexture AttenuationTexture => attenuationTexture;

    internal void Configure(
        OceanSurfaceController owner,
        OceanWaveRuntimeSettings waveSettings)
    {
        ocean = owner != null
            ? owner
            : throw new ArgumentNullException(nameof(owner));
        settings = waveSettings;
        EnsureResources();
        dirty = true;
    }

    internal void Register(
        IslandRuntime owner,
        Texture mask,
        Transform islandTransform,
        float worldSize)
    {
        if (owner == null) throw new ArgumentNullException(nameof(owner));
        if (mask == null) throw new ArgumentNullException(nameof(mask));
        if (islandTransform == null)
        {
            throw new ArgumentNullException(nameof(islandTransform));
        }
        for (var index = 0; index < bindings.Count; index++)
        {
            if (bindings[index].Owner != owner)
            {
                continue;
            }
            bindings[index].Mask = mask;
            bindings[index].IslandTransform = islandTransform;
            bindings[index].WorldSize = Mathf.Max(worldSize, 1f);
            dirty = true;
            return;
        }
        bindings.Add(new CoastalBinding
        {
            Owner = owner,
            Mask = mask,
            IslandTransform = islandTransform,
            WorldSize = Mathf.Max(worldSize, 1f),
        });
        dirty = true;
    }

    internal void Unregister(IslandRuntime owner)
    {
        if (owner == null)
        {
            return;
        }
        for (var index = bindings.Count - 1; index >= 0; index--)
        {
            if (bindings[index].Owner == owner)
            {
                bindings.RemoveAt(index);
                dirty = true;
            }
        }
    }

    private void LateUpdate()
    {
        if (ocean == null)
        {
            return;
        }
        var centre = new Vector2(transform.position.x, transform.position.z);
        if (centre != composedCentre)
        {
            dirty = true;
        }
        if (dirty)
        {
            Compose(centre);
        }
    }

    private void EnsureResources()
    {
        if (settings.MaskResolution < 1 || settings.MaskCoverageMetres <= 0f)
        {
            settings = OceanWaveRuntimeSettings.Default;
        }
        var shader = Shader.Find("Hidden/Motu/Ocean Wave Attenuation")
            ?? throw new InvalidOperationException(
                "Could not find shader 'Hidden/Motu/Ocean Wave Attenuation'.");
        if (compositionMaterial == null)
        {
            compositionMaterial = new Material(shader)
            {
                name = "Ocean Wave Attenuation Composer",
                hideFlags = HideFlags.HideAndDontSave,
            };
        }
        if (attenuationTexture != null
            && attenuationTexture.width == settings.MaskResolution
            && attenuationTexture.height == settings.MaskResolution)
        {
            return;
        }
        ReleaseTexture();
        var format = SelectFormat();
        attenuationTexture = new RenderTexture(
            settings.MaskResolution,
            settings.MaskResolution,
            0,
            format,
            RenderTextureReadWrite.Linear)
        {
            name = "Player-Centred Ocean Wave Attenuation",
            wrapMode = TextureWrapMode.Clamp,
            filterMode = FilterMode.Bilinear,
            useMipMap = false,
            autoGenerateMips = false,
            hideFlags = HideFlags.DontSave,
        };
        if (!attenuationTexture.Create())
        {
            var failedTexture = attenuationTexture;
            attenuationTexture = null;
            DestroyUnityObject(failedTexture);
            throw new InvalidOperationException(
                "Could not create the ocean wave attenuation texture.");
        }
    }

    private void Compose(Vector2 centre)
    {
        EnsureResources();
        var coverage = settings.MaskCoverageMetres;
        var minimum = centre - Vector2.one * (coverage * 0.5f);
        var compositionRect = new Vector4(minimum.x, minimum.y, coverage, coverage);
        var previous = RenderTexture.active;
        try
        {
            Graphics.SetRenderTarget(attenuationTexture);
            GL.Clear(false, true, Color.white);
            compositionMaterial.SetVector(
                CompositionWorldRectId,
                compositionRect);
            compositionMaterial.SetFloat(
                DepthAllowancePowerId,
                settings.DepthAllowancePower);
            compositionMaterial.SetFloat(
                DistanceAllowancePowerId,
                settings.DistanceAllowancePower);
            lastOverlappingBindingCount = 0;
            for (var index = bindings.Count - 1; index >= 0; index--)
            {
                var binding = bindings[index];
                if (binding.Owner == null
                    || binding.Mask == null
                    || binding.IslandTransform == null)
                {
                    bindings.RemoveAt(index);
                    continue;
                }
                if (!Overlaps(binding, minimum, coverage))
                {
                    continue;
                }
                compositionMaterial.SetTexture(SeaMaskId, binding.Mask);
                compositionMaterial.SetMatrix(
                    IslandWorldToLocalId,
                    binding.IslandTransform.worldToLocalMatrix);
                compositionMaterial.SetFloat(
                    IslandWorldSizeId,
                    binding.WorldSize);
                Graphics.Blit(
                    Texture2D.whiteTexture,
                    attenuationTexture,
                    compositionMaterial,
                    0);
                lastOverlappingBindingCount++;
            }
        }
        finally
        {
            RenderTexture.active = previous;
        }
        composedCentre = centre;
        dirty = false;
        compositionCount++;
        ocean.SetWaveAttenuation(
            attenuationTexture,
            new Vector4(
                minimum.x,
                minimum.y,
                1f / coverage,
                1f / coverage));
    }

    private static bool Overlaps(
        CoastalBinding binding,
        Vector2 compositionMinimum,
        float coverage)
    {
        var centre = new Vector2(
            binding.IslandTransform.position.x,
            binding.IslandTransform.position.z);
        var halfSize = binding.WorldSize * 0.75f;
        var compositionMaximum = compositionMinimum + Vector2.one * coverage;
        return centre.x + halfSize >= compositionMinimum.x
            && centre.y + halfSize >= compositionMinimum.y
            && centre.x - halfSize <= compositionMaximum.x
            && centre.y - halfSize <= compositionMaximum.y;
    }

    private static RenderTextureFormat SelectFormat()
    {
        if (SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.R8))
        {
            return RenderTextureFormat.R8;
        }
        if (SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.RHalf))
        {
            return RenderTextureFormat.RHalf;
        }
        return RenderTextureFormat.ARGB32;
    }

    private void OnDisable()
    {
        ocean?.SetWaveAttenuation(
            Texture2D.whiteTexture,
            new Vector4(-1f, -1f, 0.5f, 0.5f));
    }

    private void OnDestroy()
    {
        ReleaseTexture();
        DestroyUnityObject(compositionMaterial);
        compositionMaterial = null;
        bindings.Clear();
    }

    private void ReleaseTexture()
    {
        if (attenuationTexture == null)
        {
            return;
        }
        attenuationTexture.Release();
        DestroyUnityObject(attenuationTexture);
        attenuationTexture = null;
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null) return;
        if (Application.isPlaying) Destroy(value);
        else DestroyImmediate(value);
    }
}
