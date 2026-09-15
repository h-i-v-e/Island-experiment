using System;
using System.Collections.Generic;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

namespace Motu.World
{
    // Captured once per activation; never allocates in the steady-state render loop.
    // The host keeps its active scene while this owner is enabled. An additive
    // environment scene can be unloaded without changing that host scene.
    internal sealed class EnvironmentHostState
    {
        private readonly Scene scene = SceneManager.GetActiveScene();
        private readonly bool fog = RenderSettings.fog;
        private readonly FogMode fogMode = RenderSettings.fogMode;
        private readonly Color fogColour = RenderSettings.fogColor;
        private readonly float fogDensity = RenderSettings.fogDensity;
        private readonly AmbientMode ambientMode = RenderSettings.ambientMode;
        private readonly Color ambientLight = RenderSettings.ambientLight;
        internal readonly Light Sun = RenderSettings.sun;
        private readonly Dictionary<Light, LightState> lights = new Dictionary<Light, LightState>();
        private readonly Dictionary<Camera, CameraState> cameras = new Dictionary<Camera, CameraState>();

        private readonly GlobalValues<float> floats = new GlobalValues<float>(Shader.GetGlobalFloat, Shader.SetGlobalFloat,
            "_MotuCloudEnabled", "_MotuCloudCoverage", "_MotuCloudDensity", "_MotuCloudAltitude",
            "_MotuCloudWorldSize", "_MotuCloudShadowStrength", "_MotuCloudAmbientShadowStrength",
            "_MotuCloudCelestialStrength", "_MotuCloudLowElevationFade", "_MotuCloudLightActive",
            "_MotuCloudSunsetStrength", "_MotuCloudNightStrength");
        private readonly GlobalValues<Vector4> vectors = new GlobalValues<Vector4>(Shader.GetGlobalVector, Shader.SetGlobalVector,
            "_MotuEnvironmentWorldOffset", "_MotuWeatherWind", "_MotuWindMaterial", "_MotuTreeWindHeights",
            "_MotuWindOffset", "_MotuCloudVolume", "_MotuCloudBroadNoise", "_MotuCloudDetailErosion",
            "_MotuCloudWindOffset", "_MotuCloudLightDirection");
        private readonly GlobalValues<Color> colours = new GlobalValues<Color>(Shader.GetGlobalColor, Shader.SetGlobalColor,
            "_MotuCloudDayColor", "_MotuCloudSunsetColor", "_MotuCloudNightColor", "_MotuCloudLightColor",
            "_MotuCloudAmbientColor", "_MotuIslandHorizonColour");
        private readonly GlobalValues<Texture> textures = new GlobalValues<Texture>(Shader.GetGlobalTexture, Shader.SetGlobalTexture,
            "_MotuCloudWeatherTex", "_MotuWindNoise");

        internal bool HasSameActiveScene => scene == SceneManager.GetActiveScene();

        internal void BorrowLight(Light light)
        {
            if (light != null && !lights.ContainsKey(light)) lights.Add(light, new LightState(light));
        }

        internal void PrepareCamera(Camera camera, OceanSurfaceController ocean, Color? background = null)
        {
            if (camera == null || PlanarWaterReflection.IsReflectionCamera(camera)) return;
            if (!cameras.TryGetValue(camera, out var state))
            {
                state = new CameraState(camera);
                cameras.Add(camera, state);
            }
            state.Bind(camera, ocean);
            if (background.HasValue)
            {
                camera.depthTextureMode |= DepthTextureMode.Depth;
                camera.backgroundColor = background.Value;
            }
        }

        internal void Restore()
        {
            // Never copy one scene's lighting into a different active scene.
            if (HasSameActiveScene)
            {
                RenderSettings.fog = fog;
                RenderSettings.fogMode = fogMode;
                RenderSettings.fogColor = fogColour;
                RenderSettings.fogDensity = fogDensity;
                RenderSettings.ambientMode = ambientMode;
                RenderSettings.ambientLight = ambientLight;
                RenderSettings.sun = Sun;
            }
            foreach (var pair in lights) if (pair.Key != null) pair.Value.Restore(pair.Key);
            foreach (var pair in cameras) if (pair.Key != null) pair.Value.Restore(pair.Key);
            textures.Restore(); colours.Restore(); vectors.Restore(); floats.Restore();
        }

        private sealed class GlobalValues<T>
        {
            private readonly int[] ids;
            private readonly T[] values;
            private readonly Action<int, T> write;
            internal GlobalValues(Func<int, T> read, Action<int, T> write, params string[] names)
            {
                this.write = write;
                ids = new int[names.Length]; values = new T[names.Length];
                for (var i = 0; i < names.Length; i++) { ids[i] = Shader.PropertyToID(names[i]); values[i] = read(ids[i]); }
            }
            internal void Restore() { for (var i = 0; i < ids.Length; i++) write(ids[i], values[i]); }
        }

        private sealed class LightState
        {
            private readonly Vector3 position;
            private readonly Quaternion rotation;
            private readonly Color colour;
            private readonly float intensity;
            private readonly bool enabled;
            internal LightState(Light light)
            {
                position = light.transform.position; rotation = light.transform.rotation;
                colour = light.color; intensity = light.intensity; enabled = light.enabled;
            }
            internal void Restore(Light light)
            {
                light.transform.SetPositionAndRotation(position, rotation);
                light.color = colour; light.intensity = intensity; light.enabled = enabled;
            }
        }

        private sealed class CameraState
        {
            private readonly Color background;
            private readonly DepthTextureMode depth;
            private readonly PlanarWaterReflection reflection;
            private readonly Transform plane;
            private OceanUnderwaterView underwater;
            private OceanSurfaceController previousOcean;
            private bool addedUnderwater;
            internal CameraState(Camera camera)
            {
                background = camera.backgroundColor; depth = camera.depthTextureMode;
                reflection = camera.GetComponent<PlanarWaterReflection>();
                plane = reflection != null ? reflection.ReflectionPlane : null;
            }
            internal void Bind(Camera camera, OceanSurfaceController ocean)
            {
                if (ocean == null || ocean.SurfaceTransform == null) return;
                reflection?.Configure(ocean.SurfaceTransform);
                if (camera.cameraType != CameraType.Game) return;
                if (underwater == null)
                {
                    underwater = camera.GetComponent<OceanUnderwaterView>();
                    // Destroy is deferred in players; a same-frame re-enable must
                    // wait for that component to disappear before adding another.
                    if (underwater != null && underwater.PendingOwnerRemoval) { underwater = null; return; }
                    addedUnderwater = underwater == null;
                    if (addedUnderwater) underwater = camera.gameObject.AddComponent<OceanUnderwaterView>();
                    else previousOcean = underwater.Surface;
                }
                underwater.Configure(ocean);
            }
            internal void Restore(Camera camera)
            {
                if (reflection != null) reflection.Configure(plane);
                if (underwater != null)
                {
                    if (addedUnderwater)
                    {
                        underwater.PendingOwnerRemoval = true;
                        underwater.enabled = false;
                        underwater.Configure(null);
                        UnityObjectLifetime.DestroyUnityObject(underwater);
                    }
                    else underwater.Configure(previousOcean);
                }
                camera.backgroundColor = background; camera.depthTextureMode = depth;
            }
        }
    }
}
