#if UNITY_EDITOR
using System;
using UnityEngine;
using UnityEngine.Rendering;

public sealed partial class IslandGenerator
{
    public static void ValidateMaterialTextureCacheRoundTrip()
    {
        IslandMaterialTextureCache.ValidateRoundTrip();
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
                || !material.HasProperty("_WorldSize")
                || !material.HasProperty("_GrassPatchNoise")
                || !material.HasProperty("_GrassWindDirection")
                || !material.HasProperty("_GrassWindStrength")
                || !material.HasProperty("_GrassWindSpeed")
                || !material.HasProperty("_GrassWindWorldSize")
                || !material.HasProperty("_TreeWindStrengthMultiplier")
                || !material.HasProperty("_TreeWindBasePinHeight")
                || !material.HasProperty("_TreeWindFullBendHeight"))
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
            if (label == "wood")
            {
                var authoredMaterial = UnityEditor.AssetDatabase.LoadAssetAtPath<Material>(
                    "Assets/Materials/TreeWood.mat");
                var expectedAlbedo = UnityEditor.AssetDatabase.LoadAssetAtPath<Texture2D>(
                    "Assets/Generated/Textures/PlateBark/PlateBark_albedo.png");
                var expectedHeight = UnityEditor.AssetDatabase.LoadAssetAtPath<Texture2D>(
                    "Assets/Generated/Textures/PlateBark/PlateBark_height.png");
                var expectedNormal = UnityEditor.AssetDatabase.LoadAssetAtPath<Texture2D>(
                    "Assets/Generated/Textures/PlateBark/PlateBark_normal.png");
                var expectedOcclusion = UnityEditor.AssetDatabase.LoadAssetAtPath<Texture2D>(
                    "Assets/Generated/Textures/PlateBark/PlateBark_occlusion.png");
                var normalImporter = UnityEditor.AssetImporter.GetAtPath(
                    "Assets/Generated/Textures/PlateBark/PlateBark_normal.png")
                    as UnityEditor.TextureImporter;
                if (authoredMaterial == null
                    || authoredMaterial.shader != shader
                    || authoredMaterial.GetTexture("_BarkAlbedoMap") != expectedAlbedo
                    || authoredMaterial.GetTexture("_BarkHeightMap") != expectedHeight
                    || authoredMaterial.GetTexture("_BarkNormalMap") != expectedNormal
                    || authoredMaterial.GetTexture("_BarkOcclusionMap") != expectedOcclusion
                    || !Mathf.Approximately(
                        authoredMaterial.GetFloat("_BarkTileWidthMetres"),
                        1.2f)
                    || !Mathf.Approximately(
                        authoredMaterial.GetFloat("_BarkTileHeightMetres"),
                        1.6f)
                    || !Mathf.Approximately(
                        authoredMaterial.GetFloat("_BarkParallaxStrengthMetres"),
                        0.05f)
                    || normalImporter == null
                    || normalImporter.textureType != UnityEditor.TextureImporterType.NormalMap
                    || normalImporter.flipGreenChannel
                    || normalImporter.wrapMode != TextureWrapMode.Repeat)
                {
                    throw new InvalidOperationException(
                        "The tree wood material is not using the imported Plate Bark recipe maps.");
                }
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
                || !material.HasProperty("_GrassPatchNoise")
                || !material.HasProperty("_GrassWindDirection")
                || !material.HasProperty("_GrassWindStrength")
                || !material.HasProperty("_GrassWindSpeed")
                || !material.HasProperty("_GrassWindWorldSize")
                || !material.HasProperty("_TreeWindStrengthMultiplier")
                || !material.HasProperty("_TreeWindBasePinHeight")
                || !material.HasProperty("_TreeWindFullBendHeight")
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
