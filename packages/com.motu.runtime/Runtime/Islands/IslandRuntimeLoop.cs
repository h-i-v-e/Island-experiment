using Motu.Towns;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        private sealed class IslandRuntimeLoop
        {
            private readonly IslandGenerator generator;
            private readonly TownBuildingLodBudget townLods = new TownBuildingLodBudget();
            internal IslandRuntimeLoop(IslandGenerator generator)
            {
                this.generator = generator;
            }

            internal void Disable()
            {
                generator.generationLifecycle.Cancel();
                generator.ClearGeneratedContent();
            }

            internal void Update()
            {
                if (generator.activeProfile == null) return;
                generator.UpdateMaterialTransforms();
                generator.ApplyLiveSettings();
                if (!generator.managedByWorld)
                {
                    if (generator.worldEnvironment != null) generator.SyncSharedWorldLighting(generator.worldEnvironment);
                    else generator.SyncHostLighting();
                }
                var viewer = generator.Streaming.Target;
                if (viewer != null) generator.islandRuntime?.SetViewPosition(viewer.position);
                if (!generator.managedByWorld)
                    UpdateTownLods(viewer);
                if (generator.terrainStreamer != null
                    && generator.terrainStreamer.gameObject.activeInHierarchy
                    && generator.Streaming.Target != null)
                {
                    generator.terrainStreamer.SetPlayerPosition(
                        generator.Streaming.Target.position);
                }
            }

            private void UpdateTownLods(Transform viewer)
            {
                var town = generator.islandRuntime?.Town;
                if (town == null || !townLods.BeginUpdate()) return;
                if (viewer == null) viewer = Camera.main?.transform;
                if (viewer == null) return;
                townLods.Consider(town, viewer.position);
                townLods.Apply();
            }
        }
    }
}
