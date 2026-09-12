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
            terrainMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            terrainLod1Material?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            terrainLod2Material?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            grassMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            rockMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            riverMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            treeWoodMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            treeLod1WoodMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            treeFoliageMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            treeLod0FoliageMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            reedMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
            fernMaterial?.SetMatrix(IslandWorldToLocalId, worldToLocal);
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
                reedMaterial?.SetColor("_BaseColor", Reeds.BaseColour);
                reedMaterial?.SetColor("_TipColor", Reeds.TipColour);
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
                fernMaterial?.SetColor("_BaseColor", Ferns.BaseColour);
                fernMaterial?.SetColor("_TipColor", Ferns.TipColour);
            }
            if (appliedShowMeshEdges != DebugSettings.ShowMeshEdges)
            {
                appliedShowMeshEdges = DebugSettings.ShowMeshEdges;
                terrainStreamer?.SetMeshEdgesVisible(DebugSettings.ShowMeshEdges);
            }
            if (appliedShowTreeMeshEdges != DebugSettings.ShowTreeMeshEdges)
            {
                appliedShowTreeMeshEdges = DebugSettings.ShowTreeMeshEdges;
                terrainStreamer?.SetTreeMeshEdgesVisible(DebugSettings.ShowTreeMeshEdges);
            }
            if (appliedWaterfallDebug != DebugSettings.ShowWaterfallFeet)
            {
                appliedWaterfallDebug = DebugSettings.ShowWaterfallFeet;
                terrainStreamer?.SetWaterfallFootDebug(DebugSettings.ShowWaterfallFeet);
            }
        }
        private void BindWorldEnvironment(float worldSize)
        {
            if (worldEnvironment == null || !worldEnvironment.IsInstalled)
                throw new InvalidOperationException("The world must supply an initialized environment before island installation.");
            seaMaterial = worldEnvironment.SeaMaterial;
        }

        public void SetFirstPersonViewActive(bool active) =>
            worldEnvironment?.SetFirstPersonViewActive(active);
    }
}
