using Motu.Interop;
using static Motu.Rendering.ProceduralNoiseTextures;
using static UnityEngine.Object;
using System;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Streaming;

namespace Motu.Editor
{
    public static class IslandRenderingValidation
    {
        public static void ValidateMaterialTextureCacheRoundTrip()
        {
            MaterialCacheValidation.ValidateRoundTrip();
            ValidateRuntimeTreeBarkMaterialBinding();
        }

        internal static void ValidateFallenLogSurfaceShader()
        {
            var shader = Shader.Find("Motu/Tree Wood");
            if (shader == null || !shader.isSupported || UnityEditor.ShaderUtil.ShaderHasError(shader))
                throw new InvalidOperationException("The fallen-log wood shader is missing or unsupported.");
            var material = new Material(shader);
            var noise = CreateCliffNoiseTexture();
            try
            {
                foreach (var property in new[]
                {
                    "_BaseColor", "_LightColor", "_CliffNoise3D", "_TreeNoisePeriod", "_TreeNoiseDetailScale",
                    "_TreeNoiseFineScale", "_TreeNormalStrength", "_TreeHueVariationDegrees", "_WorldSize",
                    "_BarkAlbedoMap", "_BarkHeightMap", "_BarkNormalMap", "_BarkOcclusionMap",
                    "_BarkTileWidthMetres", "_BarkTileHeightMetres", "_BarkNormalMapStrength",
                    "_BarkParallaxStrengthMetres", "_BarkOcclusionStrength", "_BarkAmbientFloor",
                    "_EndGrainMap", "_EndGrainColor", "_GroundDirtColor",
                })
                    if (!material.HasProperty(property))
                        throw new InvalidOperationException($"The fallen-log shader is missing its authored {property} surface detail.");
                material.SetTexture("_CliffNoise3D", noise);
                if (material.GetTexture("_CliffNoise3D") != noise)
                    throw new InvalidOperationException("The fallen-log shader did not retain its shared 3D detail texture.");
            }
            finally { DestroyImmediate(noise); DestroyImmediate(material); }
        }

        internal static void ValidateSharedForestShader()
        {
            var shader = Shader.Find("Motu/Planet Forest");
            if (shader == null || !shader.isSupported || UnityEditor.ShaderUtil.ShaderHasError(shader))
                throw new InvalidOperationException("The shared standing-tree shader is missing or unsupported.");
            var material = new Material(shader) { enableInstancing = true };
            var noise = CreateCliffNoiseTexture();
            try
            {
                foreach (var pass in new[] { "Forward", "ShadowCaster", "DepthOnly", "Reflection" })
                {
                    var index = material.FindPass(pass);
                    if (index < 0) throw new InvalidOperationException($"Shared standing trees are missing the {pass} pass.");
                    UnityEditor.ShaderUtil.CompilePass(material, index, true);
                }
                if (UnityEditor.ShaderUtil.ShaderHasError(shader) || material.renderQueue != (int)RenderQueue.AlphaTest)
                    throw new InvalidOperationException("The instanced standing-tree passes must compile with alpha-tested silhouettes.");
                foreach (var property in new[]
                {
                    "_ForestKind", "_Cull", "_ForestNoise", "_TwigCard", "_PalmFrond",
                    "_ForestBarkAlbedoHeight", "_ForestBarkNormalOcclusion", "_ForestBarkTileSizes", "_ForestPalmBarkTileSize",
                })
                    if (!material.HasProperty(property))
                        throw new InvalidOperationException($"Shared standing trees are missing the {property} species surface contract.");
                material.SetTexture("_ForestNoise", noise);
                if (material.GetTexture("_ForestNoise") != noise)
                    throw new InvalidOperationException("Shared standing trees did not retain their detail noise texture.");
                ForestRenderingValidation.ValidateSharedTreeCatalogAndCollision();
                ValidateRuntimeTreeBarkMaterialBinding();
            }
            finally { DestroyImmediate(noise); DestroyImmediate(material); }
        }

        private static void ValidateRuntimeTreeBarkMaterialBinding()
        {
            PlanetForestModels models = null;
            try
            {
                models = PlanetForestModels.Acquire(ForestRenderingValidation.SharedCatalogFixture(), null);
                var material = models.Material(0);
                var sources = HybridForestNative.SharedBarkMaps;
                foreach (var property in new[] { "_ForestBarkAlbedoHeight", "_ForestBarkNormalOcclusion" })
                {
                    var texture = material.GetTexture(property) as Texture2DArray;
                    if (texture == null || texture.depth != 3 || texture.width != sources[0].width
                        || texture.height != sources[0].height || texture.wrapMode != TextureWrapMode.Repeat
                        || models.Material(5).GetTexture(property) != texture)
                        throw new InvalidOperationException($"Shared standing trees did not retain the three-species {property} array.");
                }
                var sizes = material.GetVector("_ForestBarkTileSizes");
                var palm = material.GetVector("_ForestPalmBarkTileSize");
                if (!Mathf.Approximately(sizes.x, sources[0].physicalTileWidthMetres)
                    || !Mathf.Approximately(sizes.y, sources[0].physicalTileHeightMetres)
                    || !Mathf.Approximately(sizes.z, sources[1].physicalTileWidthMetres)
                    || !Mathf.Approximately(sizes.w, sources[1].physicalTileHeightMetres)
                    || !Mathf.Approximately(palm.x, sources[2].physicalTileWidthMetres)
                    || !Mathf.Approximately(palm.y, sources[2].physicalTileHeightMetres))
                    throw new InvalidOperationException("Standing-tree bark must preserve each species recipe's physical tile dimensions.");
            }
            finally { models?.Release(); }
        }

    }
}
