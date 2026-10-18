using System;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.World
{
    /// <summary>Small asynchronous batches using the rendered coastal depth and shore-wave field.</summary>
    public sealed class OceanShoreSampler : IDisposable
    {
        public readonly struct Sample
        {
            public readonly Vector3 Position;
            public readonly float Depth, Breaking, Influence, Time;
            /// <summary>Local shore-wave amplitude in metres, including wind, depth, river and island-shelter attenuation.</summary>
            public readonly float Amplitude;
            /// <summary>Retained island-shelter wave fraction. Apply after volume saturation so large sheltered waves also sound quieter.</summary>
            public readonly float WaveScale;
            internal Sample(Vector3 position, float depth, float breaking, float influence, float time, float amplitude, float waveScale)
            { Position = position; Depth = depth; Breaking = breaking; Influence = influence; Time = time; Amplitude = amplitude; WaveScale = waveScale; }
        }

        private readonly Material material;
        private readonly Texture2D positions;
        private readonly RenderTexture results;
        private readonly Color[] pixels;
        private readonly Vector3[] submitted;
        private readonly Sample[] samples;
        private readonly Action<AsyncGPUReadbackRequest> onReadback;
        private bool pending, disposed, placement;
        private float submittedTime, seaLevel;
        public int Count => samples.Length;
        public int Revision { get; private set; }
        public static bool Supported => OceanSurfaceSampler.Supported;

        public OceanShoreSampler(int count)
        {
            if (count < 1 || count > 64) throw new ArgumentOutOfRangeException(nameof(count));
            if (!Supported) throw new NotSupportedException("Shore audio requires asynchronous float-texture GPU sampling.");
            var shader = Resources.Load<Shader>("OceanShoreQuery");
            if (shader == null || !shader.isSupported) throw new NotSupportedException("The shore query shader is unavailable.");
            material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
            positions = new Texture2D(count, 1, TextureFormat.RGBAFloat, false, true)
            { filterMode = FilterMode.Point, wrapMode = TextureWrapMode.Clamp, hideFlags = HideFlags.HideAndDontSave };
            // First row: contact/trigger data. Second row: local amplitude.
            results = new RenderTexture(count, 2, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear)
            { hideFlags = HideFlags.HideAndDontSave };
            results.Create();
            pixels = new Color[count]; submitted = new Vector3[count]; samples = new Sample[count];
            onReadback = Complete;
        }

        public bool SubmitPlacement(OceanSurfaceController ocean, Vector3 centre, float radiusMetres)
            => Submit(ocean, null, centre, Mathf.Clamp(radiusMetres, 1f, 128f));

        public bool SubmitWaves(OceanSurfaceController ocean, Vector3[] worldPositions)
        {
            if (worldPositions == null || worldPositions.Length != Count)
                throw new ArgumentException("Shore probe count changed.", nameof(worldPositions));
            return Submit(ocean, worldPositions, default, 0f);
        }

        private bool Submit(OceanSurfaceController ocean, Vector3[] worldPositions, Vector3 centre, float radius)
        {
            if (disposed || pending || ocean == null || !ocean.isActiveAndEnabled
                || ocean.SurfaceMaterial == null || ocean.SurfaceTransform == null) return false;
            placement = worldPositions == null;
            seaLevel = ocean.SurfaceTransform.position.y;
            if (!placement)
            {
                for (int i = 0; i < Count; i++)
                {
                    submitted[i] = worldPositions[i];
                    pixels[i] = new Color(submitted[i].x, 0, submitted[i].z, 1);
                }
                positions.SetPixels(pixels); positions.Apply(false, false);
            }
            material.CopyPropertiesFromMaterial(ocean.SurfaceMaterial);
            material.SetTexture("_QueryPositions", positions);
            material.SetVector("_ShoreQueryCentre", new Vector4(centre.x, seaLevel, centre.z, 0));
            material.SetFloat("_ShoreQueryRadius", radius);
            material.SetFloat("_ShoreQueryCount", Count);
            material.SetFloat("_ShoreQueryPlacement", placement ? 1f : 0f);
            material.SetVector("_MotuWeatherWind", Shader.GetGlobalVector("_MotuWeatherWind"));
            var previous = RenderTexture.active;
            Graphics.Blit(Texture2D.blackTexture, results, material);
            RenderTexture.active = previous;
            submittedTime = UnityEngine.Time.time; pending = true;
            AsyncGPUReadback.Request(results, 0, TextureFormat.RGBAFloat, onReadback);
            return true;
        }

        public bool TryGetSample(int index, out Sample sample)
        {
            sample = samples[index];
            return !disposed && Revision > 0 && sample.Influence > .001f
                && UnityEngine.Time.time - sample.Time < (placement ? 2f : .5f);
        }

        private void Complete(AsyncGPUReadbackRequest request)
        {
            pending = false;
            if (disposed) { Release(); return; }
            if (request.hasError) return;
            var data = request.GetData<Color>();
            for (int i = 0; i < Count; i++)
            {
                var value = data[i];
                float amplitude = placement ? 0f : data[Count + i].r;
                float waveScale = placement ? 1f : data[Count + i].g;
                if (!float.IsFinite(value.r) || !float.IsFinite(value.g) || !float.IsFinite(value.b)
                    || !float.IsFinite(value.a) || !float.IsFinite(amplitude)
                    || !float.IsFinite(waveScale)) { samples[i] = default; continue; }
                var position = placement ? new Vector3(value.r, seaLevel, value.g)
                    : new Vector3(submitted[i].x, value.r, submitted[i].z);
                samples[i] = new Sample(position, value.b, placement ? 0f : value.g, value.a,
                    submittedTime, Mathf.Max(0f, amplitude), Mathf.Clamp01(waveScale));
            }
            Revision++;
        }

        public void Dispose()
        {
            if (disposed) return;
            disposed = true;
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
