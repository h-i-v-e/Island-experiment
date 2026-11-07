using static Motu.Rendering.IslandMaterialFactory;
using static Motu.Rendering.UnityObjectLifetime;
using static Motu.Rendering.ProceduralNoiseTextures;
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Rendering;
using Motu.Settings;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        private static readonly int TerrainFieldOffsetId = Shader.PropertyToID("_MotuTerrainFieldOffset");
        private static readonly int TerrainSeamId = Shader.PropertyToID("_MotuTerrainSeam");

        internal static Vector4 TerrainFieldOffset(IslandDescriptor descriptor, IslandTerrainShape shape,
            bool coastalInlandSouth = false)
        {
            if (shape != IslandTerrainShape.CoastalStrip) return Vector4.zero;
            var direction = coastalInlandSouth ? -1f : 1f;
            return new Vector4((float)descriptor.LogicalXMetres * direction, 0f,
                (float)descriptor.LogicalZMetres * direction, 0f);
        }

        internal static Vector4 TerrainSeamSettings(IslandGenerationSettings generation) =>
            generation.TerrainShape == IslandTerrainShape.CoastalStrip
                ? new Vector4(generation.WorldSizeMetres * .5f, generation.CoastalSeamBlendWidthMetres, 0f, 0f)
                : Vector4.zero;

        private void BindTerrainFieldCoordinates(IslandDescriptor descriptor, bool coastalInlandSouth)
        {
            // The offset uses logical metres, so floating-origin moves do not
            // change the material phase or the local matrices used for queries.
            var offset = TerrainFieldOffset(descriptor, Generation.TerrainShape, coastalInlandSouth);
            var seam = TerrainSeamSettings(Generation);
            SetTerrainFieldCoordinates(terrainMaterial, offset, seam);
            SetTerrainFieldCoordinates(terrainLod1Material, offset, seam);
            SetTerrainFieldCoordinates(terrainLod2Material, offset, seam);
            SetTerrainFieldCoordinates(grassMaterial, offset, seam);
        }

        private static void SetTerrainFieldCoordinates(Material material, Vector4 offset, Vector4 seam)
        {
            if (material == null) return;
            if (material.HasProperty(TerrainFieldOffsetId)) material.SetVector(TerrainFieldOffsetId, offset);
            if (material.HasProperty(TerrainSeamId)) material.SetVector(TerrainSeamId, seam);
        }

        private void BuildRuntimeMaterials(IslandPreparedMaterialTextures materialTextures)
        {
            var skyColor = new Color(0.49f, 0.68f, 0.82f);
            BindWorldEnvironment(Generation.WorldSizeMetres);
            terrainMaterial = CreateMaterial(
                "Motu/Terrain Unified",
                Color.white,
                Rendering.TerrainMaterial,
                Generation.WorldSizeMetres);
            terrainMaterialTextures = new TerrainMaterialTextureArrays(materialTextures);
            terrainMaterialTextures.BindTerrain(terrainMaterial);
            cliffNoiseTexture = Rendering.CliffDetailNoise;
            if (cliffNoiseTexture == null)
            {
                cliffNoiseLease = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.Cliff);
                cliffNoiseTexture = (Texture3D)cliffNoiseLease.Texture;
            }
            terrainMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
            var dirtColor = materialTextures.colours.dirt;
            var rockColor = materialTextures.colours.stone;
            var sandColor = materialTextures.colours.sand;
            terrainMaterial.SetColor("_RockColor", rockColor);
            terrainMaterial.SetColor("_GroundDirtColor", dirtColor);
            terrainMaterial.SetColor("_SandColor", sandColor);
            terrainMaterial.SetColor("_ForestFloorColor", Color.white);
            terrainMaterial.SetColor("_StonesColor", Color.white);
            grassMaterial = CreateMaterial(
                "Motu/Terrain Grass",
                Color.white,
                Rendering.GrassMaterial,
                Generation.WorldSizeMetres);
            terrainMaterialTextures.BindGrass(grassMaterial);
            grassMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
            CopyTerrainBlendSettingsToGrass();
            ApplySnowlineSettings();
            fallenLogMaterial = CreateMaterial(
                "Motu/Tree Wood",
                new Color(0.16f, 0.085f, 0.045f, 1f),
                null,
                Generation.WorldSizeMetres);
            CreateFallenLogBarkTextures(materialTextures.treeBark);
            fallenLogMaterial.SetTexture("_EndGrainMap", Resources.Load<Texture2D>("Motu/TreeEndGrain"));
            fallenLogMaterial.SetColor("_GroundDirtColor", dirtColor);
            fallenLogMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
            fallenLogMaterial.enableInstancing = false;
            reedMaterial = CreateMaterial(
                "Motu/Riverbank Reeds",
                Reeds.BaseColour,
                null,
                Generation.WorldSizeMetres);
            if (reedMaterial.shader.name != "Motu/Riverbank Reeds")
            {
                throw new InvalidOperationException(
                    "Could not find shader 'Motu/Riverbank Reeds'.");
            }
            reedMaterial.SetColor("_BaseColor", Reeds.BaseColour);
            reedMaterial.SetColor("_TipColor", Reeds.TipColour);
            reedMaterial.enableInstancing = true;
            fernMaterial = CreateMaterial(
                "Motu/Forest Ferns",
                Ferns.BaseColour,
                null,
                Generation.WorldSizeMetres);
            if (fernMaterial.shader.name != "Motu/Forest Ferns")
            {
                throw new InvalidOperationException(
                    "Could not find shader 'Motu/Forest Ferns'.");
            }
            fernMaterial.SetColor("_BaseColor", Ferns.BaseColour);
            fernMaterial.SetColor("_TipColor", Ferns.TipColour);
            fernMaterial.enableInstancing = true;
            rockMaterial = CreateMaterial(
                "Motu/Rock Decoration",
                rockColor,
                Rendering.RockMaterial,
                Generation.WorldSizeMetres);
            rockMaterial.name = "Island rock decoration material";
            rockMaterial.enableInstancing = true;
            rockMaterial.SetColor("_RockColor", rockColor);
            rockMaterial.SetTexture("_CliffNoise3D", cliffNoiseTexture);
            rockMaterial.SetFloat(
                "_CliffNoisePeriod",
                terrainMaterial.GetFloat("_CliffNoisePeriod"));
            rockMaterial.SetFloat(
                "_CliffNoiseDetailScale",
                terrainMaterial.GetFloat("_CliffNoiseDetailScale"));
            rockMaterial.SetFloat(
                "_CliffNormalStrength",
                terrainMaterial.GetFloat("_CliffNormalStrength"));
            terrainMaterial.SetFloat(
                "_RockPatchNoiseDetailScale",
                RockPatchNoiseDetailScale);
            grassMaterial.SetFloat(
                "_RockPatchNoiseDetailScale",
                RockPatchNoiseDetailScale);
            terrainMaterial.SetFloat(
                "_SandPatchNoiseWorldSize",
                Rendering.SandPatchSizeMetres);
            grassMaterial.SetFloat(
                "_SandPatchNoiseWorldSize",
                Rendering.SandPatchSizeMetres);
            grassPatchNoiseTexture = worldEnvironment?.WeatherNoiseTexture;
            if (grassPatchNoiseTexture == null)
            {
                grassNoiseLease = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.Weather);
                grassPatchNoiseTexture = (Texture2D)grassNoiseLease.Texture;
            }
            terrainMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
            terrainMaterial.SetFloat(
                "_GrassPatchNoiseWorldSize",
                Rendering.GrassPatchSizeMetres);
            grassMaterial.SetTexture("_GrassPatchNoise", grassPatchNoiseTexture);
            grassMaterial.SetFloat(
                "_GrassPatchNoiseWorldSize",
                Rendering.GrassPatchSizeMetres);
            ApplyGrassColourSettings();
            terrainLod1Material = new Material(terrainMaterial)
            {
                name = "Island terrain LOD1 material (no parallax)",
            };
            terrainLod1Material.EnableKeyword("MOTU_TERRAIN_LOD1");
            terrainLod2Material = new Material(terrainMaterial)
            {
                name = "Island terrain LOD2 material (procedural only)",
            };
            terrainLod2Material.EnableKeyword("MOTU_TERRAIN_LOD2");
            var sun = RenderSettings.sun;
            grassMaterial.SetVector(
                "_GrassLightDirection",
                sun != null ? -sun.transform.forward : Vector3.down);
            grassMaterial.SetColor(
                "_GrassLightColor",
                sun != null ? sun.color * sun.intensity : Color.white);
            grassMaterial.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
            riverNoiseTexture = Rendering.RiverNoise;
            if (riverNoiseTexture == null)
            {
                riverNoiseLease = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.River);
                riverNoiseTexture = (Texture2D)riverNoiseLease.Texture;
            }
            var waterColor = new Color(0.03f, 0.28f, 0.55f, 1f);
            const float shallowWaterOpacity = 0.25f;
            const float fullOpacityDepth = 5f;
            riverMaterial = CreateMaterial(
                "Motu/River Water",
                waterColor,
                Rendering.RiverMaterial,
                Generation.WorldSizeMetres);
            riverMaterial.renderQueue = (int)RenderQueue.Transparent + 10;
            riverMaterial.SetTexture("_NoiseTex", riverNoiseTexture);
            riverMaterial.SetFloat("_WhitewaterStrength", 0.9f);
            riverMaterial.SetFloat("_ShallowOpacity", shallowWaterOpacity);
            riverMaterial.SetFloat("_OpacityDepth", fullOpacityDepth);
            riverMaterial.SetFloat("_EstuaryBlendHeight", Rendering.EstuaryBlendHeightMetres);
            riverMaterial.SetFloat("_SeaLevel", SeaHeight);
            riverMaterial.SetColor("_ReflectionColor", skyColor);
            riverMaterial.SetColor(
                "_ReflectionHorizonColor",
                Color.Lerp(skyColor, Color.white, 0.35f));
            riverMaterial.SetFloat("_ReflectionStrength", 0.45f);
            riverMaterial.SetFloat("_PlanarReflectionWeight", 1f);
            riverMaterial.SetFloat("_PlanarReflectionDistortion", 0.006f);
            riverMaterial.SetFloat("_SunGlintStrength", 0.55f);
            riverMaterial.SetColor(
                "_SeaColor",
                seaMaterial != null && seaMaterial.HasProperty("_Color")
                    ? seaMaterial.GetColor("_Color")
                    : waterColor);

            meshEdgeMaterial = CreateMaterial(
                "Motu/Mesh Edge Overlay",
                Color.black,
                null,
                Generation.WorldSizeMetres);
            meshEdgeMaterial.renderQueue = (int)RenderQueue.Overlay + 100;
            meshEdgeMaterial.SetColor("_Color", Color.black);
            meshEdgeMaterial.SetFloat("_ZTest", (float)CompareFunction.LessEqual);
            UpdateMaterialTransforms(true);
        }

        private void CopyTerrainBlendSettingsToGrass()
        {
            CopyMaterialVector("_TerrainLayerWorldSizesA");
            CopyMaterialVector("_TerrainLayerWorldSizesB");
            CopyMaterialVector("_TerrainHeightInfluencesA");
            CopyMaterialVector("_TerrainHeightInfluencesB");
            CopyMaterialFloat("_TerrainHeightBlendDepth");
            CopyMaterialFloat("_TopTextureFadeOutSlope");
            CopyMaterialFloat("_SteepStoneBlendWidth");
            CopyMaterialFloat("_ForestFloorEdgeNoiseStrength");
            CopyMaterialFloat("_ForestFloorEdgeBlendWidth");
            CopyMaterialFloat("_StonesEdgeNoiseStrength");
            CopyMaterialFloat("_StonesEdgeBlendWidth");
            CopyMaterialFloat("_BeachEdgeNoiseStrength");
            CopyMaterialFloat("_BeachEdgeBlendWidth");
            CopyMaterialFloat("_RiverEdgeNoiseStrength");
            CopyMaterialFloat("_RiverEdgeBlendWidth");
            CopyMaterialFloat("_CliffNormalCutoff");
            CopyMaterialFloat("_CliffBoundaryNoiseStrength");
            CopyMaterialFloat("_RockBoundaryNoiseStrength");
            CopyMaterialFloat("_SandRockSlopeThreshold");
            CopyMaterialFloat("_CliffNoisePeriod");
            CopyMaterialFloat("_RockPatchNoiseDetailScale");
            CopyMaterialFloat("_SandPatchNoiseWorldSize");
            CopyMaterialFloat("_GrassPatchNoiseWorldSize");
            CopyMaterialFloat("_SnowEdgeNoiseMetres");
            CopyMaterialFloat("_SnowMacroNoiseMetres");
        }

        private void ApplySnowlineSettings()
        {
            var snowline = Forest.SnowlineMetres;
            if (terrainMaterial != null && terrainMaterial.HasProperty("_SnowLine"))
            {
                terrainMaterial.SetFloat("_SnowLine", snowline);
            }
            terrainLod1Material?.SetFloat("_SnowLine", snowline);
            terrainLod2Material?.SetFloat("_SnowLine", snowline);
            if (grassMaterial != null && grassMaterial.HasProperty("_SnowLine"))
            {
                grassMaterial.SetFloat("_SnowLine", snowline);
            }
        }

        private void CopyMaterialVector(string propertyName)
        {
            if (terrainMaterial.HasProperty(propertyName)
                && grassMaterial.HasProperty(propertyName))
            {
                grassMaterial.SetVector(
                    propertyName,
                    terrainMaterial.GetVector(propertyName));
            }
        }

        private void CopyMaterialFloat(string propertyName)
        {
            if (terrainMaterial.HasProperty(propertyName)
                && grassMaterial.HasProperty(propertyName))
            {
                grassMaterial.SetFloat(
                    propertyName,
                    terrainMaterial.GetFloat(propertyName));
            }
        }


        private void ApplyGrassColourSettings()
        {
            appliedGrassColourA = Rendering.GrassColourA;
            appliedGrassColourB = Rendering.GrassColourB;
            appliedGrassColourNoiseWorldSize = Rendering.GrassColourNoiseWorldSizeMetres;
            terrainMaterial?.SetColor("_GrassColorA", appliedGrassColourA.Value);
            terrainMaterial?.SetColor("_GrassColorB", appliedGrassColourB.Value);
            terrainMaterial?.SetFloat(
                "_GrassColorNoiseWorldSize",
                appliedGrassColourNoiseWorldSize);
            terrainLod1Material?.SetColor("_GrassColorA", appliedGrassColourA.Value);
            terrainLod1Material?.SetColor("_GrassColorB", appliedGrassColourB.Value);
            terrainLod1Material?.SetFloat(
                "_GrassColorNoiseWorldSize",
                appliedGrassColourNoiseWorldSize);
            terrainLod2Material?.SetColor("_GrassColorA", appliedGrassColourA.Value);
            terrainLod2Material?.SetColor("_GrassColorB", appliedGrassColourB.Value);
            terrainLod2Material?.SetFloat(
                "_GrassColorNoiseWorldSize",
                appliedGrassColourNoiseWorldSize);
            grassMaterial?.SetColor("_GrassColorA", appliedGrassColourA.Value);
            grassMaterial?.SetColor("_GrassColorB", appliedGrassColourB.Value);
            grassMaterial?.SetFloat(
                "_GrassColorNoiseWorldSize",
                appliedGrassColourNoiseWorldSize);
        }

        private void CreateSurfaceTextures(
            IslandPreparedSurfaceMaps surfaceMaps,
            IslandPreparedSurfaceMaps[] lodSurfaceMaps = null)
        {
            if (!terrainMaterial.HasProperty("_WorldNormal")
                || !terrainMaterial.HasProperty("_Occlusion"))
            {
                throw new InvalidOperationException(
                    "The unified terrain shader does not expose its shared surface textures.");
            }
            // Keep LOD0's planar maps and full mip chain unchanged. Its normal
            // map remains disabled by the streamer's per-renderer property.
            terrainOcclusionTexture = CreateSurfaceTexture(
                "Motu Shared Terrain Occlusion",
                surfaceMaps.dimension,
                TextureFormat.R8,
                surfaceMaps.occlusion);
            islandRuntime?.OwnTexture(terrainOcclusionTexture);
            terrainNormalTexture = CreateSurfaceTexture(
                "Motu Shared Terrain World Normal",
                surfaceMaps.dimension,
                TextureFormat.RGB24,
                surfaceMaps.normalRgb);
            islandRuntime?.OwnTexture(terrainNormalTexture);
            terrainMaterial.SetTexture("_WorldNormal", terrainNormalTexture);
            terrainMaterial.SetTexture("_Occlusion", terrainOcclusionTexture);
            BindLodSurfaceTextures(
                terrainLod1Material,
                lodSurfaceMaps != null && lodSurfaceMaps.Length > 0 ? lodSurfaceMaps[0] : null,
                1,
                out terrainLod1NormalTexture,
                out terrainLod1OcclusionTexture);
            BindLodSurfaceTextures(
                terrainLod2Material,
                lodSurfaceMaps != null && lodSurfaceMaps.Length > 1 ? lodSurfaceMaps[1] : null,
                2,
                out terrainLod2NormalTexture,
                out terrainLod2OcclusionTexture);
        }

        private void BindLodSurfaceTextures(
            Material material,
            IslandPreparedSurfaceMaps maps,
            int lod,
            out Texture2D normal,
            out Texture2D occlusion)
        {
            normal = null;
            occlusion = null;
            if (maps == null)
            {
                // Legacy prepared payloads retain the original planar mesh UVs.
                material.SetTexture("_WorldNormal", terrainNormalTexture);
                material.SetTexture("_Occlusion", terrainOcclusionTexture);
                return;
            }
            occlusion = CreateSurfaceAtlasTexture(
                $"Motu Terrain LOD{lod} Atlas Occlusion",
                maps.width,
                maps.height,
                TextureFormat.R8,
                maps.occlusion);
            islandRuntime?.OwnTexture(occlusion);
            normal = CreateSurfaceAtlasTexture(
                $"Motu Terrain LOD{lod} Atlas World Normal",
                maps.width,
                maps.height,
                TextureFormat.RGB24,
                maps.normalRgb);
            islandRuntime?.OwnTexture(normal);
            material.SetTexture("_WorldNormal", normal);
            material.SetTexture("_Occlusion", occlusion);
        }

        private void CreateSeaMaskTexture(IslandPreparedSeaMask seaMask)
        {
            if (!SystemInfo.SupportsTextureFormat(TextureFormat.RGBA32))
            {
                throw new InvalidOperationException(
                    "This graphics device does not support the required RGBA32 sea mask texture.");
            }
            seaMaskTexture = CreateSurfaceTexture(
                "Motu Coastal Wave Mask",
                seaMask.dimension,
                TextureFormat.RGBA32,
                seaMask.rgba);
            islandRuntime.OwnTexture(seaMaskTexture);
        }

        private void TransferMaterialOwnershipToRuntime()
        {
            if (islandRuntime == null)
            {
                throw new InvalidOperationException(
                    "Per-island materials require an installing island runtime.");
            }
            islandRuntime.OwnTerrainTextureArrays(
                terrainMaterialTextures,
                terrainMaterial,
                grassMaterial);
            islandRuntime.OwnMaterial(terrainMaterial);
            islandRuntime.OwnMaterial(terrainLod1Material);
            islandRuntime.OwnMaterial(terrainLod2Material);
            islandRuntime.OwnMaterial(grassMaterial);
            islandRuntime.OwnMaterial(rockMaterial);
            islandRuntime.OwnMaterial(fallenLogMaterial);
            islandRuntime.OwnTexture(fallenLogBarkAlbedoTexture);
            islandRuntime.OwnTexture(fallenLogBarkHeightTexture);
            islandRuntime.OwnTexture(fallenLogBarkNormalTexture);
            islandRuntime.OwnTexture(fallenLogBarkOcclusionTexture);
            islandRuntime.OwnMaterial(reedMaterial);
            islandRuntime.OwnMaterial(fernMaterial);
            islandRuntime.OwnMaterial(riverMaterial);
            islandRuntime.OwnMaterial(meshEdgeMaterial);
            islandRuntime.OwnResource(cliffNoiseLease);
            islandRuntime.OwnResource(riverNoiseLease);
            islandRuntime.OwnResource(grassNoiseLease);
        }


        internal void CreateFallenLogBarkTextures(IslandPreparedMaterialTexture bark)
        {
            if (bark == null) throw new ArgumentNullException(nameof(bark));
            fallenLogBarkAlbedoTexture = CreateRuntimeMaterialTexture(
                "Motu Runtime Tree Bark Albedo",
                bark.width,
                bark.height,
                TextureFormat.RGB24,
                false,
                bark.albedoRgb);
            try
            {
                fallenLogBarkHeightTexture = CreateRuntimeMaterialTexture(
                    "Motu Runtime Tree Bark Height",
                    bark.width,
                    bark.height,
                    TextureFormat.R16,
                    true,
                    bark.heightR16);
                fallenLogBarkNormalTexture = CreateRuntimeMaterialTexture(
                    "Motu Runtime Tree Bark Normal",
                    bark.width,
                    bark.height,
                    TextureFormat.RGB24,
                    true,
                    bark.normalRgb);
                fallenLogBarkOcclusionTexture = CreateRuntimeMaterialTexture(
                    "Motu Runtime Tree Bark Occlusion",
                    bark.width,
                    bark.height,
                    TextureFormat.R8,
                    true,
                    bark.occlusion);
                fallenLogMaterial.SetTexture("_BarkAlbedoMap", fallenLogBarkAlbedoTexture);
                fallenLogMaterial.SetTexture("_BarkHeightMap", fallenLogBarkHeightTexture);
                fallenLogMaterial.SetTexture("_BarkNormalMap", fallenLogBarkNormalTexture);
                fallenLogMaterial.SetTexture("_BarkOcclusionMap", fallenLogBarkOcclusionTexture);
                fallenLogMaterial.SetFloat("_BarkTileWidthMetres", bark.physicalTileWidthMetres);
                fallenLogMaterial.SetFloat("_BarkTileHeightMetres", bark.physicalTileHeightMetres);
            }
            catch
            {
                DestroyUnityObject(fallenLogBarkAlbedoTexture);
                DestroyUnityObject(fallenLogBarkHeightTexture);
                DestroyUnityObject(fallenLogBarkNormalTexture);
                DestroyUnityObject(fallenLogBarkOcclusionTexture);
                fallenLogBarkAlbedoTexture = null;
                fallenLogBarkHeightTexture = null;
                fallenLogBarkNormalTexture = null;
                fallenLogBarkOcclusionTexture = null;
                throw;
            }
        }


        internal void DestroyRuntimeMaterials()
        {
            terrainMaterialTextures?.Unbind(terrainMaterial, grassMaterial);
            terrainMaterialTextures?.Dispose();
            terrainMaterialTextures = null;
            DestroyUnityObject(terrainMaterial);
            DestroyUnityObject(terrainLod1Material);
            DestroyUnityObject(terrainLod2Material);
            DestroyUnityObject(grassMaterial);
            DestroyUnityObject(rockMaterial);
            DestroyUnityObject(fallenLogMaterial);
            DestroyUnityObject(fallenLogBarkAlbedoTexture);
            DestroyUnityObject(fallenLogBarkHeightTexture);
            DestroyUnityObject(fallenLogBarkNormalTexture);
            DestroyUnityObject(fallenLogBarkOcclusionTexture);
            DestroyUnityObject(reedMaterial);
            DestroyUnityObject(fernMaterial);
            DestroyUnityObject(riverMaterial);
            DestroyUnityObject(meshEdgeMaterial);
            cliffNoiseLease?.Dispose();
            riverNoiseLease?.Dispose();
            grassNoiseLease?.Dispose();
            terrainMaterial = null;
            terrainLod1Material = null;
            terrainLod2Material = null;
            grassMaterial = null;
            rockMaterial = null;
            fallenLogMaterial = null;
            fallenLogBarkAlbedoTexture = null;
            fallenLogBarkHeightTexture = null;
            fallenLogBarkNormalTexture = null;
            fallenLogBarkOcclusionTexture = null;
            reedMaterial = null;
            fernMaterial = null;
            riverMaterial = null;
            seaMaterial = null;
            meshEdgeMaterial = null;
            cliffNoiseTexture = null;
            riverNoiseTexture = null;
            grassPatchNoiseTexture = null;
            cliffNoiseLease = riverNoiseLease = grassNoiseLease = null;
        }

        private void ResetAppliedLiveSettings()
        {
            appliedShowRivers = null;
            appliedShowGrass = null;
            appliedShowRocks = null;
            appliedShowForests = null;
            appliedShowReeds = null;
            appliedReedBaseColour = null;
            appliedReedTipColour = null;
            appliedShowFerns = null;
            appliedFernBaseColour = null;
            appliedFernTipColour = null;
            appliedShowMeshEdges = null;
            appliedWaterfallDebug = null;
            appliedGrassColourA = null;
            appliedGrassColourB = null;
            appliedGrassColourNoiseWorldSize = float.NaN;
            hasAppliedWorldToLocal = false;
        }


    }
}
