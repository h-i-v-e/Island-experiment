using Motu.Interop;
using UnityEngine;

namespace Motu.Islands
{
    public sealed partial class IslandRuntime
    {
        private bool peakQueried, peakAvailable;
        private Vector3 peakUp, nativePeak;

        /// <summary>Resident final mesh maximum, cached until the island's up axis changes.</summary>
        public bool TryGetTerrainPeak(out Vector3 worldPeak)
        {
            worldPeak = default;
            if (State != IslandRuntimeState.Active && State != IslandRuntimeState.Dormant
                || nativeHandle == null || !nativeHandle.IsValid || WorldSizeMetres <= 0) return false;
            var matrix = transform.localToWorldMatrix;
            var up = new Vector3(matrix.m10, matrix.m12, matrix.m11).normalized;
            if (!TerrainSurfaceCoordinates.IsFinite(up) || up.sqrMagnitude < .5f) return false;
            if (!peakQueried || !peakUp.Equals(up))
            {
                peakAvailable = nativeHandle.TryGetTerrainPeak(up, out nativePeak);
                peakQueried = true;
                peakUp = up;
            }
            if (!peakAvailable) return false;
            var local = new Vector3(nativePeak.x - .5f, nativePeak.z, nativePeak.y - .5f) * WorldSizeMetres;
            worldPeak = matrix.MultiplyPoint3x4(local);
            return TerrainSurfaceCoordinates.IsFinite(worldPeak);
        }
    }
}
