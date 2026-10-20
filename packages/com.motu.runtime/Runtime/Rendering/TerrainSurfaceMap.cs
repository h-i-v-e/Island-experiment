using System;
using Motu.World;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Rendering
{
    /// <summary>A once-per-island asynchronous bake of the terrain shader's surface coverage.</summary>
    internal sealed class TerrainSurfaceMap : IDisposable
    {
        const int Resolution = 1024;
        readonly Transform owner;
        readonly float worldSize;
        Material material;
        RenderTexture target;
        byte[] kinds;
        float[] heights;
        bool pending, disposed;

        internal TerrainSurfaceMap(Transform owner, float worldSize, Material terrainMaterial)
        {
            this.owner = owner; this.worldSize = worldSize;
            var shader = Resources.Load<Shader>("TerrainSurfaceQuery");
            if (!SystemInfo.supportsAsyncGPUReadback || !SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat)
                || shader == null || !shader.isSupported) return;
            material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
            material.CopyPropertiesFromMaterial(terrainMaterial);
            material.SetFloat("_SurfaceWorldSize", worldSize);
            material.SetFloat("_SurfaceUseNormalMap", terrainMaterial.GetTexture("_WorldNormal") != null ? 1f : 0f);
            target = new RenderTexture(Resolution, Resolution, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear)
            { hideFlags = HideFlags.HideAndDontSave };
            target.Create();
        }

        internal void Bake(Action drawMeshes)
        {
            if (disposed || material == null || pending || kinds != null) return;
            var previous = RenderTexture.active;
            bool previousSrgb = GL.sRGBWrite;
            try
            {
                RenderTexture.active = target; GL.sRGBWrite = false;
                GL.Clear(false, true, Color.clear);
                if (!material.SetPass(0)) return;
                drawMeshes();
                pending = true;
                AsyncGPUReadback.Request(target, 0, TextureFormat.RGBAFloat, Complete);
            }
            finally { RenderTexture.active = previous; GL.sRGBWrite = previousSrgb; }
        }

        void Complete(AsyncGPUReadbackRequest request)
        {
            pending = false;
            if (!disposed && !request.hasError)
            {
                var data = request.GetData<Color>();
                kinds = new byte[data.Length]; heights = new float[data.Length];
                for (int i = 0; i < data.Length; i++)
                {
                    if (!float.IsFinite(data[i].r) || !float.IsFinite(data[i].g)) continue;
                    kinds[i] = (byte)Mathf.Clamp(Mathf.RoundToInt(data[i].r), 0, 7);
                    heights[i] = data[i].g;
                }
            }
            ReleaseGpu();
        }

        internal bool TryGetSample(Vector3 worldPosition, out TerrainSurfaceSample sample)
        {
            sample = default;
            if (disposed || kinds == null || owner == null) return false;
            var local = owner.InverseTransformPoint(worldPosition);
            float u = local.x / worldSize + .5f, v = local.z / worldSize + .5f;
            if (u < 0 || v < 0 || u > 1 || v > 1) return false;
            int i = Mathf.Min((int)(v * Resolution), Resolution - 1) * Resolution
                + Mathf.Min((int)(u * Resolution), Resolution - 1);
            if (kinds[i] == 0) return false;
            float height = owner.TransformPoint(new Vector3(local.x, heights[i], local.z)).y;
            sample = new TerrainSurfaceSample((TerrainSurfaceKind)(kinds[i] - 1), height, owner.position.y);
            return true;
        }

        public void Dispose()
        {
            disposed = true; kinds = null; heights = null;
            if (!pending) ReleaseGpu();
        }
        void ReleaseGpu()
        {
            if (target != null) target.Release();
            UnityObjectLifetime.DestroyUnityObject(target); target = null;
            UnityObjectLifetime.DestroyUnityObject(material); material = null;
        }
    }
}
