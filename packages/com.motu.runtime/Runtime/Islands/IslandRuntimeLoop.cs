using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandGenerator
    {
        private sealed class IslandRuntimeLoop
        {
            private readonly IslandGenerator generator;
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
                if (!generator.managedByWorld) generator.SyncHostLighting();
                var viewer = generator.Streaming.Target;
                if (viewer != null) generator.islandRuntime?.SetViewPosition(viewer.position);
                if (generator.terrainStreamer != null
                    && generator.Streaming.Target != null)
                {
                    generator.terrainStreamer.SetPlayerPosition(
                        generator.Streaming.Target.position);
                }
            }

        }
    }
}
