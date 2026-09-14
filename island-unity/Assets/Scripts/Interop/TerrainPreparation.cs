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
        internal static IslandPreparedBoulderCollider[][] PrepareBoulderColliders(IntPtr handle, float worldSize)
        {
            MotuNative.CreateBoulderColliders(handle, out var export);
            try
            {
                if (export.handle == IntPtr.Zero || export.length < 0
                    || (export.length > 0 && export.data == IntPtr.Zero))
                    throw new InvalidOperationException("The native boulder collider export is invalid.");
                var packed = new float[checked(export.length * 4)];
                if (packed.Length != 0) Marshal.Copy(export.data, packed, 0, packed.Length);
                var spheres = new Vector4[export.length];
                for (var i = 0; i < spheres.Length; i++)
                    spheres[i] = new Vector4(packed[i * 4], packed[i * 4 + 1], packed[i * 4 + 2], packed[i * 4 + 3]);
                return BucketBoulderColliders(spheres, worldSize);
            }
            finally { MotuNative.ReleaseBoulderColliders(ref export); }
        }

        internal static IslandPreparedBoulderCollider[][] BucketBoulderColliders(Vector4[] spheres, float worldSize)
        {
            const int resolution = TerrainTileStreamer.Lod1Resolution;
            var buckets = new List<IslandPreparedBoulderCollider>[resolution * resolution];
            foreach (var sphere in spheres)
            {
                if (!IsFinite(sphere.x) || !IsFinite(sphere.y) || !IsFinite(sphere.z) || !IsFinite(sphere.w)
                    || sphere.w <= 0 || sphere.x < 0 || sphere.x > 1 || sphere.y < 0 || sphere.y > 1)
                    throw new InvalidOperationException("The native boulder sphere is invalid.");
                var x = Mathf.Min(Mathf.FloorToInt(sphere.x * resolution), resolution - 1);
                var y = Mathf.Min(Mathf.FloorToInt(sphere.y * resolution), resolution - 1);
                var index = y * resolution + x;
                buckets[index] ??= new List<IslandPreparedBoulderCollider>();
                buckets[index].Add(new IslandPreparedBoulderCollider(
                    new Vector3((sphere.x - .5f) * worldSize, sphere.z * worldSize,
                        (sphere.y - .5f) * worldSize), sphere.w * worldSize));
            }
            var result = new IslandPreparedBoulderCollider[buckets.Length][];
            for (var i = 0; i < result.Length; i++)
                result[i] = buckets[i]?.ToArray() ?? Array.Empty<IslandPreparedBoulderCollider>();
            return result;
        }

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
