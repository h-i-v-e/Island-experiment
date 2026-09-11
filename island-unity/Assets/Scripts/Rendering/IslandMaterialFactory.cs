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
                var shader = Shader.Find(shaderName) ?? Shader.Find("Standard");
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
