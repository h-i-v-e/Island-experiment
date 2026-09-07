using System;
using System.Runtime.InteropServices;
using UnityEngine;
using Motu.Interop;
using Motu.Settings;
using Motu.Streaming;
using static Motu.Interop.IslandMeshInterop;

namespace Motu.Editor
{
    public static class RiverGeometryValidation
    {
        public static void Run()
        {
            // A dedicated fixture: ordinary island creation need not produce a river.
            var settings = new IslandGenerationSettings { WaterRatio = .6f };
            var options = settings.ToNativeOptions(new IslandRiverSettings());
            var handle = MotuNative.CreateMotu(666, ref options);
            if (handle == IntPtr.Zero) throw new InvalidOperationException("River fixture generation failed.");
            try { ValidateRiverGrid(handle, true); }
            finally { MotuNative.ReleaseMotu(handle); }
            Debug.Log("River fixture geometry and downstream flow coordinates passed.");
        }

        internal static void ValidateRiverGrid(IntPtr handle, bool requireGeometry)
        {
            var riverArea = new MotuNative.ExportArea(0f, 0f, 1f, 1f);
            const int riverResolution = TerrainTileStreamer.Lod1Resolution;
                MotuNative.CreateRiverMeshGrid(
                    handle,
                    ref riverArea,
                    riverResolution,
                    out var riverGrid);
                try
                {
                    if (riverGrid.handle == IntPtr.Zero
                        || riverGrid.data == IntPtr.Zero
                        || riverGrid.length != riverResolution * riverResolution)
                    {
                        throw new InvalidOperationException("Native river-grid layout is invalid.");
                    }
                    var exportSize = Marshal.SizeOf<MotuNative.ExportMesh>();
                    var foundRiverGeometry = false;
                    var minimumRiverV = float.PositiveInfinity;
                    var maximumRiverV = float.NegativeInfinity;
                    for (var index = 0; index < riverGrid.length; index++)
                    {
                        var nativeMesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(
                            IntPtr.Add(riverGrid.data, index * exportSize));
                        if (nativeMesh.triangles.length == 0)
                        {
                            continue;
                        }
                        foundRiverGeometry = true;
                        if (nativeMesh.uv.length != nativeMesh.vertices.length)
                        {
                            throw new InvalidOperationException(
                                "A sliced river tile has invalid UV coordinates.");
                        }
                        foreach (var riverUv in CopyVector2Array(nativeMesh.uv))
                        {
                            if (!IsFinite(riverUv.x) || !IsFinite(riverUv.y))
                            {
                                throw new InvalidOperationException(
                                    "A sliced river tile has invalid flow coordinates.");
                            }
                            minimumRiverV = Mathf.Min(minimumRiverV, riverUv.y);
                            maximumRiverV = Mathf.Max(maximumRiverV, riverUv.y);
                        }
                    }
                    if ((requireGeometry && !foundRiverGeometry)
                        || (foundRiverGeometry && maximumRiverV <= minimumRiverV))
                    {
                        throw new InvalidOperationException(
                            $"River fixture: geometry={foundRiverGeometry}, flow range={minimumRiverV}..{maximumRiverV}.");
                    }
                }
                finally
                {
                    MotuNative.ReleaseMeshGrid(ref riverGrid);
                }

        }
    }
}
