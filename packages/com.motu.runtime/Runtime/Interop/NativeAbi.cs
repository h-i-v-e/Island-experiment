using System;
using System.Runtime.InteropServices;

namespace Motu.Interop
{
    /// <summary>Checks scalar ABI metadata before passing any managed records to Rust.</summary>
    internal static class NativeAbi
    {
        internal const uint Version = 1;
        private static readonly Lazy<bool> compatible = new Lazy<bool>(() => { Validate(); return true; });
        internal static void EnsureCompatible() => _ = compatible.Value;
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetMotuAbiVersion();
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetMotuAbiTypeSize(uint typeId);
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetMotuAbiForestOffset(uint fieldId);

        internal static void Validate()
        {
            try
            {
                var actual = GetMotuAbiVersion();
                if (actual != Version)
                    throw new InvalidOperationException($"Motu native ABI {actual} does not match managed ABI {Version}. Install matching packages and restart Unity.");
                Type[] types = { typeof(MotuNative.Options), typeof(MotuNative.ForestOptions),
                    typeof(MotuNative.ReedOptions), typeof(MotuNative.FernOptions),
                    typeof(MotuNative.Vector3Array), typeof(MotuNative.TriangleArray),
                    typeof(MotuNative.ExportMesh), typeof(MotuNative.ExportMeshGrid) };
                for (uint i = 0; i < types.Length; i++)
                    if (GetMotuAbiTypeSize(i) != Marshal.SizeOf(types[i]))
                        throw new InvalidOperationException($"Motu native layout mismatch for {types[i].Name}. Install matching native and managed packages.");
                string[] fields = { "snowlineMetres", "prototypeCount", "minimumScale", "fallenLogDensity" };
                for (uint i = 0; i < fields.Length; i++)
                    if (GetMotuAbiForestOffset(i) != (uint)Marshal.OffsetOf<MotuNative.ForestOptions>(fields[i]).ToInt32())
                        throw new InvalidOperationException($"Motu native forest field offset mismatch: {fields[i]}.");
            }
            catch (DllNotFoundException error)
            {
                throw new InvalidOperationException("Motu native library is unavailable for this platform/CPU. This package ships macOS Apple Silicon. Check plugin import settings and restart Unity after installing the matching binary.", error);
            }
            catch (EntryPointNotFoundException error)
            {
                throw new InvalidOperationException("Motu native library predates this package's ABI handshake. Install the matching native binary and restart Unity.", error);
            }
        }
    }
}
