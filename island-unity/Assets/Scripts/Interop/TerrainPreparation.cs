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
    internal static class TerrainPreparation
    {
        internal static IslandPreparedSurfaceMaps PrepareSurfaceMaps(
            IntPtr handle,
            int dimension)
        {
            MotuNative.CreateSurfaceMaps(handle, 0, dimension, out var surfaceMaps);
            try
            {
                if (surfaceMaps.handle == IntPtr.Zero
                    || surfaceMaps.occlusion == IntPtr.Zero
                    || surfaceMaps.normalRgb == IntPtr.Zero
                    || surfaceMaps.width != dimension
                    || surfaceMaps.height != dimension)
                {
                    throw new InvalidOperationException(
                        "The Rust generator returned invalid terrain surface maps.");
                }

                var pixelCount = checked(dimension * dimension);
                var occlusionBytes = new byte[pixelCount];
                Marshal.Copy(surfaceMaps.occlusion, occlusionBytes, 0, occlusionBytes.Length);
                var normalBytes = new byte[checked(pixelCount * 3)];
                Marshal.Copy(surfaceMaps.normalRgb, normalBytes, 0, normalBytes.Length);
                return new IslandPreparedSurfaceMaps(dimension, normalBytes, occlusionBytes);
            }
            finally
            {
                MotuNative.ReleaseSurfaceMaps(ref surfaceMaps);
            }
        }
        internal static IslandPreparedColliderHeightMap PrepareColliderHeightMap(
            IntPtr handle,
            float terrainWorldSize)
        {
            var mapPointer = MotuNative.CreateTerrainColliderHeightMap(
                handle,
                TerrainTileStreamer.ColliderSamplesPerTile);
            try
            {
                if (mapPointer == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "The Rust generator returned a null terrain-collider height map.");
                }
                var native = Marshal.PtrToStructure<MotuNative.ExportHeightMapWithSeaLevel>(
                    mapPointer);
                var expectedDimension = checked(
                    TerrainTileStreamer.Lod1Resolution
                    * (TerrainTileStreamer.ColliderSamplesPerTile - 1)
                    + 1);
                if (native.data == IntPtr.Zero
                    || native.width != expectedDimension
                    || native.height != expectedDimension)
                {
                    throw new InvalidOperationException(
                        "The Rust generator returned invalid terrain-collider height-map data.");
                }

                var heights = new float[checked(native.width * native.height)];
                Marshal.Copy(native.data, heights, 0, heights.Length);
                return new IslandPreparedColliderHeightMap(
                    native.width,
                    TerrainTileStreamer.ColliderSamplesPerTile,
                    heights,
                    terrainWorldSize);
            }
            finally
            {
                MotuNative.ReleaseTerrainColliderHeightMap(mapPointer);
            }
        }
    }
}
