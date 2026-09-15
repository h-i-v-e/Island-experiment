using System;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace Motu.World
{
    public sealed partial class WorldEnvironmentController
    {
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
            Camera.onPreCull += PrepareCameraRender;
            SceneManager.activeSceneChanged += ActiveSceneChanged;
            ApplyWeatherWindGlobals(Vector2.right, ReferenceWindSpeedMetresPerSecond);
            ApplyWeatherWindOffset(Vector2.zero);
        }

        private void ReleaseEnvironment()
        {
            Camera.onPreCull -= PrepareCameraRender;
            SceneManager.activeSceneChanged -= ActiveSceneChanged;
            if (hostState == null) return;
            if (skyDomeObject != null) skyDomeObject.SetActive(false);
            if (moonLight != null) moonLight.enabled = false;
            if (ocean != null) { ocean.SetVisible(false); ocean.enabled = false; }
            hostState.Restore();
            hostState = null;
            if (activeOwner == this) activeOwner = null;
        }

        private void ActiveSceneChanged(Scene previous, Scene next)
        {
            // Unity can deliver the initial activation notification after Awake
            // has already acquired this same scene's environment.
            if (hostState == null || hostState.HasSameActiveScene) return;
            // RenderSettings belongs to the active scene. The host should disable
            // us before switching it; never keep writing into the incoming scene.
            Debug.LogWarning("Disable the Motu environment before changing the active scene. The environment has been disabled to preserve the incoming scene's lighting.", this);
            enabled = false;
        }

        private void ResumeEnvironment()
        {
            AcquireEnvironment();
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
