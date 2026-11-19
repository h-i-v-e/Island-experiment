using System;
using UnityEngine;
using Motu.Islands;

namespace Motu.Rendering
{
    internal sealed class TerrainMaterialTextureArrays : IDisposable
    {
        internal const int DirtLayer = 0;
        internal const int ForestFloorLayer = 1;
        internal const int RockLayer = 2;
        internal const int RiverBedLayer = 3;
        internal const int BeachLayer = 4;
        internal const int FallenStonesLayer = 5;
        internal const int RoadLayer = 6;
        internal const int LayerCount = RoadLayer + 1;

        private static readonly int AlbedoArrayId = Shader.PropertyToID("_TerrainAlbedoArray");
        private static readonly int NormalArrayId = Shader.PropertyToID("_TerrainNormalArray");
        private static readonly int MaskArrayId = Shader.PropertyToID("_TerrainMaskArray");
        private static readonly int PackedRoadTileMetresId = Shader.PropertyToID("_PackedRoadTileMetres");
        private static readonly int RoadHeightRangeId = Shader.PropertyToID("_RoadHeightRange");
        private static readonly int ParallaxNeutralHeightsAId =
            Shader.PropertyToID("_TerrainParallaxNeutralHeightsA");
        private static readonly int ParallaxNeutralHeightsBId =
            Shader.PropertyToID("_TerrainParallaxNeutralHeightsB");

        private Texture2DArray albedo;
        private Texture2DArray normal;
        private Texture2DArray mask;
        private readonly Vector4 parallaxNeutralHeightsA;
        private readonly Vector4 parallaxNeutralHeightsB;
        private readonly Vector4 roadTileMetres;
        private readonly float roadHeightRange;

        internal TerrainMaterialTextureArrays(IslandPreparedMaterialTextures textures)
        {
            if (textures == null) throw new ArgumentNullException(nameof(textures));
            var layers = new[]
            {
                textures.dirt,
                textures.forestFloor,
                textures.rock,
                textures.riverBed,
                textures.beach,
                textures.fallenStones,
                textures.road,
            };
            var width = layers[0].width;
            var height = layers[0].height;
            foreach (var layer in layers)
            {
                if (layer.width != width || layer.height != height)
                {
                    throw new InvalidOperationException(
                        "Runtime terrain material layers must have identical dimensions.");
                }
            }
            parallaxNeutralHeightsA = new Vector4(
                layers[DirtLayer].NormalizedBaseHeight,
                layers[ForestFloorLayer].NormalizedBaseHeight,
                layers[RockLayer].NormalizedBaseHeight,
                layers[RiverBedLayer].NormalizedBaseHeight);
            parallaxNeutralHeightsB = new Vector4(
                layers[BeachLayer].NormalizedBaseHeight,
                layers[FallenStonesLayer].NormalizedBaseHeight,
                layers[RoadLayer].NormalizedBaseHeight,
                0f);
            roadTileMetres = new Vector4(textures.road.physicalTileWidthMetres, textures.road.physicalTileHeightMetres, 0f, 0f);
            roadHeightRange = textures.road.maximumHeight - textures.road.minimumHeight;

            albedo = CreateArray(
                "Island terrain albedo array",
                width,
                height,
                TextureFormat.RGBA32,
                false);
            normal = CreateArray(
                "Island terrain normal array",
                width,
                height,
                TextureFormat.RGBA32,
                true);
            mask = CreateArray(
                "Island terrain height and occlusion array",
                width,
                height,
                TextureFormat.RG16,
                true);

            try
            {
                for (var layerIndex = 0; layerIndex < layers.Length; layerIndex++)
                {
                    var layer = layers[layerIndex];
                    albedo.SetPixelData(ExpandRgb(layer.albedoRgb), 0, layerIndex);
                    normal.SetPixelData(ExpandRgb(layer.normalRgb), 0, layerIndex);
                    mask.SetPixelData(PackHeightAndOcclusion(layer), 0, layerIndex);
                }
                albedo.Apply(true, true);
                normal.Apply(true, true);
                mask.Apply(true, true);
            }
            catch
            {
                Dispose();
                throw;
            }
        }

        internal void BindTerrain(Material material)
        {
            RequireProperties(
                material,
                AlbedoArrayId,
                NormalArrayId,
                MaskArrayId,
                ParallaxNeutralHeightsAId,
                ParallaxNeutralHeightsBId);
            material.SetTexture(AlbedoArrayId, albedo);
            material.SetTexture(NormalArrayId, normal);
            material.SetTexture(MaskArrayId, mask);
            if (material.HasProperty(PackedRoadTileMetresId))
                material.SetVector(PackedRoadTileMetresId, roadTileMetres);
            if (material.HasProperty(RoadHeightRangeId))
                material.SetFloat(RoadHeightRangeId, roadHeightRange);
            material.SetVector(ParallaxNeutralHeightsAId, parallaxNeutralHeightsA);
            material.SetVector(ParallaxNeutralHeightsBId, parallaxNeutralHeightsB);
        }

        internal void BindGrass(Material material)
        {
            RequireProperties(material, MaskArrayId);
            material.SetTexture(MaskArrayId, mask);
        }

        internal void BindRoad(Material material)
        {
            material.SetTexture(AlbedoArrayId, albedo);
            material.SetVector("_RoadTileMetres", roadTileMetres);
            material.SetFloat("_UseTerrainAlbedo", 1f);
        }

        internal void Unbind(Material terrain, Material grass)
        {
            if (terrain != null)
            {
                terrain.SetTexture(AlbedoArrayId, null);
                terrain.SetTexture(NormalArrayId, null);
                terrain.SetTexture(MaskArrayId, null);
            }
            if (grass != null) grass.SetTexture(MaskArrayId, null);
        }

        public void Dispose()
        {
            DestroyUnityObject(albedo);
            DestroyUnityObject(normal);
            DestroyUnityObject(mask);
            albedo = null;
            normal = null;
            mask = null;
        }

        private static Texture2DArray CreateArray(
            string name,
            int width,
            int height,
            TextureFormat format,
            bool linear)
        {
            var texture = new Texture2DArray(width, height, LayerCount, format, true, linear)
            {
                name = name,
                filterMode = FilterMode.Trilinear,
                wrapMode = TextureWrapMode.Repeat,
                anisoLevel = 4,
                hideFlags = HideFlags.DontSave,
            };
            return texture;
        }

        private static byte[] ExpandRgb(byte[] source)
        {
            var pixels = source.Length / 3;
            var destination = new byte[checked(pixels * 4)];
            for (var index = 0; index < pixels; index++)
            {
                destination[index * 4] = source[index * 3];
                destination[index * 4 + 1] = source[index * 3 + 1];
                destination[index * 4 + 2] = source[index * 3 + 2];
                destination[index * 4 + 3] = 255;
            }
            return destination;
        }

        private static byte[] PackHeightAndOcclusion(IslandPreparedMaterialTexture source)
        {
            var pixels = checked(source.width * source.height);
            var destination = new byte[checked(pixels * 2)];
            for (var index = 0; index < pixels; index++)
            {
                destination[index * 2] = source.heightR16[index * 2 + 1];
                destination[index * 2 + 1] = source.occlusion[index];
            }
            return destination;
        }

        private static void RequireProperties(Material material, params int[] properties)
        {
            if (material == null) throw new ArgumentNullException(nameof(material));
            foreach (var property in properties)
            {
                if (!material.HasProperty(property))
                {
                    throw new InvalidOperationException(
                        "The terrain shader does not expose its runtime texture-array contract.");
                }
            }
        }

        private static void DestroyUnityObject(UnityEngine.Object value)
        {
            if (value == null) return;
            if (Application.isPlaying) UnityEngine.Object.Destroy(value);
            else UnityEngine.Object.DestroyImmediate(value);
        }
    }
}
