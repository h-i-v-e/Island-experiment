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
                PrepareForestMeshGrid(handle, worldSize, 2, ForestTileStreamer.Lod2Resolution, false),
                PrepareForestMeshGrid(handle, worldSize, 2, ForestTileStreamer.Lod2Resolution, true),
                PrepareForestMeshGrid(handle, worldSize, 1, ForestTileStreamer.Lod1Resolution, false),
                PrepareForestMeshGrid(handle, worldSize, 1, ForestTileStreamer.Lod1Resolution, true),
                PrepareForestMeshGrid(handle, worldSize, 0, ForestTileStreamer.Lod1Resolution, false),
                PrepareForestMeshGrid(handle, worldSize, 0, ForestTileStreamer.Lod1Resolution, true),
                PrepareForestTrunkColliders(handle, worldSize));
        }
        internal static IslandPreparedMesh[] PrepareReedMeshGrid(
            IntPtr handle,
            float worldSize)
        {
            MotuNative.CreateReedMeshGrid(handle, out var export);
            try
            {
                if (export.handle == IntPtr.Zero
                    || export.data == IntPtr.Zero
                    || export.length != ReedTileStreamer.TileCount)
                {
                    throw new InvalidOperationException(
                        "The Rust reed owner grid returned an invalid batch.");
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
                                $"The empty Rust reed tile {index} contains sidecar data.");
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
                            $"The Rust reed tile {index} has invalid mesh attributes.");
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
            float worldSize)
        {
            MotuNative.CreateForestTrunkColliders(handle, out var export);
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
                    ForestTileStreamer.Lod1TileCount];
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
                        Mathf.FloorToInt(native.owner.x * ForestTileStreamer.Lod1Resolution),
                        ForestTileStreamer.Lod1Resolution - 1);
                    var tileY = Mathf.Min(
                        Mathf.FloorToInt(native.owner.y * ForestTileStreamer.Lod1Resolution),
                        ForestTileStreamer.Lod1Resolution - 1);
                    var tileIndex = tileY * ForestTileStreamer.Lod1Resolution + tileX;
                    buckets[tileIndex] ??= new List<IslandPreparedTreeCollider>();
                    buckets[tileIndex].Add(new IslandPreparedTreeCollider(bottom, top, radius));
                }

                var result = new IslandPreparedTreeCollider[ForestTileStreamer.Lod1TileCount][];
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
        internal static IslandPreparedMesh[] PrepareForestMeshGrid(
            IntPtr handle,
            float worldSize,
            int visualLod,
            int divisions,
            bool wood)
        {
            if (divisions <= 0)
            {
                throw new ArgumentOutOfRangeException(nameof(divisions));
            }

            var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
            MotuNative.ExportMeshGrid export;
            if (wood)
            {
                MotuNative.CreateForestWoodMeshGrid(
                    handle,
                    ref area,
                    visualLod,
                    divisions,
                    out export);
            }
            else
            {
                MotuNative.CreateForestFoliageMeshGrid(
                    handle,
                    ref area,
                    visualLod,
                    divisions,
                    out export);
            }

            try
            {
                var expectedLength = checked(divisions * divisions);
                if (export.handle == IntPtr.Zero
                    || export.data == IntPtr.Zero
                    || export.length != expectedLength)
                {
                    throw new InvalidOperationException(
                        $"The Rust forest {(wood ? "wood" : "foliage")} grid returned an invalid "
                        + $"LOD {visualLod} batch.");
                }

                var result = new IslandPreparedMesh[export.length];
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < export.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(export.data, index * exportSize));
                    if (nativeMesh.handle == IntPtr.Zero)
                    {
                        ValidateEmptyForestMesh(nativeMesh, visualLod, index);
                        continue;
                    }
                    if (nativeMesh.vertices.length == 0
                        && nativeMesh.normals.length == 0
                        && nativeMesh.triangles.length == 0)
                    {
                        continue;
                    }
                    ValidateForestNativeMesh(nativeMesh, visualLod, index, wood);
                    var prepared = IslandMeshInterop.CopyGeneratedMeshData(
                        nativeMesh,
                        worldSize);
                    ValidatePreparedForestMesh(prepared, visualLod, index, wood);
                    result[index] = prepared;
                }
                return result;
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref export);
            }
        }
        internal static void ValidateEmptyForestMesh(
            MotuNative.ExportMesh mesh,
            int visualLod,
            int index)
        {
            if (mesh.vertices.length != 0
                || mesh.normals.length != 0
                || mesh.triangles.length != 0
                || mesh.uv.length != 0
                || mesh.material.length != 0
                || mesh.environment.length != 0)
            {
                throw new InvalidOperationException(
                    $"The Rust forest tile {index} at LOD {visualLod} has data without ownership.");
            }
        }
        internal static void ValidateForestNativeMesh(
            MotuNative.ExportMesh mesh,
            int visualLod,
            int index,
            bool wood)
        {
            if (mesh.vertices.length <= 0
                || mesh.vertices.data == IntPtr.Zero
                || mesh.normals.length != mesh.vertices.length
                || mesh.normals.data == IntPtr.Zero
                || mesh.triangles.length <= 0
                || mesh.triangles.length % 3 != 0
                || mesh.triangles.data == IntPtr.Zero
                || mesh.uv.length != mesh.vertices.length
                || mesh.uv.data == IntPtr.Zero
                || mesh.material.length != mesh.vertices.length
                || mesh.material.data == IntPtr.Zero
                || mesh.environment.length != 0)
            {
                throw new InvalidOperationException(
                    $"The Rust forest {(wood ? "wood" : "foliage")} tile {index} at LOD "
                    + $"{visualLod} has invalid mesh attributes.");
            }
        }
        internal static void ValidatePreparedForestMesh(
            IslandPreparedMesh mesh,
            int visualLod,
            int index,
            bool wood)
        {
            if (mesh == null
                || mesh.vertices.Length == 0
                || mesh.normals.Length != mesh.vertices.Length
                || mesh.triangles.Length == 0
                || mesh.triangles.Length % 3 != 0
                || mesh.uv.Length != mesh.vertices.Length
                || mesh.environment.Length != 0
                || mesh.material.Length != mesh.vertices.Length)
            {
                throw new InvalidOperationException(
                    $"The copied forest {(wood ? "wood" : "foliage")} tile {index} at LOD "
                    + $"{visualLod} is invalid.");
            }
            for (var vertex = 0; vertex < mesh.vertices.Length; vertex++)
            {
                if (!IsFinite(mesh.vertices[vertex]) || !IsFinite(mesh.normals[vertex]))
                {
                    throw new InvalidOperationException(
                        $"The copied forest tile {index} at LOD {visualLod} contains a non-finite value.");
                }
            }
            for (var triangle = 0; triangle < mesh.triangles.Length; triangle++)
            {
                if (mesh.triangles[triangle] < 0
                    || mesh.triangles[triangle] >= mesh.vertices.Length)
                {
                    throw new InvalidOperationException(
                        $"The copied forest tile {index} at LOD {visualLod} has an invalid triangle index.");
                }
            }
        }
    }
}
