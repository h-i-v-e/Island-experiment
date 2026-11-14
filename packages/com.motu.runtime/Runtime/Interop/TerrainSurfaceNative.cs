using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Interop
{
    internal static class TerrainSurfaceNative
    {
        [StructLayout(LayoutKind.Sequential)]
        internal struct Sample
        {
            internal Vector3 Position;
            internal Vector3 Normal;
        }

        static TerrainSurfaceNative()
        {
            NativeAbi.EnsureCompatible();
            if (GetAccurateTerrainSurfaceApiVersion() != 1 || Marshal.SizeOf<Sample>() != 24)
                throw new InvalidOperationException("Accurate terrain queries require the matching native library. Rebuild and restart Unity.");
        }

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetAccurateTerrainSurfaceApiVersion();

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte TryGetAccurateTerrainSurface(IntPtr handle,
            Vector3 origin, Vector3 down, out Sample sample);

        internal static bool TrySample(NativeIslandHandle handle, Transform root, float worldSize,
            Vector3 worldPoint, out float height, out Vector3 normal)
        {
            height = 0;
            normal = Vector3.up;
            return handle != null && root != null
                && TerrainSurfaceCoordinates.TryCreateLine(root.worldToLocalMatrix, worldSize, worldPoint, out var origin, out var down)
                && handle.TrySampleTerrainSurface(origin, down, out var sample)
                && TerrainSurfaceCoordinates.TryToWorld(root.localToWorldMatrix, worldSize, sample, out height, out normal);
        }
    }

    // Pure coordinate conversion can be checked without loading native code.
    internal static class TerrainSurfaceCoordinates
    {
        internal static bool TryCreateLine(Matrix4x4 inverse, float worldSize, Vector3 worldPoint,
            out Vector3 origin, out Vector3 down)
        {
            origin = down = default;
            if (!IsFinite(worldPoint)
                || !float.IsFinite(worldSize) || worldSize <= 0) return false;
            var local = inverse.MultiplyPoint3x4(worldPoint) / worldSize;
            var direction = inverse.MultiplyVector(Vector3.down) / worldSize;
            origin = new Vector3(local.x + .5f, local.z + .5f, local.y);
            down = new Vector3(direction.x, direction.z, direction.y);
            return IsFinite(origin) && IsFinite(down) && down.sqrMagnitude > 0;
        }

        internal static bool TryToWorld(Matrix4x4 matrix, float worldSize, TerrainSurfaceNative.Sample sample,
            out float height, out Vector3 normal)
        {
            height = 0;
            normal = Vector3.up;
            if (!float.IsFinite(worldSize) || worldSize <= 0) return false;
            var position = new Vector3((sample.Position.x - .5f) * worldSize,
                sample.Position.z * worldSize, (sample.Position.y - .5f) * worldSize);
            var nativeNormal = new Vector3(sample.Normal.x, sample.Normal.z, sample.Normal.y);
            var worldPosition = matrix.MultiplyPoint3x4(position);
            var worldNormal = matrix.inverse.transpose.MultiplyVector(nativeNormal);
            if (!IsFinite(worldPosition) || !IsFinite(worldNormal) || worldNormal.sqrMagnitude < 1e-12f) return false;
            height = worldPosition.y;
            normal = worldNormal.normalized;
            if (normal.y < 0) normal = -normal;
            return true;
        }

        internal static bool IsFinite(Vector3 point) =>
            float.IsFinite(point.x) && float.IsFinite(point.y) && float.IsFinite(point.z);
    }
}
