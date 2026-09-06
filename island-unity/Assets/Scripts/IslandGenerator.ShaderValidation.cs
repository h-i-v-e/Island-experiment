#if UNITY_EDITOR
using System;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class IslandGenerator
{
    public static void ValidateMaterialTextureCacheRoundTrip()
    {
        IslandMaterialTextureCache.ValidateRoundTrip();
        ValidateRuntimeTreeBarkMaterialBinding();
    }

    private static void ValidateTreeSurfaceShader(string shaderName, string label)
    {
        var shader = Shader.Find(shaderName);
        if (shader == null
            || !shader.isSupported
            || UnityEditor.ShaderUtil.ShaderHasError(shader))
        {
            throw new InvalidOperationException(
                $"The tree {label} shader is missing or unsupported.");
        }
        var material = new Material(shader);
        Material lod0Material = null;
        var noise = CreateCliffNoiseTexture();
        try
        {
            if (!material.HasProperty("_BaseColor")
                || !material.HasProperty("_LightColor")
                || !material.HasProperty("_CliffNoise3D")
                || !material.HasProperty("_TreeNoisePeriod")
                || !material.HasProperty("_TreeNoiseDetailScale")
                || !material.HasProperty("_TreeNoiseFineScale")
                || !material.HasProperty("_TreeNormalStrength")
                || !material.HasProperty("_TreeHueVariationDegrees")
                || !material.HasProperty("_WorldSize"))
            {
                throw new InvalidOperationException(
                    $"The tree {label} shader is missing its layered-noise properties.");
            }
            if (label == "wood"
                && (!material.HasProperty("_BarkAlbedoMap")
                    || !material.HasProperty("_BarkHeightMap")
                    || !material.HasProperty("_BarkNormalMap")
                    || !material.HasProperty("_BarkOcclusionMap")
                    || !material.HasProperty("_BarkTileWidthMetres")
                    || !material.HasProperty("_BarkTileHeightMetres")
                    || !material.HasProperty("_BarkNormalMapStrength")
                    || !material.HasProperty("_BarkParallaxStrengthMetres")
                    || !material.HasProperty("_BarkOcclusionStrength")
                    || !material.HasProperty("_BarkAmbientFloor")
                    || !material.HasProperty("_WorldSize")))
            {
                throw new InvalidOperationException(
                    "The tree wood shader is missing its directional bark properties.");
            }
            if (label == "foliage"
                && (!material.HasProperty("_CanopyCoverage")
                    || !material.HasProperty("_CanopyEdgeSoftness")
                    || !material.HasProperty("_AlphaCutoff")
                    || !material.HasProperty("_FoliageFurHeight")
                    || !material.HasProperty("_FoliageLeafWorldSize")
                    || !material.HasProperty("_FoliageLeafCoverage")
                    || !material.HasProperty("_FoliageLeafEdgeSoftness")
                    || !material.HasProperty("_TranslucencyColor")
                    || !material.HasProperty("_FoliageTranslucency")
                    || !material.HasProperty("_FoliageAmbientFloor")
                    || !material.HasProperty("_CullMode")
                    || !material.HasProperty("_GrassPlayerPosition")
                    || !material.HasProperty("_GrassRadius")
                    || !material.HasProperty("_GrassFadeWidth")))
            {
                throw new InvalidOperationException(
                    "The tree foliage shader is missing its canopy or fur properties.");
            }
            if (label == "foliage" && material.passCount != 10)
            {
                throw new InvalidOperationException(
                    "The tree foliage shader must contain one canopy pass, eight fur passes, "
                    + "and one shadow pass.");
            }
            if (label == "foliage"
                && (material.renderQueue != (int)RenderQueue.AlphaTest
                    || material.FindPass("ShadowCaster") < 0))
            {
                throw new InvalidOperationException(
                    "The tree foliage shader must expose an alpha-tested shadow caster.");
            }
            if (label == "foliage")
            {
                material.SetFloat("_CullMode", (float)CullMode.Back);
                lod0Material = new Material(material);
                lod0Material.SetFloat("_CullMode", (float)CullMode.Off);
                ForestTileStreamer.ValidateLowPolyCanopyShadowProxy(
                    material,
                    lod0Material);
            }
            material.SetTexture("_CliffNoise3D", noise);
            if (material.GetTexture("_CliffNoise3D") != noise)
            {
                throw new InvalidOperationException(
                    $"The tree {label} shader did not retain its 3D noise texture.");
            }
        }
        finally
        {
            DestroyImmediate(noise);
            DestroyImmediate(lod0Material);
            DestroyImmediate(material);
        }
    }

    private static void ValidateRuntimeTreeBarkMaterialBinding()
    {
        const int resolution = 16;
        var prepared = IslandPreparationPipeline.PrepareMaterialTextures(
            new IslandMaterialColours(
                new Color(0.09f, 0.055f, 0.026f),
                new Color(0.30f, 0.32f, 0.29f),
                new Color(0.62f, 0.57f, 0.34f)),
            resolution);
        var shader = Shader.Find("Motu/Tree Wood")
            ?? throw new InvalidOperationException("Could not find shader 'Motu/Tree Wood'.");
        var host = new GameObject("Runtime tree bark validation");
        host.SetActive(false);
        var generator = host.AddComponent<IslandGenerator>();
        generator.treeWoodMaterial = new Material(shader);
        try
        {
            generator.CreateTreeBarkTextures(prepared.treeBark);
            var material = generator.treeWoodMaterial;
            foreach (var property in new[]
            {
                "_BarkAlbedoMap",
                "_BarkHeightMap",
                "_BarkNormalMap",
                "_BarkOcclusionMap",
            })
            {
                var texture = material.GetTexture(property) as Texture2D;
                if (texture == null
                    || texture.width != resolution
                    || texture.height != resolution
                    || texture.wrapMode != TextureWrapMode.Repeat)
                {
                    throw new InvalidOperationException(
                        $"Runtime tree bark did not bind a valid {property} texture.");
                }
            }
            if (!Mathf.Approximately(
                    material.GetFloat("_BarkTileWidthMetres"),
                    prepared.treeBark.physicalTileWidthMetres)
                || !Mathf.Approximately(
                    material.GetFloat("_BarkTileHeightMetres"),
                    prepared.treeBark.physicalTileHeightMetres))
            {
                throw new InvalidOperationException(
                    "Runtime tree bark did not preserve the recipe's physical tile dimensions.");
            }
        }
        finally
        {
            generator.DestroyRuntimeMaterials();
            DestroyImmediate(host);
        }
    }

    private static void ValidateDistantFoliageShader()
    {
        var shader = Shader.Find("Motu/Tree Foliage Distant");
        if (shader == null
            || !shader.isSupported
            || UnityEditor.ShaderUtil.ShaderHasError(shader))
        {
            throw new InvalidOperationException(
                "The distant tree foliage shader is missing or unsupported.");
        }
        var material = new Material(shader);
        try
        {
            if (material.passCount != 2
                || material.FindPass("ShadowCaster") < 0
                || material.renderQueue != (int)RenderQueue.Geometry
                || !material.HasProperty("_WorldSize")
                || !material.HasProperty("_MotuNightStrength"))
            {
                throw new InvalidOperationException(
                    "Distant foliage is missing its base, wind, or shadow contract.");
            }
        }
        finally
        {
            DestroyImmediate(material);
        }
    }
}
#endif
