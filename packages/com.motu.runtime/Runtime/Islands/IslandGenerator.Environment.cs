using System;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        private void UpdateMaterialTransforms(bool force = false)
        {
            var islandTransform = islandRuntime != null
                ? islandRuntime.transform
                : transform;
            var worldToLocal = islandTransform.worldToLocalMatrix;
            if (!force && hasAppliedWorldToLocal && appliedWorldToLocal == worldToLocal)
            {
                return;
            }
            appliedWorldToLocal = worldToLocal;
            hasAppliedWorldToLocal = true;
            SetIslandTransform(terrainMaterial, worldToLocal);
            SetIslandTransform(terrainLod1Material, worldToLocal);
            SetIslandTransform(terrainLod2Material, worldToLocal);
            SetIslandTransform(grassMaterial, worldToLocal);
            SetIslandTransform(rockMaterial, worldToLocal);
            SetIslandTransform(fallenLogMaterial, worldToLocal);
            SetIslandTransform(riverMaterial, worldToLocal);
            SetIslandTransform(reedMaterial, worldToLocal);
            SetIslandTransform(fernMaterial, worldToLocal);
        }

        private static void SetIslandTransform(Material material, Matrix4x4 worldToLocal)
        {
            if (material != null) material.SetMatrix(IslandWorldToLocalId, worldToLocal);
        }

        private void ApplyLiveSettings()
        {
            if (!appliedGrassColourA.HasValue
                || !appliedGrassColourB.HasValue
                || appliedGrassColourA.Value != Rendering.GrassColourA
                || appliedGrassColourB.Value != Rendering.GrassColourB
                || !Mathf.Approximately(
                    appliedGrassColourNoiseWorldSize,
                    Rendering.GrassColourNoiseWorldSizeMetres))
            {
                ApplyGrassColourSettings();
            }
            if (appliedShowRivers != Rendering.ShowRivers)
            {
                appliedShowRivers = Rendering.ShowRivers;
                terrainStreamer?.SetRiversVisible(Rendering.ShowRivers);
            }
            if (appliedShowGrass != Rendering.ShowGrass)
            {
                appliedShowGrass = Rendering.ShowGrass;
                terrainStreamer?.SetGrassVisible(Rendering.ShowGrass);
            }
            if (appliedShowRocks != Rendering.ShowRocks)
            {
                appliedShowRocks = Rendering.ShowRocks;
                terrainStreamer?.SetRocksVisible(Rendering.ShowRocks);
            }
            var snowline = Forest.SnowlineMetres;
            var terrainSnowlineChanged = terrainMaterial != null
                && terrainMaterial.HasProperty("_SnowLine")
                && !Mathf.Approximately(terrainMaterial.GetFloat("_SnowLine"), snowline);
            var grassSnowlineChanged = grassMaterial != null
                && grassMaterial.HasProperty("_SnowLine")
                && !Mathf.Approximately(grassMaterial.GetFloat("_SnowLine"), snowline);
            if (terrainSnowlineChanged || grassSnowlineChanged)
            {
                ApplySnowlineSettings();
            }
            if (appliedShowForests != Forest.ShowForests)
            {
                appliedShowForests = Forest.ShowForests;
                terrainStreamer?.SetForestsVisible(Forest.ShowForests);
                if (islandRuntime?.BirdPerches != null) islandRuntime.BirdPerches.TreesVisible = Forest.ShowForests;
            }
            if (appliedShowReeds != Reeds.ShowReeds)
            {
                appliedShowReeds = Reeds.ShowReeds;
                terrainStreamer?.SetReedsVisible(Reeds.ShowReeds);
            }
            if (appliedReedBaseColour != Reeds.BaseColour
                || appliedReedTipColour != Reeds.TipColour)
            {
                appliedReedBaseColour = Reeds.BaseColour;
                appliedReedTipColour = Reeds.TipColour;
                SetFoliageColours(reedMaterial, Reeds.BaseColour, Reeds.TipColour);
            }
            if (appliedShowFerns != Ferns.ShowFerns)
            {
                appliedShowFerns = Ferns.ShowFerns;
                terrainStreamer?.SetFernsVisible(Ferns.ShowFerns);
            }
            if (appliedFernBaseColour != Ferns.BaseColour
                || appliedFernTipColour != Ferns.TipColour)
            {
                appliedFernBaseColour = Ferns.BaseColour;
                appliedFernTipColour = Ferns.TipColour;
                SetFoliageColours(fernMaterial, Ferns.BaseColour, Ferns.TipColour);
            }
            if (appliedShowMeshEdges != DebugSettings.ShowMeshEdges)
            {
                appliedShowMeshEdges = DebugSettings.ShowMeshEdges;
                terrainStreamer?.SetMeshEdgesVisible(DebugSettings.ShowMeshEdges);
            }
            if (appliedWaterfallDebug != DebugSettings.ShowWaterfallFeet)
            {
                appliedWaterfallDebug = DebugSettings.ShowWaterfallFeet;
                terrainStreamer?.SetWaterfallFootDebug(DebugSettings.ShowWaterfallFeet);
            }
        }

        private static void SetFoliageColours(Material material, Color baseColour, Color tipColour)
        {
            if (material == null) return;
            material.SetColor("_BaseColor", baseColour);
            material.SetColor("_TipColor", tipColour);
        }

        private void BindWorldEnvironment(float worldSize)
        {
            if (worldEnvironment != null && !worldEnvironment.IsInstalled)
                throw new InvalidOperationException("The assigned Motu environment must be initialized before island installation.");
            seaMaterial = worldEnvironment != null ? worldEnvironment.SeaMaterial : Rendering.SeaMaterial;
        }

        private void SyncHostLighting()
        {
            if (grassMaterial == null) return;
            var light = RenderSettings.sun;
            grassMaterial.SetVector("_GrassLightDirection", light != null ? -light.transform.forward : Vector3.down);
            grassMaterial.SetColor("_GrassLightColor", light != null ? light.color * light.intensity : Color.black);
            grassMaterial.SetColor("_GrassAmbientColor", RenderSettings.ambientLight);
        }

        public void SetFirstPersonViewActive(bool active) =>
            worldEnvironment?.SetFirstPersonViewActive(active);
    }
}
