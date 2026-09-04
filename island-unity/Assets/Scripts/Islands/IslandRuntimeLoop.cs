using UnityEngine;

public sealed partial class IslandGenerator
{
    private sealed class IslandRuntimeLoop
    {
        private readonly IslandGenerator generator;
        private bool hasStarted;

        internal IslandRuntimeLoop(IslandGenerator generator)
        {
            this.generator = generator;
        }

        internal void Start()
        {
            hasStarted = true;
            if (!generator.worldManaged && generator.Generation.GenerateOnStart)
            {
                generator.Generate();
            }
        }

        internal void Enable()
        {
            Camera.onPreCull += generator.PrepareCameraRender;
            if (generator.controlsWorldEnvironment && Application.isPlaying)
            {
                generator.EnsureWorldEnvironment();
            }
            EnsureActiveCameraDepthTextures();
            if (generator.controlsWorldEnvironment)
            {
                generator.ApplyDistanceHazeSettings();
                generator.UpdateSolarLighting(0f);
            }
            if (!generator.worldManaged
                && hasStarted
                && generator.Generation.GenerateOnStart
                && generator.terrainStreamer == null)
            {
                generator.Generate();
            }
        }

        internal void Disable()
        {
            Camera.onPreCull -= generator.PrepareCameraRender;
            if (generator.controlsWorldEnvironment)
            {
                RenderSettings.fog = false;
            }
            generator.generationLifecycle.Cancel();
            generator.ClearGeneratedContent();
        }

        internal void Update()
        {
            ApplyDebugKeys();
            generator.UpdateMaterialTransforms();
            generator.ApplyLiveSettings();
            if (generator.controlsWorldEnvironment)
            {
                generator.UpdateSolarLighting(Time.unscaledDeltaTime);
                generator.ApplyCloudSettings(Time.unscaledDeltaTime);
                generator.worldEnvironment?.SetFollowTarget(
                    generator.WorldEnvironmentFollowTarget());
            }
            if (!generator.worldManaged
                && generator.terrainStreamer != null
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
