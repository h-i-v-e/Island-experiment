using System;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

namespace Motu.World
{
    public sealed partial class WorldEnvironmentController
    {
        private void PrepareSrpCamera(ScriptableRenderContext context, Camera camera)
        {
            if (CanReceiveEnvironmentCallbacks()) PrepareCameraRender(camera);
        }

        private static WorldEnvironmentController activeOwner;
        private EnvironmentHostState hostState;
        private bool seaVisible;

        private void AcquireEnvironment()
        {
            if (hostState != null) return;
            if (!isActiveAndEnabled)
                throw new InvalidOperationException("Enable the world environment before initializing it.");
            if (activeOwner != null && activeOwner != this)
                throw new InvalidOperationException("Motu supports one active global environment. Disable the current owner before initializing another.");
            hostState = new EnvironmentHostState();
            activeOwner = this;
            RenderPipelineManager.beginCameraRendering += PrepareSrpCamera;
            SceneManager.activeSceneChanged += ActiveSceneChanged;
            ApplyWeatherWindGlobals(Vector2.right, ReferenceWindSpeedMetresPerSecond);
            ApplyWeatherWindOffset(Vector2.zero);
        }

        private void ReleaseEnvironment()
        {
            UnsubscribeEnvironmentEvents();
            if (hostState == null) return;
            if (skyDomeObject != null) skyDomeObject.SetActive(false);
            if (moonLight != null) moonLight.enabled = false;
            if (ocean != null) { ocean.SetVisible(false); ocean.enabled = false; }
            hostState.Restore();
            hostState = null;
            if (activeOwner == this) activeOwner = null;
        }

        private void UnsubscribeEnvironmentEvents()
        {
            RenderPipelineManager.beginCameraRendering -= PrepareSrpCamera;
            SceneManager.activeSceneChanged -= ActiveSceneChanged;
        }

        private bool CanReceiveEnvironmentCallbacks()
        {
            if (this != null) return true;
            // Edit-mode audits can initialize an owner without Unity delivering
            // its lifecycle callbacks. Detach the destroyed target without
            // restoring its obsolete host snapshot into a subsequent owner.
            UnsubscribeEnvironmentEvents();
            hostState = null;
            if (ReferenceEquals(activeOwner, this)) activeOwner = null;
            return false;
        }

        private void ActiveSceneChanged(Scene previous, Scene next)
        {
            // Unity can deliver the initial activation notification after Awake
            // has already acquired this same scene's environment.
            if (!CanReceiveEnvironmentCallbacks() || hostState == null || hostState.HasSameActiveScene) return;
            // RenderSettings belongs to the active scene. The host should disable
            // us before switching it; never keep writing into the incoming scene.
            Debug.LogWarning("Disable the Motu environment before changing the active scene. The environment has been disabled to preserve the incoming scene's lighting.", this);
            enabled = false;
        }

        private void ResumeEnvironment()
        {
            AcquireEnvironment();
            CloudNoiseTextures.EnsureBound();
            Shader.SetGlobalFloat("_MotuCloudDomeRadius", cloudDomeRadius);
            hostState.BorrowLight(sunlight);
            skyDomeObject.SetActive(true);
            ocean.enabled = true;
            ocean.SetVisible(seaVisible);
            Shader.SetGlobalVector(EnvironmentWorldOffsetId, new Vector4(0f, -seaLevel, 0f, 0f));
            ApplyWeatherWindNoise(WeatherNoiseTexture);
            if (environmentSettings != null)
            {
                UpdateSolarLighting(0f);
                ApplyWeatherWind();
                ApplyCloudSettings(0f);
                ApplyDistanceHazeSettings();
            }
            BindExistingReflectionCameras();
        }
    }
}
