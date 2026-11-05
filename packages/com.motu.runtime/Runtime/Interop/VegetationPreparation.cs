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
    internal static class VegetationPreparation
    {
        internal static IslandPreparedForestData PrepareForestData(
            IntPtr handle,
            float worldSize)
        {
            return new IslandPreparedForestData(
                HybridForestNative.Prepare(handle, worldSize),
                PrepareForestTrunkColliders(handle, worldSize),
                PrepareForestTrunkColliders(handle, worldSize, fallenLogs: true),
                HybridForestNative.PrepareFallenLogs(handle, worldSize));
        }
        internal static IslandPreparedMesh[] PrepareFernMeshGrid(
            IntPtr handle,
            float worldSize)
        {
            MotuNative.CreateFernMeshGrid(handle, out var export);
            try
            {
                if (export.handle == IntPtr.Zero
                    || export.data == IntPtr.Zero
                    || export.length != FernTileStreamer.TileCount)
                {
                    throw new InvalidOperationException(
                        "The Rust fern owner grid returned an invalid batch.");
                }

                var result = new IslandPreparedMesh[export.length];
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < export.length; index++)
                {
                    var native = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(export.data, index * exportSize));
                    if (native.vertices.length == 0
                        && native.normals.length == 0
                        && native.triangles.length == 0)
                    {
                        if (native.uv.length != 0
                            || native.material.length != 0
                            || native.environment.length != 0)
                        {
                            throw new InvalidOperationException(
                                $"The empty Rust fern tile {index} contains sidecar data.");
                        }
                        continue;
                    }
                    if (native.vertices.length <= 0
                        || native.vertices.data == IntPtr.Zero
                        || native.normals.length != native.vertices.length
                        || native.normals.data == IntPtr.Zero
                        || native.uv.length != native.vertices.length
                        || native.uv.data == IntPtr.Zero
                        || native.material.length != native.vertices.length
                        || native.material.data == IntPtr.Zero
                        || native.environment.length != native.vertices.length
                        || native.environment.data == IntPtr.Zero
                        || native.triangles.length <= 0
                        || native.triangles.length % 3 != 0
                        || native.triangles.data == IntPtr.Zero)
                    {
                        throw new InvalidOperationException(
                            $"The Rust fern tile {index} has invalid mesh attributes.");
                    }
                    result[index] = IslandMeshInterop.CopyGeneratedMeshData(
                        native,
                        worldSize);
                }
                return result;
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref export);
            }
        }
        internal static IslandPreparedTreeCollider[][] PrepareForestTrunkColliders(
            IntPtr handle,
            float worldSize,
            bool fallenLogs = false)
        {
            MotuNative.ExportForestTrunkColliders export;
            if (fallenLogs) MotuNative.CreateForestLogColliders(handle, out export);
            else MotuNative.CreateForestTrunkColliders(handle, out export);
            try
            {
                if (export.handle == IntPtr.Zero
                    || export.length < 0
                    || (export.length > 0 && export.data == IntPtr.Zero))
                {
                    throw new InvalidOperationException(
                        "The Rust forest trunk-collider export is invalid.");
                }

                var buckets = new List<IslandPreparedTreeCollider>[
                    ForestTileStreamer.Lod0TileCount];
                var exportSize = Marshal.SizeOf<MotuNative.ForestTrunkColliderExport>();
                for (var index = 0; index < export.length; index++)
                {
                    var native = Marshal.PtrToStructure<MotuNative.ForestTrunkColliderExport>(
                        IntPtr.Add(export.data, index * exportSize));
                    if (!IsFiniteNative(native.bottom)
                        || !IsFiniteNative(native.top)
                        || !IsFinite(native.owner.x)
                        || !IsFinite(native.owner.y)
                        || !IsFinite(native.radius)
                        || native.owner.x < 0f
                        || native.owner.x > 1f
                        || native.owner.y < 0f
                        || native.owner.y > 1f
                        || native.radius <= 0f)
                    {
                        throw new InvalidOperationException(
                            $"The Rust forest trunk collider {index} is invalid.");
                    }
                    var bottom = NativePositionToUnity(native.bottom, worldSize);
                    var top = NativePositionToUnity(native.top, worldSize);
                    var radius = native.radius * worldSize;
                    if ((top - bottom).sqrMagnitude <= Mathf.Epsilon || !IsFinite(radius))
                    {
                        throw new InvalidOperationException(
                            $"The copied forest trunk collider {index} is degenerate.");
                    }
                    var tileX = Mathf.Min(
                        Mathf.FloorToInt(native.owner.x * ForestTileStreamer.Lod0Resolution),
                        ForestTileStreamer.Lod0Resolution - 1);
                    var tileY = Mathf.Min(
                        Mathf.FloorToInt(native.owner.y * ForestTileStreamer.Lod0Resolution),
                        ForestTileStreamer.Lod0Resolution - 1);
                    var tileIndex = tileY * ForestTileStreamer.Lod0Resolution + tileX;
                    buckets[tileIndex] ??= new List<IslandPreparedTreeCollider>();
                    buckets[tileIndex].Add(new IslandPreparedTreeCollider(bottom, top, radius));
                }

                var result = new IslandPreparedTreeCollider[ForestTileStreamer.Lod0TileCount][];
                for (var tile = 0; tile < result.Length; tile++)
                {
                    result[tile] = buckets[tile]?.ToArray()
                        ?? Array.Empty<IslandPreparedTreeCollider>();
                }
                return result;
            }
            finally
            {
                MotuNative.ReleaseForestTrunkColliders(ref export);
            }
        }
        internal static Vector3 NativePositionToUnity(
            MotuNative.NativeVector3 position,
            float worldSize)
        {
            return new Vector3(
                (position.x - 0.5f) * worldSize,
                position.z * worldSize,
                (position.y - 0.5f) * worldSize);
        }
        internal static bool IsFiniteNative(MotuNative.NativeVector3 value)
        {
            return IsFinite(value.x) && IsFinite(value.y) && IsFinite(value.z);
        }
    }
}
