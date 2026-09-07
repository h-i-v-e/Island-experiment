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

            internal void Enable()
            {
                EnsureActiveCameraDepthTextures();
            }

            internal void Disable()
            {
                generator.generationLifecycle.Cancel();
                generator.ClearGeneratedContent();
            }

            internal void Update()
            {
                ApplyDebugKeys();
                generator.UpdateMaterialTransforms();
                generator.ApplyLiveSettings();
                if (generator.terrainStreamer != null
                    && generator.Streaming.Target != null)
                {
                    generator.terrainStreamer.SetPlayerPosition(
                        generator.Streaming.Target.position);
                }
            }

            private void ApplyDebugKeys()
            {
                var settings = generator.DebugSettings;
                if (settings.ToggleMeshEdgesKey != KeyCode.None
                    && Input.GetKeyDown(settings.ToggleMeshEdgesKey))
                {
                    settings.ShowMeshEdges = !settings.ShowMeshEdges;
                }
                if (settings.ToggleTreeMeshEdgesKey != KeyCode.None
                    && Input.GetKeyDown(settings.ToggleTreeMeshEdgesKey))
                {
                    settings.ShowTreeMeshEdges = !settings.ShowTreeMeshEdges;
                }
                if (settings.ToggleFrameRateKey != KeyCode.None
                    && Input.GetKeyDown(settings.ToggleFrameRateKey))
                {
                    settings.ShowFrameRate = !settings.ShowFrameRate;
                }
            }

            private static void EnsureActiveCameraDepthTextures()
            {
                foreach (var camera in Camera.allCameras)
                {
                    IslandGenerator.EnsureCameraDepthTexture(camera);
                }
            }
        }
    }
}
