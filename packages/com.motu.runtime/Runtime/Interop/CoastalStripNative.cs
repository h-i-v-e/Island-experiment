using System;
using System.Runtime.InteropServices;

namespace Motu.Interop
{
    internal static class CoastalStripNative
    {
        internal const uint ApiVersion = 1;
        internal const uint AlgorithmRevision = 4;

        [StructLayout(LayoutKind.Sequential)]
        internal struct Options
        {
            internal uint sectionIndex, sectionCount, seamSegments;
            internal float seamBlendWidth, seamRidgeHeight;
        }

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetCoastalStripApiVersion();

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetCoastalStripAlgorithmRevision();

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetCoastalStripOptionsSize();

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr CreateMotuWithCoastalStrip(int seed, ref MotuNative.Options options,
            ref MotuNative.ForestOptions forest, ref MotuNative.ReedOptions reeds, ref MotuNative.FernOptions ferns,
            ref CaveNative.Options caves, ref CaveNative.NetworkOptions network, ref CaveNative.WalkOptions walk,
            ref HybridForestNative.Options hybrid, ref Options strip);

        internal static void EnsureCompatible()
        {
            NativeAbi.EnsureCompatible();
            try
            {
                if (GetCoastalStripApiVersion() != ApiVersion
                    || GetCoastalStripAlgorithmRevision() != AlgorithmRevision
                    || GetCoastalStripOptionsSize() != Marshal.SizeOf<Options>())
                    throw new InvalidOperationException("The coastal-strip native API does not match this package. Install the matching native library and restart Unity.");
            }
            catch (EntryPointNotFoundException error)
            {
                throw new InvalidOperationException("Coastal strips require the updated native library. Install it and restart Unity.", error);
            }
        }
    }
}
