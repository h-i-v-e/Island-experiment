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
    internal static class WaterPreparation
    {
        internal static IslandPreparedSeaMask PrepareSeaMask(IntPtr handle, int dimension)
        {
            MotuNative.CreateSeaMask(handle, dimension, out var seaMask);
            try
            {
                if (seaMask.handle == IntPtr.Zero
                    || seaMask.rgba == IntPtr.Zero
                    || seaMask.width != dimension
                    || seaMask.height != dimension)
                {
                    throw new InvalidOperationException(
                        "The Rust generator returned an invalid coastal wave mask.");
                }

                var byteCount = checked(dimension * dimension * 4);
                var rgba = new byte[byteCount];
                Marshal.Copy(seaMask.rgba, rgba, 0, rgba.Length);
                return new IslandPreparedSeaMask(dimension, rgba);
            }
            finally
            {
                MotuNative.ReleaseSeaMask(ref seaMask);
            }
        }
        internal static IslandPreparedMesh PrepareSkyDome(float worldSize)
        {
            MotuNative.CreateSkyDome(out var export);
            try
            {
                if (export.handle == IntPtr.Zero
                    || export.vertices.data == IntPtr.Zero
                    || export.vertices.length == 0
                    || export.normals.length != export.vertices.length
                    || export.uv.length != export.vertices.length
                    || export.triangles.data == IntPtr.Zero
                    || export.triangles.length == 0)
                {
                    throw new InvalidOperationException(
                        "The Rust generator returned an invalid sky-dome mesh.");
                }
                return IslandMeshInterop.CopyGeneratedMeshData(export, worldSize);
            }
            finally
            {
                MotuNative.ReleaseMesh(ref export);
            }
        }
        internal static IslandPreparedMesh[] PrepareRiverTiles(IntPtr handle, float worldSize)
        {
            var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
            MotuNative.CreateRiverMeshGrid(
                handle,
                ref area,
                TerrainTileStreamer.Lod1Resolution,
                out var export);
            try
            {
                var expectedLength = TerrainTileStreamer.Lod1Resolution
                    * TerrainTileStreamer.Lod1Resolution;
                if (export.handle == IntPtr.Zero
                    || export.data == IntPtr.Zero
                    || export.length != expectedLength)
                {
                    throw new InvalidOperationException(
                        "The Rust river slicer returned an invalid LOD 1 tile batch.");
                }

                var result = new IslandPreparedMesh[export.length];
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < export.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(export.data, index * exportSize));
                    if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                    {
                        result[index] = IslandMeshInterop.CopyRiverMeshData(
                            nativeMesh,
                            worldSize);
                    }
                }
                return result;
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref export);
            }
        }
        internal static IslandPreparedMesh[] PrepareRiverRockTiles(IntPtr handle, float worldSize)
        {
            var area = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
            MotuNative.CreateRiverRockMeshGrid(
                handle,
                ref area,
                TerrainTileStreamer.Lod1Resolution,
                out var export);
            try
            {
                var expectedLength = TerrainTileStreamer.Lod1Resolution
                    * TerrainTileStreamer.Lod1Resolution;
                if (export.handle == IntPtr.Zero
                    || export.data == IntPtr.Zero
                    || export.length != expectedLength)
                {
                    throw new InvalidOperationException(
                        "The Rust river-rock slicer returned an invalid LOD 1 tile batch.");
                }

                var result = new IslandPreparedMesh[export.length];
                var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < export.length; index++)
                {
                    var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                        IntPtr.Add(export.data, index * exportSize));
                    if (nativeMesh.handle != IntPtr.Zero && nativeMesh.triangles.length != 0)
                    {
                        result[index] = IslandMeshInterop.CopyRiverMeshData(
                            nativeMesh,
                            worldSize);
                    }
                }
                return result;
            }
            finally
            {
                MotuNative.ReleaseMeshGrid(ref export);
            }
        }
        internal static IslandPreparedWaterfallFoot[] PrepareWaterfallFeet(
            IntPtr handle,
            float worldSize)
        {
            MotuNative.CreateWaterfallFeet(handle, out var export);
            try
            {
                if (export.handle == IntPtr.Zero || export.length < 0)
                {
                    throw new InvalidOperationException(
                        "The Rust waterfall-foot export is invalid.");
                }
                if (export.length == 0)
                {
                    return Array.Empty<IslandPreparedWaterfallFoot>();
                }
                if (export.data == IntPtr.Zero)
                {
                    throw new InvalidOperationException(
                        "The Rust waterfall-foot data is missing.");
                }

                var result = new IslandPreparedWaterfallFoot[export.length];
                var exportSize = Marshal.SizeOf<MotuNative.WaterfallFootExport>();
                for (var index = 0; index < export.length; index++)
                {
                    var native = Marshal.PtrToStructure<MotuNative.WaterfallFootExport>(
                        IntPtr.Add(export.data, index * exportSize));
                    var position = new Vector3(
                        (native.position.x - 0.5f) * worldSize,
                        native.position.z * worldSize,
                        (native.position.y - 0.5f) * worldSize);
                    var direction = new Vector3(
                        native.direction.x,
                        native.direction.z,
                        native.direction.y).normalized;
                    result[index] = new IslandPreparedWaterfallFoot(
                        position,
                        direction,
                        native.halfWidth * worldSize,
                        native.drop * worldSize);
                }
                return result;
            }
            finally
            {
                MotuNative.ReleaseWaterfallFeet(ref export);
            }
        }
    }
}
