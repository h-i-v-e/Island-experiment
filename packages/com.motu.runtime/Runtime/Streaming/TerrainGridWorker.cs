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
            int divisions, byte clampSides, float worldSize, CancellationToken cancellation)
        {
            await Gate.WaitAsync(cancellation).ConfigureAwait(false);
            try
            {
                cancellation.ThrowIfCancellationRequested();
                using var lease = owner.Acquire();
                return await Task.Run(() => Prepare(lease.Value, area, lod, divisions,
                    clampSides, worldSize, cancellation), cancellation).ConfigureAwait(false);
            }
            finally { Gate.Release(); }
        }

        internal static IslandPreparedMesh[] Prepare(
            IntPtr handle, MotuNative.ExportArea area, int lod, int divisions,
            byte clampSides, float worldSize, CancellationToken cancellation)
        {
            cancellation.ThrowIfCancellationRequested();
            MotuNative.CreateMeshGrid(handle, ref area, lod, divisions, clampSides, out var export);
            try
            {
                if (export.handle == IntPtr.Zero || export.data == IntPtr.Zero
                    || export.length != divisions * divisions)
                    throw new InvalidOperationException("The Rust grid slicer returned an invalid tile batch.");
                var meshes = new IslandPreparedMesh[export.length];
                var stride = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < meshes.Length; index++)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var source = Marshal.PtrToStructure<MotuNative.ExportMesh>(IntPtr.Add(export.data, index * stride));
                    if (source.handle != IntPtr.Zero && source.triangles.length != 0)
                        meshes[index] = IslandMeshInterop.CopyTerrainMeshData(source, lod, worldSize);
                }
                cancellation.ThrowIfCancellationRequested();
                return meshes;
            }
            finally { MotuNative.ReleaseMeshGrid(ref export); }
        }
    }
}
