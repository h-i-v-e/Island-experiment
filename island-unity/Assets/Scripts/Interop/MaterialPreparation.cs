using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using UnityEngine;
using Debug = UnityEngine.Debug;
using Motu.Interop;
using Motu.Rendering;
using Motu.Streaming;

using Motu.Islands;
using static Motu.Interop.IslandMeshInterop;
using static Motu.Interop.NativeExportValidation;
namespace Motu.Interop
{
    internal static class MaterialPreparation
    {
        internal static IslandPreparedMaterialTextures PrepareMaterialTextures(
            IslandMaterialColours colours,
            int resolution,
            long cacheBudgetBytes = 0L,
            string cacheDirectory = null)
        {
            var cached = IslandMaterialTextureCache.TryLoad(
                colours,
                resolution,
                cacheBudgetBytes,
                cacheDirectory);
            if (cached != null)
            {
                return cached;
            }
            var inputs = colours.ToNative();
            var options = new MotuNative.MaterialBakeOptions
            {
                width = checked((uint)resolution),
                height = checked((uint)resolution),
                normalConvention = 1,
                materialMask = 0x7f,
                reserved = 0,
            };
            var succeeded = MotuNative.BakeMotuMaterialTextures(
                ref inputs,
                ref options,
                out var textures);
            try
            {
                if (succeeded == 0 || textures.handle == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "The Rust procedural material library could not bake the requested textures.");
                }
                var result = new IslandPreparedMaterialTextures(
                    colours,
                    CopyMaterialTexture(textures.dirt, resolution, "dirt"),
                    CopyMaterialTexture(textures.forestFloor, resolution, "forest floor"),
                    CopyMaterialTexture(textures.rock, resolution, "rock"),
                    CopyMaterialTexture(textures.riverBed, resolution, "river bed"),
                    CopyMaterialTexture(textures.beach, resolution, "beach"),
                    CopyMaterialTexture(textures.fallenStones, resolution, "fallen stones"),
                    CopyMaterialTexture(textures.treeBark, resolution, "tree bark"));
                IslandMaterialTextureCache.TrySave(
                    result,
                    cacheBudgetBytes,
                    cacheDirectory);
                return result;
            }
            finally
            {
                MotuNative.ReleaseMaterialTextureSet(ref textures);
            }
        }
        internal static IslandPreparedMaterialTexture PrepareTreeBarkTexture(int resolution)
        {
            var inputs = new IslandMaterialColours(Color.black, Color.black, Color.black).ToNative();
            var options = new MotuNative.MaterialBakeOptions
            {
                width = checked((uint)resolution),
                height = checked((uint)resolution),
                normalConvention = 1,
                materialMask = 0x40,
                reserved = 0,
            };
            var succeeded = MotuNative.BakeMotuMaterialTextures(
                ref inputs,
                ref options,
                out var textures);
            try
            {
                if (succeeded == 0 || textures.handle == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "The Rust procedural material library could not bake tree bark.");
                }
                return CopyMaterialTexture(textures.treeBark, resolution, "tree bark");
            }
            finally
            {
                MotuNative.ReleaseMaterialTextureSet(ref textures);
            }
        }
        internal static IslandPreparedMaterialTexture CopyMaterialTexture(
            MotuNative.ExportMaterialTexture source,
            int resolution,
            string label)
        {
            var pixels = checked(resolution * resolution);
            if (source.width != resolution
                || source.height != resolution
                || !float.IsFinite(source.physicalTileWidthMetres)
                || source.physicalTileWidthMetres <= 0f
                || !float.IsFinite(source.physicalTileHeightMetres)
                || source.physicalTileHeightMetres <= 0f
                || float.IsNaN(source.minimumHeight)
                || float.IsInfinity(source.minimumHeight)
                || float.IsNaN(source.maximumHeight)
                || float.IsInfinity(source.maximumHeight)
                || float.IsNaN(source.baseHeight)
                || float.IsInfinity(source.baseHeight)
                || source.maximumHeight <= source.minimumHeight
                || source.baseHeight < source.minimumHeight
                || source.baseHeight > source.maximumHeight
                || source.albedoRgb.data == IntPtr.Zero
                || source.albedoRgb.length != checked(pixels * 3)
                || source.normalRgb.data == IntPtr.Zero
                || source.normalRgb.length != checked(pixels * 3)
                || source.heightR16.data == IntPtr.Zero
                || source.heightR16.length != checked(pixels * 2)
                || source.occlusion.data == IntPtr.Zero
                || source.occlusion.length != pixels)
            {
                throw new InvalidOperationException(
                    $"The Rust generator returned invalid {label} material textures.");
            }

            var albedo = new byte[source.albedoRgb.length];
            var normal = new byte[source.normalRgb.length];
            var height = new byte[source.heightR16.length];
            var occlusion = new byte[source.occlusion.length];
            Marshal.Copy(source.albedoRgb.data, albedo, 0, albedo.Length);
            Marshal.Copy(source.normalRgb.data, normal, 0, normal.Length);
            Marshal.Copy(source.heightR16.data, height, 0, height.Length);
            Marshal.Copy(source.occlusion.data, occlusion, 0, occlusion.Length);
            return new IslandPreparedMaterialTexture(
                resolution,
                resolution,
                source.physicalTileWidthMetres,
                source.physicalTileHeightMetres,
                source.minimumHeight,
                source.maximumHeight,
                source.baseHeight,
                albedo,
                normal,
                height,
                occlusion);
        }
    }
}
