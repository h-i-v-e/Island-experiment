using System;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.World
{
    // A small, reusable GPU query batch. One in-flight request per floating body;
    // never waits for the GPU on the physics thread.
    internal sealed class OceanSurfaceSampler : IDisposable
    {
        private readonly Material material;
        private readonly Texture2D positions;
        private readonly RenderTexture results;
        private readonly Color[] pixels;
        private readonly Vector3[] submittedPositions;
        private readonly Vector3[] sampledPositions;
        private readonly float[] heights;
        private readonly float[] sampleTimes;
        private readonly Action<AsyncGPUReadbackRequest> onReadback;
        private bool pending;
        private bool disposed;
        private float submittedTime;

        internal int Count => heights.Length;
        internal bool Pending => pending;
        internal static bool Supported => SystemInfo.supportsAsyncGPUReadback
            && SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat)
            && SystemInfo.SupportsTextureFormat(TextureFormat.RGBAFloat);

        internal OceanSurfaceSampler(int count)
        {
            if (count < 1 || count > 64) throw new ArgumentOutOfRangeException(nameof(count));
            if (!Supported) throw new NotSupportedException("Ocean buoyancy requires float textures and asynchronous GPU readback.");
            var shader = Resources.Load<Shader>("OceanSurfaceQuery");
            if (shader == null || !shader.isSupported)
                throw new NotSupportedException("The ocean surface query shader is unavailable on this graphics device.");
            material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
            positions = new Texture2D(count, 1, TextureFormat.RGBAFloat, false, true)
            { filterMode = FilterMode.Point, wrapMode = TextureWrapMode.Clamp, hideFlags = HideFlags.HideAndDontSave };
            results = new RenderTexture(count, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear)
            { hideFlags = HideFlags.HideAndDontSave };
            results.Create();
            pixels = new Color[count];
            submittedPositions = new Vector3[count];
            sampledPositions = new Vector3[count];
            heights = new float[count];
            sampleTimes = new float[count];
            for (var i = 0; i < count; i++) sampleTimes[i] = float.NegativeInfinity;
            onReadback = Complete;
        }

        internal void Submit(OceanSurfaceController ocean, Vector3[] worldPositions)
        {
            if (disposed || pending || ocean == null || ocean.SurfaceMaterial == null
                || ocean.SurfaceTransform == null) return;
            if (worldPositions.Length != Count) throw new ArgumentException("Probe count changed.", nameof(worldPositions));
            for (var i = 0; i < Count; i++)
            {
                var p = worldPositions[i];
                submittedPositions[i] = p;
                pixels[i] = new Color(p.x, p.y, p.z, 1f);
            }
            positions.SetPixels(pixels);
            positions.Apply(false, false);
            material.CopyPropertiesFromMaterial(ocean.SurfaceMaterial);
            material.SetTexture("_QueryPositions", positions);
            material.SetVector("_QueryOceanOrigin", ocean.SurfaceTransform.position);
            // Snapshot global wind too: the query must use the same field as the
            // surface, even when a weather driver changes it the following frame.
            material.SetTexture("_MotuWindNoise", Shader.GetGlobalTexture("_MotuWindNoise") ?? Texture2D.grayTexture);
            material.SetVector("_MotuWeatherWind", Shader.GetGlobalVector("_MotuWeatherWind"));
            material.SetVector("_MotuWindOffset", Shader.GetGlobalVector("_MotuWindOffset"));
            var previous = RenderTexture.active;
            Graphics.Blit(Texture2D.blackTexture, results, material);
            RenderTexture.active = previous;
            submittedTime = Time.time;
            pending = true;
            AsyncGPUReadback.Request(results, 0, TextureFormat.RGBAFloat, onReadback);
        }

        internal bool TryGetHeight(int index, Vector3 currentPosition, float maximumTravel, out float height)
            => TryGetHeight(index, currentPosition, maximumTravel, out height, out _);

        internal bool TryGetHeight(int index, Vector3 currentPosition, float maximumTravel,
            out float height, out float confidence)
        {
            height = heights[index];
            confidence = SampleConfidence(Time.time - sampleTimes[index]);
            var offset = currentPosition - sampledPositions[index];
            offset.y = 0f;
            return !disposed && confidence > 0f
                && offset.sqrMagnitude <= maximumTravel * maximumTravel
                && !float.IsNaN(height) && !float.IsInfinity(height);
        }

        internal static float SampleConfidence(float age)
        {
            // Hold through an ordinary delayed frame, then gradually lose support.
            // The former 0.25 s cutoff toggled individual probe forces on and off.
            return 1f - Mathf.SmoothStep(0f, 1f, Mathf.InverseLerp(.25f, 1f, age));
        }

        private void Complete(AsyncGPUReadbackRequest request)
        {
            pending = false;
            if (disposed) { Release(); return; }
            // A failed readback should not discard the last usable water surface.
            // Each probe ages independently until a successful sample replaces it.
            if (request.hasError) return;
            var data = request.GetData<Color>();
            for (var i = 0; i < Count; i++)
            {
                // Folded/extremely steep surfaces need not have a unique height.
                // Reject a failed inversion instead of applying a wild impulse.
                if (data[i].g > .1f || float.IsNaN(data[i].g)
                    || float.IsNaN(data[i].r) || float.IsInfinity(data[i].r)) continue;
                heights[i] = data[i].r;
                sampledPositions[i] = submittedPositions[i];
                sampleTimes[i] = submittedTime;
            }
        }

        public void Dispose()
        {
            if (disposed) return;
            disposed = true;
            // Readback retains the render texture until its callback completes.
            if (!pending) Release();
        }

        private void Release()
        {
            results.Release();
            UnityObjectLifetime.DestroyUnityObject(results);
            UnityObjectLifetime.DestroyUnityObject(positions);
            UnityObjectLifetime.DestroyUnityObject(material);
        }
    }
}
