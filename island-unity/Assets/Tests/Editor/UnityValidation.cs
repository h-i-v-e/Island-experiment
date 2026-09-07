using UnityEngine;

namespace Motu.Editor
{
    /// <summary>Stable batch entry points; exceptions make Unity batchmode fail.</summary>
    public static class UnityValidation
    {
        public static void Run()
        {
            IslandRequestValidation.ValidateSoilAndSnapshots();
            IslandRequestValidation.ValidateCancellation();
            IslandOwnershipValidation.ValidateOwnershipContract();
            MaterialCacheValidation.ValidateRoundTrip();
            VegetationWindValidation.BatchValidateSharedWind();
            OceanShoreDistanceValidation.BatchValidateShoreDistance();
            OceanWaveTransitionValidation.BatchValidateWaveTransitions();
            MinimapTeleportValidation.BatchValidateMinimapTeleport();
            IslandGeneratorValidation.BatchValidateIslandWorldArchitecture();
            IslandNativeValidation.BatchValidateInitialSoilNative();
            Debug.Log("MOTU UNITY VALIDATION PASSED");
        }

        // The full native export/render fixture remains separately invokable.
        public static void NativeExports() => IslandNativeValidation.BatchValidateNativeInterop();
    }
}
