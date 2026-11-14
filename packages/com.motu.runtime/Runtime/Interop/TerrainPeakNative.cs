using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Interop
{
    internal static class TerrainPeakNative
    {
        static TerrainPeakNative()
        {
            NativeAbi.EnsureCompatible();
            if (GetTerrainPeakApiVersion() != 1)
                throw new InvalidOperationException("Terrain peak queries require the matching native library. Rebuild and restart Unity.");
        }

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetTerrainPeakApiVersion();

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte TryGetTerrainPeak(IntPtr handle, Vector3 up, out Vector3 position);
    }
}
