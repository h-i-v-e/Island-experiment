using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

public sealed partial class IslandGenerator
{
    private sealed class IslandRuntimeInstaller
    {
        private readonly IslandGenerator generator;

        internal IslandRuntimeInstaller(IslandGenerator generator)
        {
            this.generator = generator;
        }

        internal async Task InstallAsync(
            IslandPreparedData prepared,
            IslandDescriptor descriptor,
            float worldSize,
            CancellationToken cancellationToken,
            UnityFrameBudget frameBudget)
        {
            generator.ClearGeneratedContent();
            generator.DestroyRuntimeMaterials();
            generator.BuildRuntimeMaterials(prepared.materialTextures);
            await frameBudget.YieldIfExceededAsync(cancellationToken);

            generator.islandRuntime = IslandRuntime.Create(
                descriptor,
                generator.transform);
            generator.runtimeRoot = generator.islandRuntime.gameObject;
            generator.TransferMaterialOwnershipToRuntime();
            generator.UpdateMaterialTransforms(true);
            generator.islandHandle = prepared.TakeHandle();
            generator.islandRuntime.AdoptNativeHandle(generator.islandHandle);

            generator.CreateSurfaceTextures(prepared.surfaceMaps);
            await frameBudget.YieldIfExceededAsync(cancellationToken);
            generator.CreateSeaMaskTexture(prepared.seaMask);
            await frameBudget.YieldIfExceededAsync(cancellationToken, true);

            generator.BindWorldEnvironment(worldSize);
            await frameBudget.YieldIfExceededAsync(cancellationToken);

            generator.CreateCoastalWaterOverlay(worldSize);
            generator.islandRuntime.SetCoastalWaterObject(
                generator.coastalWaterObject);
            generator.islandRuntime.SetCoastalWaveMask(
                generator.worldEnvironment,
                generator.seaMaskTexture,
                worldSize);
            if (generator.controlsWorldEnvironment)
            {
                generator.UpdateSolarLighting(0f);
            }

            var terrainRoot = new GameObject("Terrain Tiles");
            terrainRoot.transform.SetParent(generator.runtimeRoot.transform, false);
            generator.terrainStreamer = terrainRoot.AddComponent<TerrainTileStreamer>();
            generator.islandRuntime.SetTerrainStreamer(generator.terrainStreamer);
            await generator.terrainStreamer.InitializeAsync(
                generator.islandHandle.Value,
                generator.terrainMaterial,
                generator.terrainLod1Material,
                generator.terrainLod2Material,
                generator.grassMaterial,
                generator.rockMaterial,
                generator.treeWoodMaterial,
                generator.treeLod1WoodMaterial,
                generator.treeFoliageMaterial,
                generator.treeLod0FoliageMaterial,
                generator.reedMaterial,
                generator.fernMaterial,
                generator.riverMaterial,
                generator.meshEdgeMaterial,
                worldSize,
                prepared.overviewTiles,
                prepared.riverTiles,
                prepared.riverRockTiles,
                prepared.forest,
                prepared.reedTiles,
                prepared.fernTiles,
                prepared.waterfallFeet,
                prepared.colliderHeightMap,
                generator.Rendering.ShowRivers,
                generator.Rendering.ShowGrass,
                generator.Rendering.ShowRocks,
                generator.Forest.ShowForests,
                generator.Reeds.ShowReeds,
                generator.Ferns.ShowFerns,
                cancellationToken,
                frameBudget);
            generator.terrainStreamer.SetWaterfallFootDebug(
                generator.DebugSettings.ShowWaterfallFeet);
            generator.islandRuntime.Activate();

            generator.ResetAppliedLiveSettings();
            generator.ApplyLiveSettings();
            if (generator.Streaming.Target != null)
            {
                generator.terrainStreamer.SetPlayerPosition(
                    generator.Streaming.Target.position);
            }
        }
    }
}
