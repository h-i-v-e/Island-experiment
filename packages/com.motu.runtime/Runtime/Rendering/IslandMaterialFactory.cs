using System;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Rendering
{
    internal static class IslandMaterialFactory
    {
        internal static Material CreateMaterial(
            string shaderName,
            Color color,
            Material template,
            float worldSize)
        {
            Material material;
            if (template != null)
            {
                material = new Material(template);
            }
            else
            {
                var shader = Motu.Rendering.MotuShaders.Find(shaderName);
                if (shader == null)
                {
                    throw new InvalidOperationException($"Could not find shader '{shaderName}'.");
                }
                material = new Material(shader);
                material.color = color;
                if (material.HasProperty("_BaseColor"))
                {
                    material.SetColor("_BaseColor", color);
                }
            }
            material.name = $"{shaderName} (Island Instance)";
            if (material.HasProperty("_WorldSize"))
            {
                material.SetFloat("_WorldSize", worldSize);
            }

            return material;
        }
        internal static Texture2D CreateSurfaceTexture(
            string textureName,
            int dimension,
            TextureFormat format,
            byte[] pixels)
        {
            var texture = new Texture2D(dimension, dimension, format, true, true)
            {
                name = textureName,
                filterMode = FilterMode.Trilinear,
                wrapMode = TextureWrapMode.Clamp,
                anisoLevel = 4,
            };
            try
            {
                // Rust supplies only mip 0. LoadRawTextureData expects storage for the
                // entire mip chain when the texture was created with mipmaps enabled.
                // Upload the base mip explicitly and let Apply generate the rest.
                texture.SetPixelData(pixels, 0);
                texture.Apply(true, true);
                return texture;
            }
            catch
            {
                DestroyUnityObject(texture);
                throw;
            }
        }
        // The native atlas has eight texels of extrapolated chart gutter. Only
        // base, half and quarter resolution fit that gutter; a full mip chain
        // would average unrelated charts. The last supplied mip also bounds the
        // hardware sampler's LOD, and anisotropy stays at one for the same reason.
        internal static Texture2D CreateSurfaceAtlasTexture(
            string textureName,
            int width,
            int height,
            TextureFormat format,
            byte[] pixels)
        {
            if (width <= 0 || height <= 0)
            {
                throw new ArgumentOutOfRangeException(nameof(width), "Surface atlas dimensions must be positive.");
            }
            var channels = format == TextureFormat.RGB24 ? 3
                : format == TextureFormat.R8 ? 1
                : throw new ArgumentException("Surface atlases require linear RGB24 or R8 pixels.", nameof(format));
            if (pixels == null || pixels.Length != checked(width * height * channels))
            {
                throw new ArgumentException("Surface atlas pixels do not match its dimensions.", nameof(pixels));
            }
            var mipCount = 1;
            for (var extent = Math.Max(width, height); mipCount < 3 && extent > 1; extent /= 2)
            {
                mipCount++;
            }
            var texture = new Texture2D(width, height, format, mipCount, true)
            {
                name = textureName,
                filterMode = FilterMode.Trilinear,
                wrapMode = TextureWrapMode.Clamp,
                anisoLevel = 1,
            };
            try
            {
                var mipPixels = pixels;
                var mipWidth = width;
                var mipHeight = height;
                for (var mip = 0; mip < mipCount; mip++)
                {
                    texture.SetPixelData(mipPixels, mip);
                    if (mip + 1 < mipCount)
                    {
                        mipPixels = DownsampleSurfaceAtlasMip(mipPixels, mipWidth, mipHeight, channels);
                        mipWidth = Math.Max(mipWidth / 2, 1);
                        mipHeight = Math.Max(mipHeight / 2, 1);
                    }
                }
                texture.Apply(false, true);
                return texture;
            }
            catch
            {
                DestroyUnityObject(texture);
                throw;
            }
        }

        private static byte[] DownsampleSurfaceAtlasMip(byte[] source, int width, int height, int channels)
        {
            var nextWidth = Math.Max(width / 2, 1);
            var nextHeight = Math.Max(height / 2, 1);
            var pixels = new byte[nextWidth * nextHeight * channels];
            for (var y = 0; y < nextHeight; y++)
            {
                for (var x = 0; x < nextWidth; x++)
                {
                    var firstX = x * width / nextWidth;
                    var lastX = (x + 1) * width / nextWidth;
                    var firstY = y * height / nextHeight;
                    var lastY = (y + 1) * height / nextHeight;
                    var samples = (lastX - firstX) * (lastY - firstY);
                    for (var channel = 0; channel < channels; channel++)
                    {
                        var total = 0;
                        for (var sourceY = firstY; sourceY < lastY; sourceY++)
                        {
                            for (var sourceX = firstX; sourceX < lastX; sourceX++)
                            {
                                total += source[(sourceY * width + sourceX) * channels + channel];
                            }
                        }
                        pixels[(y * nextWidth + x) * channels + channel] =
                            (byte)((total + samples / 2) / samples);
                    }
                }
            }
            return pixels;
        }

        internal static Texture2D CreateRuntimeMaterialTexture(
            string textureName,
            int width,
            int height,
            TextureFormat format,
            bool linear,
            byte[] pixels)
        {
            if (!SystemInfo.SupportsTextureFormat(format))
            {
                throw new InvalidOperationException(
                    $"This graphics device does not support runtime material format {format}.");
            }
            var texture = new Texture2D(width, height, format, true, linear)
            {
                name = textureName,
                filterMode = FilterMode.Trilinear,
                wrapMode = TextureWrapMode.Repeat,
                anisoLevel = 4,
            };
            try
            {
                texture.SetPixelData(pixels, 0);
                texture.Apply(true, true);
                return texture;
            }
            catch
            {
                DestroyUnityObject(texture);
                throw;
            }
        }
    }
}
