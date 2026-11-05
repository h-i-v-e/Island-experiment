using System;
using Motu.Interop;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // Two shared arrays retain all three species without splitting instanced draws.
    // Albedo RGB + height A; tangent normal RGB + occlusion A.
    internal sealed class PlanetForestBark : IDisposable
    {
        internal readonly Texture2DArray albedoHeight, normalOcclusion;
        private readonly Vector4 tileSizes;
        private readonly Vector4 palmTileSize;

        internal PlanetForestBark()
        {
            var maps = HybridForestNative.SharedBarkMaps;
            var width = maps[0].width;
            var height = maps[0].height;
            albedoHeight = MakeArray("Shared forest bark albedo and height", width, height, false);
            try
            {
                normalOcclusion = MakeArray("Shared forest bark normals and occlusion", width, height, true);
                for (var species = 0; species < 3; ++species)
                {
                    var source = maps[species];
                    if (source.width != width || source.height != height)
                        throw new InvalidOperationException("Forest bark maps must share an extent.");
                    var surface = new Color32[width * height];
                    var normals = new Color32[surface.Length];
                    for (var p = 0; p < surface.Length; ++p)
                    {
                        surface[p] = new Color32(source.albedoRgb[p * 3], source.albedoRgb[p * 3 + 1],
                            source.albedoRgb[p * 3 + 2], source.heightR16[p * 2 + 1]);
                        normals[p] = new Color32(source.normalRgb[p * 3], source.normalRgb[p * 3 + 1],
                            source.normalRgb[p * 3 + 2], source.occlusion[p]);
                    }
                    albedoHeight.SetPixels32(surface, species);
                    normalOcclusion.SetPixels32(normals, species);
                }
                albedoHeight.Apply(true, true);
                normalOcclusion.Apply(true, true);
                tileSizes = new Vector4(maps[0].physicalTileWidthMetres, maps[0].physicalTileHeightMetres,
                    maps[1].physicalTileWidthMetres, maps[1].physicalTileHeightMetres);
                palmTileSize = new Vector4(maps[2].physicalTileWidthMetres, maps[2].physicalTileHeightMetres, 0, 0);
            }
            catch { Dispose(); throw; }
        }

        private static Texture2DArray MakeArray(string name, int width, int height, bool linear)
            => new Texture2DArray(width, height, 3, TextureFormat.RGBA32, true, linear)
            { name = name, wrapMode = TextureWrapMode.Repeat, filterMode = FilterMode.Trilinear, anisoLevel = 4 };

        internal void Bind(Material material)
        {
            material.SetTexture("_ForestBarkAlbedoHeight", albedoHeight);
            material.SetTexture("_ForestBarkNormalOcclusion", normalOcclusion);
            material.SetVector("_ForestBarkTileSizes", tileSizes);
            material.SetVector("_ForestPalmBarkTileSize", palmTileSize);
        }

        public void Dispose()
        {
            DestroyUnityObject(albedoHeight);
            DestroyUnityObject(normalOcclusion);
        }
    }
}
