using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;

namespace Motu.Streaming
{
    internal static class TerrainGridWorker
    {
        // One grid export across all islands bounds temporary native/managed buffers.
        private static readonly SemaphoreSlim Gate = new SemaphoreSlim(1, 1);

        internal static async Task<IslandPreparedMesh[]> PrepareAsync(
            NativeIslandHandle owner, MotuNative.ExportArea area, int lod,
            int divisions, byte clampSides, float worldSize, CancellationToken cancellation,
            WheatField[] wheatFields = null)
        {
            await Gate.WaitAsync(cancellation).ConfigureAwait(false);
            try
            {
                cancellation.ThrowIfCancellationRequested();
                using var lease = owner.Acquire();
                return await Task.Run(() => Prepare(lease.Value, area, lod, divisions,
                    clampSides, worldSize, cancellation, wheatFields), cancellation).ConfigureAwait(false);
            }
            finally { Gate.Release(); }
        }

        internal static IslandPreparedMesh[] Prepare(
            IntPtr handle, MotuNative.ExportArea area, int lod, int divisions,
            byte clampSides, float worldSize, CancellationToken cancellation, WheatField[] wheatFields = null)
        {
            cancellation.ThrowIfCancellationRequested();
            MotuNative.ExportMeshGrid export;
            if (lod == 0)
                MotuNative.CreateMeshGrid(handle, ref area, lod, divisions, clampSides, out export);
            else
                MotuNative.CreateTerrainAtlasMeshGrid(handle, ref area, lod, divisions, clampSides, out export);
            try
            {
                if (export.handle == IntPtr.Zero || export.data == IntPtr.Zero
                    || export.length != divisions * divisions)
                    throw new InvalidOperationException("The Rust grid slicer returned an invalid tile batch.");
                var meshes = new IslandPreparedMesh[export.length];
                var coverageSources = IslandMeshInterop.GetTerrainCoverageSources(handle);
                var stride = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < meshes.Length; index++)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var source = Marshal.PtrToStructure<MotuNative.ExportMesh>(IntPtr.Add(export.data, index * stride));
                    if (source.handle != IntPtr.Zero && source.triangles.length != 0)
                    {
                        meshes[index] = IslandMeshInterop.CopyTerrainMeshData(source, lod, worldSize, coverageSources);
                        PrepareWheat(meshes[index], wheatFields, area, divisions, index, worldSize, lod, cancellation);
                    }
                }
                cancellation.ThrowIfCancellationRequested();
                return meshes;
            }
            finally { MotuNative.ReleaseMeshGrid(ref export); }
        }

        internal static void PrepareWheat(IslandPreparedMesh mesh, WheatField[] fields,
            MotuNative.ExportArea area, int divisions, int index, float worldSize, int lod,
            CancellationToken cancellation)
        {
            if ((fields?.Length ?? 0) == 0) return;
            var x = index % divisions;
            var z = index / divisions;
            float Coordinate(float min, float max, int interval) =>
                (min + (max - min) * ((float)interval / divisions) - 0.5f) * worldSize;

            var bounds = new WheatTileBounds(
                Coordinate(area.min.x, area.max.x, x), Coordinate(area.min.y, area.max.y, z),
                Coordinate(area.min.x, area.max.x, x + 1), Coordinate(area.min.y, area.max.y, z + 1));
            mesh.wheat = WheatTileGeometry.Build(mesh, fields, lod, cancellation, bounds);
        }
    }
}
