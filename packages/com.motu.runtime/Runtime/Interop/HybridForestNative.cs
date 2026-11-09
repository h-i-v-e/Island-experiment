using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Motu.Islands;
using UnityEngine;

namespace Motu.Interop
{
    internal sealed class HybridForestData
    {
        internal readonly IslandPreparedMesh[][] prototypes;
        internal readonly HybridForestNative.Instance[] instances;
        internal readonly int broadleafCount;
        internal readonly float worldSize;
        internal readonly int shadowResolution;
        internal readonly byte[] shadowPixels;
        internal HybridForestData(IslandPreparedMesh[][] prototypes, HybridForestNative.Instance[] instances,
            int broadleafCount, float worldSize, int shadowResolution = 1, byte[] shadowPixels = null)
        {
            this.prototypes = prototypes;
            this.instances = instances;
            this.broadleafCount = broadleafCount;
            this.worldSize = worldSize;
            this.shadowResolution = shadowResolution;
            this.shadowPixels = shadowPixels ?? new byte[1];
            if (shadowResolution < 1 || shadowResolution > 2048 || (shadowResolution & (shadowResolution - 1)) != 0
                || this.shadowPixels.Length != checked(shadowResolution * shadowResolution))
                throw new ArgumentException("Invalid native forest shadow mask dimensions.");
        }
    }

    internal static class HybridForestNative
    {
        static HybridForestNative() => NativeAbi.EnsureCompatible();
        internal const uint CatalogVersion = 6;
        private const string Library = "motu";
        private static readonly object PrototypeLock = new object();
        private static readonly Lazy<IslandPreparedMaterialTexture[]> BarkMaps =
            new Lazy<IslandPreparedMaterialTexture[]>(() => new[] { PrepareBark(0), PrepareBark(1), PrepareBark(2) });
        internal static IslandPreparedMaterialTexture[] SharedBarkMaps => BarkMaps.Value;
        private static readonly Dictionary<(uint count, float size), WeakReference<IslandPreparedMesh[][]>> Prototypes
            = new Dictionary<(uint, float), WeakReference<IslandPreparedMesh[][]>>();
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetHybridForestCatalogVersion();
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern byte BakeHybridForestBark(uint species, uint resolution,
            out MotuNative.ExportMaterialTextureSet output);

        private static IslandPreparedMaterialTexture PrepareBark(uint species)
        {
            const int resolution = 256;
            var succeeded = BakeHybridForestBark(species, resolution, out var textures);
            try
            {
                if (succeeded == 0 || textures.handle == IntPtr.Zero)
                    throw new InvalidOperationException("Could not bake the shared forest bark recipes.");
                return MaterialPreparation.CopyMaterialTexture(textures.treeBark, resolution,
                    species == 0 ? "broadleaf bark" : species == 1 ? "pine bark" : "palm bark");
            }
            finally { MotuNative.ReleaseMaterialTextureSet(ref textures); }
        }
        [StructLayout(LayoutKind.Sequential)]
        internal struct Options
        {
            internal float latitudeDegrees, altitudeCooling;
            internal uint variantsPerSpecies;
        }
        // Native XY plane, Z-up; converted once when constructing instance matrices.
        [StructLayout(LayoutKind.Sequential)]
        internal struct Instance
        {
            internal Vector3 anchor;
            internal float yawRadians, scale;
            internal uint prototype;
        }
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr CreateMotuWithHybridForest(int seed, ref MotuNative.Options options,
            ref MotuNative.ForestOptions forest, ref MotuNative.ReedOptions reeds, ref MotuNative.FernOptions ferns,
            ref CaveNative.Options caves, ref CaveNative.NetworkOptions network, ref CaveNative.WalkOptions walk,
            ref Options hybrid);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetHybridForestInfo(IntPtr handle, out uint trees, out uint broadleafCount);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint CopyHybridForestInstances(IntPtr handle, [Out] Instance[] instances, uint capacity);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint CopyHybridForestShadowMask(IntPtr handle, uint resolution, [Out] byte[] pixels, uint capacity);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void CreateHybridForestPrototypeMesh(IntPtr handle, uint prototype, uint kind,
            out MotuNative.ExportMesh mesh);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void CreateForestLogMeshGrid(IntPtr handle, out MotuNative.ExportMeshGrid output);

        internal static IslandPreparedMesh[] PrepareFallenLogs(IntPtr handle, float worldSize)
        {
            CreateForestLogMeshGrid(handle, out var export);
            try
            {
                if (export.handle == IntPtr.Zero || export.data == IntPtr.Zero || export.length != 64)
                    throw new InvalidOperationException("The native fallen-log mesh grid is invalid.");
                var result = new IslandPreparedMesh[64];
                var stride = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var index = 0; index < result.Length; index++)
                {
                    var mesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(IntPtr.Add(export.data, index * stride));
                    result[index] = IslandMeshInterop.CopyOptionalGeneratedMeshData(mesh, worldSize);
                }
                return result;
            }
            finally { MotuNative.ReleaseMeshGrid(ref export); }
        }

        internal static HybridForestData Prepare(IntPtr handle, float worldSize)
        {
            try
            {
                if (GetHybridForestCatalogVersion() != CatalogVersion)
                    throw new InvalidOperationException("Restart Unity to load the three-species forest native library (catalog version 6).");
            }
            catch (EntryPointNotFoundException error)
            {
                throw new InvalidOperationException("Restart Unity to load the updated Planet forest native library.", error);
            }
            var count = GetHybridForestInfo(handle, out var trees, out var broadleaf);
            if (count == 0)
                throw new InvalidOperationException("The native island is missing its shared forest catalog. Regenerate its snapshot.");
            // Generation runs on a worker. Bake once here, before any main-thread uploads.
            _ = SharedBarkMaps;
            if (count > 24 || broadleaf == 0 || broadleaf * 3 != count || trees > int.MaxValue)
                throw new InvalidOperationException("Invalid experimental forest metadata.");
            var instances = new Instance[checked((int)trees)];
            if (CopyHybridForestInstances(handle, instances, trees) != trees)
                throw new InvalidOperationException("Incomplete experimental forest placements.");
            foreach (var tree in instances)
                if (tree.prototype >= count || !IslandMeshInterop.IsFinite(tree.anchor)
                    || !IslandMeshInterop.IsFinite(tree.scale) || tree.scale <= 0
                    || !IslandMeshInterop.IsFinite(tree.yawRadians))
                    throw new InvalidOperationException("Invalid experimental forest instance.");
            IslandPreparedMesh[][] prototypes;
            lock (PrototypeLock)
            {
                var key = (count, worldSize);
                if (!Prototypes.TryGetValue(key, out var cached) || !cached.TryGetTarget(out prototypes))
                {
                    prototypes = new IslandPreparedMesh[count][];
                    for (uint i = 0; i < count; ++i)
                    {
                        prototypes[i] = new IslandPreparedMesh[8];
                        for (uint kind = 0; kind < 8; ++kind)
                        {
                            CreateHybridForestPrototypeMesh(handle, i, kind, out var mesh);
                            try
                            {
                                if (mesh.handle == IntPtr.Zero) throw new InvalidOperationException("Missing tree prototype.");
                                prototypes[i][kind] = IslandMeshInterop.CopyOptionalGeneratedMeshData(mesh, worldSize);
                            }
                            finally { MotuNative.ReleaseMesh(ref mesh); }
                        }
                    }
                    Prototypes[key] = new WeakReference<IslandPreparedMesh[][]>(prototypes);
                }
            }
            var shadowResolution = trees == 0 ? 1
                : Mathf.Clamp(Mathf.NextPowerOfTwo(Mathf.CeilToInt(worldSize / 2f)), 256, 2048);
            var shadowPixels = PrepareShadowMask(handle, shadowResolution);
            return new HybridForestData(prototypes, instances, checked((int)broadleaf), worldSize, shadowResolution, shadowPixels);
        }

        // Runs on the island preparation worker. Rust writes into this pinned
        // managed allocation; the main thread only uploads it and creates mips.
        private static byte[] PrepareShadowMask(IntPtr handle, int resolution)
        {
            var pixels = new byte[checked(resolution * resolution)];
            try
            {
                if (CopyHybridForestShadowMask(handle, (uint)resolution, pixels, (uint)pixels.Length) != pixels.Length)
                    throw new InvalidOperationException("Incomplete native forest shadow mask.");
            }
            catch (EntryPointNotFoundException error)
            {
                throw new InvalidOperationException("Restart Unity to load the native forest shadow-mask generator.", error);
            }
            return pixels;
        }
    }
}
