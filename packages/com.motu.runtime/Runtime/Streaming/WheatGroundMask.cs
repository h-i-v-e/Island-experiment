using System;
using System.Threading;
using Motu.Islands;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    internal sealed class WheatGroundMask
    {
        private const float MetresPerTexel = 0.25f;
        private const float EdgeFeatherMetres = 0.45f;
        private const int MaximumDimension = 2048;
        private const int PaddingTexels = 2;
        private readonly int width;
        private readonly int height;
        private readonly Vector4 uv;
        private byte[] pixels;
        internal bool HasExclusions { get; }

        private WheatGroundMask(int width, int height, Vector4 uv, byte[] pixels, bool hasExclusions)
        {
            this.width = width;
            this.height = height;
            this.uv = uv;
            this.pixels = pixels;
            HasExclusions = hasExclusions;
        }

        internal static WheatGroundMask Build(WheatField[] fields, CancellationToken token)
        {
            if (fields == null || fields.Length == 0) return null;

            var min = new Vector2(float.PositiveInfinity, float.PositiveInfinity);
            var max = new Vector2(float.NegativeInfinity, float.NegativeInfinity);
            bool hasExclusions = false;
            foreach (var field in fields)
            {
                token.ThrowIfCancellationRequested();
                if (!TryGetPlotBounds(field, out _, out var bounds)) continue;
                min = Vector2.Min(min, field.Centre - bounds);
                max = Vector2.Max(max, field.Centre + bounds);
                hasExclusions |= field.Exclusions.Length != 0;
            }

            var size = max - min;
            if (!float.IsFinite(size.x) || !float.IsFinite(size.y)) return null;

            var texel = Mathf.Max(MetresPerTexel,
                Mathf.Max(size.x, size.y) / (MaximumDimension - PaddingTexels * 2));
            int width = Mathf.Min(MaximumDimension, Mathf.CeilToInt(size.x / texel) + PaddingTexels * 2);
            int height = Mathf.Min(MaximumDimension, Mathf.CeilToInt(size.y / texel) + PaddingTexels * 2);
            var origin = min - Vector2.one * (texel * PaddingTexels);
            int channels = hasExclusions ? 2 : 1;
            var pixels = new byte[width * height * channels];
            foreach (var field in fields)
            {
                if (!TryGetPlotBounds(field, out var axis, out var bounds)) continue;
                Rasterize(field, axis, bounds, origin, texel, pixels, width, height, channels, token);
            }

            // Red is ground crop coverage, green is the shared canopy cutout.
            if (hasExclusions)
            {
                foreach (var field in fields)
                    foreach (var exclusion in field.Exclusions)
                        RasterizeExclusion(exclusion, origin, texel, pixels, width, height, token);
                for (int i = 0; i < pixels.Length; i += 2)
                {
                    if ((i & 16383) == 0) token.ThrowIfCancellationRequested();
                    pixels[i] = (byte)((pixels[i] * (255 - pixels[i + 1]) + 127) / 255);
                }
            }

            var scale = new Vector2(1f / (width * texel), 1f / (height * texel));
            var offset = Vector2.Scale(-origin, scale);
            return new WheatGroundMask(width, height, new Vector4(scale.x, scale.y, offset.x, offset.y), pixels, hasExclusions);
        }

        internal Texture2D Bind(params Material[] materials)
        {
            if (pixels == null) throw new InvalidOperationException("The wheat ground mask has already been uploaded.");

            var maskId = Shader.PropertyToID("_MotuWheatGroundMask");
            var uvId = Shader.PropertyToID("_MotuWheatGroundUV");
            var texture = new Texture2D(width, height, HasExclusions ? TextureFormat.RG16 : TextureFormat.R8, true, true)
            {
                name = "Island wheat ground coverage",
                wrapMode = TextureWrapMode.Clamp,
                filterMode = FilterMode.Trilinear
            };
            try
            {
                texture.SetPixelData(pixels, 0);
                texture.Apply(true, true);
                foreach (var material in materials)
                {
                    if (material == null || !material.HasProperty(maskId)) continue;
                    material.SetTexture(maskId, texture);
                    material.SetVector(uvId, uv);
                }
            }
            catch
            {
                DestroyUnityObject(texture);
                throw;
            }

            pixels = null;
            return texture;
        }

        private static bool TryGetPlotBounds(WheatField field, out Vector2 axis, out Vector2 bounds)
        {
            axis = default;
            bounds = default;
            if (!float.IsFinite(field.Centre.x) || !float.IsFinite(field.Centre.y)
                || !float.IsFinite(field.HalfExtents.x) || !float.IsFinite(field.HalfExtents.y)
                || field.HalfExtents.x <= 0f || field.HalfExtents.y <= 0f || !float.IsFinite(field.Yaw))
                return false;

            axis = new Vector2(Mathf.Cos(field.Yaw), Mathf.Sin(field.Yaw));
            float x = Mathf.Abs(axis.x);
            float z = Mathf.Abs(axis.y);
            bounds = new Vector2(
                x * field.HalfExtents.x + z * field.HalfExtents.y,
                z * field.HalfExtents.x + x * field.HalfExtents.y);
            return true;
        }

        private static void Rasterize(WheatField field, Vector2 axis, Vector2 bounds, Vector2 origin,
            float texel, byte[] pixels, int width, int height, int channels, CancellationToken token)
        {
            var min = (field.Centre - bounds - origin) / texel;
            var max = (field.Centre + bounds - origin) / texel;
            int startX = Mathf.Clamp(Mathf.FloorToInt(min.x), 0, width);
            int startY = Mathf.Clamp(Mathf.FloorToInt(min.y), 0, height);
            int endX = Mathf.Clamp(Mathf.CeilToInt(max.x), 0, width);
            int endY = Mathf.Clamp(Mathf.CeilToInt(max.y), 0, height);
            float inverseFeather = 1f / Mathf.Max(EdgeFeatherMetres, texel);

            for (int y = startY; y < endY; y++)
            {
                token.ThrowIfCancellationRequested();
                float z = origin.y + (y + 0.5f) * texel - field.Centre.y;
                for (int x = startX; x < endX; x++)
                {
                    float horizontal = origin.x + (x + 0.5f) * texel - field.Centre.x;
                    float along = horizontal * axis.x + z * axis.y;
                    float across = z * axis.x - horizontal * axis.y;
                    var inward = (float)field.InwardDistance(along, across);
                    byte coverage = (byte)Mathf.RoundToInt(Mathf.Clamp01(inward * inverseFeather) * 255f);
                    int index = (y * width + x) * channels;
                    if (coverage > pixels[index]) pixels[index] = coverage;
                }
            }
        }

        private static void RasterizeExclusion(WheatExclusion exclusion, Vector2 origin, float texel,
            byte[] pixels, int width, int height, CancellationToken token)
        {
            // Give narrow trunk cuts a texel of support even when plots span the full island.
            float radius = exclusion.Radius + texel * 0.5f;
            var centre = (exclusion.Centre - origin) / texel;
            float extent = radius / texel;
            int startX = Mathf.Max(0, Mathf.FloorToInt(centre.x - extent));
            int startY = Mathf.Max(0, Mathf.FloorToInt(centre.y - extent));
            int endX = Mathf.Min(width, Mathf.CeilToInt(centre.x + extent));
            int endY = Mathf.Min(height, Mathf.CeilToInt(centre.y + extent));
            for (int y = startY; y < endY; y++)
            {
                token.ThrowIfCancellationRequested();
                for (int x = startX; x < endX; x++)
                {
                    float distance = (new Vector2(x + 0.5f, y + 0.5f) - centre).magnitude;
                    byte coverage = (byte)Mathf.RoundToInt(Mathf.Clamp01((extent - distance) * 2f) * 255f);
                    int index = 2 * (y * width + x) + 1;
                    if (coverage > pixels[index]) pixels[index] = coverage;
                }
            }
        }
    }
}
