using System;
using Motu.Interop;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // Uploads the shared Rust bake once per island. Only the GPU copy survives
    // installation; the renderer never generates circles or retains CPU pixels.
    internal sealed class ForestShadowMask : IDisposable
    {
        private static readonly int MaskId = Shader.PropertyToID("_MotuForestShadowMask");
        private static readonly int UvId = Shader.PropertyToID("_MotuForestShadowUV");
        private static readonly int StrengthId = Shader.PropertyToID("_MotuForestShadowStrength");
        private readonly Material[] materials;
        internal Texture2D Texture { get; private set; }

        internal ForestShadowMask(HybridForestData forest, params Material[] materials)
        {
            this.materials = materials;
            var resolution = forest.shadowResolution;
            Texture = new Texture2D(resolution, resolution, TextureFormat.R8, true, true)
            {
                name = "Island overhead forest shadows",
                wrapMode = TextureWrapMode.Clamp,
                filterMode = FilterMode.Trilinear,
                anisoLevel = 2
            };
            Texture.SetPixelData(forest.shadowPixels, 0);
            // Level zero is strictly binary. Filtered mips preserve coverage at sea distance.
            Texture.Apply(true, true);
            var uv = new Vector4(1f / forest.worldSize, 1f / forest.worldSize, .5f, .5f);
            foreach (var material in materials)
            {
                if (material == null) continue;
                material.SetTexture(MaskId, Texture);
                material.SetVector(UvId, uv);
            }
        }

        internal void SetVisible(bool visible)
        {
            foreach (var material in materials)
                if (material != null) material.SetFloat(StrengthId, visible ? 1f : 0f);
        }

        public void Dispose()
        {
            foreach (var material in materials)
                if (material != null && material.GetTexture(MaskId) == Texture)
                {
                    material.SetTexture(MaskId, null);
                    material.SetFloat(StrengthId, 0f);
                }
            DestroyUnityObject(Texture);
            Texture = null;
        }
    }
}
