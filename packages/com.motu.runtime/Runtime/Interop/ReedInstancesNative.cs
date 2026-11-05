using System;
using System.Runtime.InteropServices;
using Motu.Islands;
using Motu.Streaming;
using UnityEngine;

namespace Motu.Interop
{
    internal static class ReedInstancesNative
    {
        [StructLayout(LayoutKind.Sequential)]
        internal struct Instance
        {
            internal Vector3 root, groundNormal;
            internal float heightMetres, widthMetres, variant, tint, stiffness, phase;
        }

        [StructLayout(LayoutKind.Sequential)]
        internal struct Export
        {
            internal IntPtr handle, data;
            internal uint length;
            internal IntPtr tileOffsets;
            internal uint tileOffsetsLength, tileResolution;
        }

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern void CreateReedInstances(IntPtr handle, out Export output);

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern void ReleaseReedInstances(ref Export output);

        internal static IslandPreparedReedTile[] Prepare(IntPtr handle, float worldSize)
        {
            NativeAbi.EnsureCompatible();
            Export export;
            try { CreateReedInstances(handle, out export); }
            catch (EntryPointNotFoundException error)
            {
                throw new InvalidOperationException("Restart Unity to load the native reed instancing library.", error);
            }
            try
            {
                return CopyTiles(export, worldSize);
            }
            finally { ReleaseReedInstances(ref export); }
        }

        private static IslandPreparedReedTile[] CopyTiles(Export export, float worldSize)
        {
            ValidateExport(export);
            var offsets = new int[ReedTileStreamer.TileCount + 1];
            Marshal.Copy(export.tileOffsets, offsets, 0, offsets.Length);
            if (offsets[0] != 0 || offsets[offsets.Length - 1] != export.length)
                throw new InvalidOperationException("Invalid native reed tile offsets.");
            var tiles = new IslandPreparedReedTile[ReedTileStreamer.TileCount];
            for (var tile = 0; tile < tiles.Length; tile++)
            {
                var start = offsets[tile];
                var end = offsets[tile + 1];
                if (start < 0 || end < start || end > export.length)
                    throw new InvalidOperationException("Unordered native reed tile offsets.");
                if (start != end) tiles[tile] = CopyTile(export.data, start, end, worldSize);
            }
            return tiles;
        }

        private static IslandPreparedReedTile CopyTile(IntPtr data, int start, int end, float worldSize)
        {
            var placements = new IslandPreparedReedPlacement[end - start];
            var stride = Marshal.SizeOf<Instance>();
            var scale = worldSize / 2000f;
            for (var index = start; index < end; index++)
            {
                var instance = Marshal.PtrToStructure<Instance>(IntPtr.Add(data, checked(index * stride)));
                ValidateInstance(instance);
                placements[index - start] = new IslandPreparedReedPlacement(
                    new Vector3((instance.root.x - .5f) * worldSize,
                        instance.root.z * worldSize, (instance.root.y - .5f) * worldSize),
                    new Vector3(instance.groundNormal.x, instance.groundNormal.z, instance.groundNormal.y),
                    instance.heightMetres * scale, instance.widthMetres * scale,
                    new Vector4(instance.variant, instance.tint, instance.stiffness, instance.phase));
            }
            return new IslandPreparedReedTile(placements);
        }

        private static void ValidateExport(Export export)
        {
            if (export.handle == IntPtr.Zero || export.length > int.MaxValue / 48
                || (export.length > 0 && export.data == IntPtr.Zero)
                || export.tileResolution != ReedTileStreamer.Resolution
                || export.tileOffsetsLength != ReedTileStreamer.TileCount + 1
                || export.tileOffsets == IntPtr.Zero)
                throw new InvalidOperationException("The native reed instance grid is invalid.");
        }

        private static void ValidateInstance(Instance instance)
        {
            if (!IslandMeshInterop.IsFinite(instance.root) || !IslandMeshInterop.IsFinite(instance.groundNormal)
                || instance.groundNormal.z <= .0001f
                || !IslandMeshInterop.IsFinite(instance.heightMetres) || instance.heightMetres <= 0
                || !IslandMeshInterop.IsFinite(instance.widthMetres) || instance.widthMetres <= 0
                || !UnitInterval(instance.variant) || !UnitInterval(instance.tint)
                || !UnitInterval(instance.stiffness) || !UnitInterval(instance.phase))
                throw new InvalidOperationException("The native reed placement is invalid.");
        }

        private static bool UnitInterval(float value) => IslandMeshInterop.IsFinite(value) && value >= 0 && value <= 1;
    }
}
