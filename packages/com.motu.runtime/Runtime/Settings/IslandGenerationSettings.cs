using System;
using UnityEngine;
using UnityEngine.Serialization;
using Motu.Interop;
using Motu.World;

namespace Motu.Settings
{
    [Serializable]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandGenerationSettings")]
    public sealed class IslandGenerationSettings
    {
        // Values are copied; authored Unity asset references intentionally remain shared.
        internal IslandGenerationSettings Copy() => (IslandGenerationSettings)MemberwiseClone();

        [Tooltip("Island uses a radial falloff. Coastal Strip repeats along world X, with sea at local -Z and inland heights at local +Z.")]
        [SerializeField] private IslandTerrainShape terrainShape;

        [Header("Coastal Strip")]
        [Tooltip("Number of 2 km sections before the shared height and hardness fields repeat along X.")]
        [Range(1, 4096)] [SerializeField] private int coastalSectionCount = 8;

        [Tooltip("Fixed edge segments shared by adjacent sections and every terrain LOD.")]
        [Range(8, 256)] [SerializeField] private int coastalSeamSegments = 32;

        [Tooltip("Width of the protected transition beside each section seam, in metres.")]
        [Range(40f, 500f)] [SerializeField] private float coastalSeamBlendWidthMetres = 160f;

        [Tooltip("Extra original-field seam ridge, fading towards the sea, to keep drainage inside each section. Zero disables the extra ridge.")]
        [Min(0f)] [SerializeField] private float coastalSeamRidgeHeightMetres = 40f;

        [Tooltip("Restore previously generated islands from the persistent on-disk snapshot cache.")]
        [SerializeField] private bool useSnapshotCache = true;

        [Tooltip("Optional absolute snapshot cache directory owned by Motu. Empty uses persistentDataPath/GeneratedIslandCache.")]
        [SerializeField] private string snapshotCacheDirectory;
        public string SnapshotCacheDirectory
        {
            get => snapshotCacheDirectory;
            set
            {
                if (!string.IsNullOrWhiteSpace(value) && !System.IO.Path.IsPathRooted(value))
                    throw new ArgumentException("The snapshot cache directory must be absolute.", nameof(value));
                snapshotCacheDirectory = value;
            }
        }

        [Tooltip("Maximum shared generated-island snapshot cache size in GiB. Old snapshots are removed first.")]
        [Range(1, 64)]
        [SerializeField] private int snapshotCacheBudgetGiB = 8;

        [Tooltip("Deterministic seed used by the native island generator.")]
        [SerializeField] private int seed = 666;

        [Tooltip("Maximum generated terrain height above sea level in metres.")]
        [Min(1f)]
        [SerializeField] private float maximumHeightMetres = 400f;

        [Tooltip("Water/land balance for islands, or shoreline position within coastal bays. Regenerate to apply.")]
        [Range(0.60f, 0.95f)]
        [SerializeField] private float waterRatio = 0.95f;

        [Tooltip("Multiplier applied to inland terrain slopes. Regenerate to apply.")]
        [Range(0.2f, 4f)]
        [SerializeField] private float inlandSlopeMultiplier = 1.3f;

        [Tooltip("Multiplier applied to coastal terrain slopes. Regenerate to apply.")]
        [Range(0.1f, 4f)]
        [SerializeField] private float coastalSlopeMultiplier = 1f;

        [Tooltip("Final extra seabed drop as a fraction of distance from land. 0.1 adds 10 metres of depth at 100 metres offshore; zero disables it. Applied after erosion and river carving. Regenerate to apply.")]
        [Range(0f, 1f)]
        [SerializeField] private float seabedDepthDistanceRatio = 0.1f;

        [Tooltip("Spatial frequency of the broad noise shared by the original height field and hardness. Higher values can split the land into smaller island groups. Regenerate to apply.")]
        [Range(0.1f, 32f)]
        [SerializeField] private float continentalNoiseFrequency = 2.2f;

        [Tooltip("Broad terrain noise strength. Coastal strips use it to vary their shoreline and inland mountains. Regenerate to apply.")]
        [Range(0f, 4f)]
        [SerializeField] private float continentalNoiseStrength = 0.78f;

        [Tooltip("Spatial frequency of the fine noise shared by the original height field and hardness. Higher values produce smaller features. Regenerate to apply.")]
        [Range(0.1f, 64f)]
        [SerializeField] private float detailNoiseFrequency = 12f;

        [Tooltip("Fine terrain relief strength. In coastal bays it can also raise small offshore islands. Regenerate to apply.")]
        [Range(0f, 4f)]
        [SerializeField] private float detailNoiseStrength = 0.22f;

        [Tooltip("Signed offset applied after the water-ratio sea level is selected. Negative values submerge land and isolate high points into archipelagos. Regenerate to apply.")]
        [Range(-2f, 2f)]
        [SerializeField] private float landMassOffset;

        [Tooltip("Loose soil depth in metres beneath the initial land surface. Eroded before bedrock; does not raise the surface or coat the seabed. Zero disables the blanket. Regenerate to apply.")]
        [Min(0f)]
        [SerializeField] private float initialSoilDepthMetres;

        [Tooltip("Hydraulic erosion strength. Regenerate to apply.")]
        [Range(0f, 8f)]
        [SerializeField] private float hydraulicErosionStrength = 1f;

        [Tooltip("Sediment deposition strength. Regenerate to apply.")]
        [Range(0f, 4f)]
        [SerializeField] private float sedimentDepositionStrength = 1.5f;

        [Tooltip("Maximum slope on which hydraulic sediment is deposited, in degrees.")]
        [Range(1f, 45f)]
        [SerializeField] private float depositionMaximumSlopeDegrees = 12f;

        public bool UseSnapshotCache
        {
            get => useSnapshotCache;
            set => useSnapshotCache = value;
        }
        public int SnapshotCacheBudgetGiB
        {
            get => Mathf.Clamp(snapshotCacheBudgetGiB, 1, 64);
            set => snapshotCacheBudgetGiB = Mathf.Clamp(value, 1, 64);
        }
        internal long SnapshotCacheBudgetBytes =>
            (long)SnapshotCacheBudgetGiB * 1024L * 1024L * 1024L;
        public int Seed { get => seed; set => seed = value; }
        public IslandTerrainShape TerrainShape
        {
            get => terrainShape;
            set
            {
                if (value != IslandTerrainShape.Island && value != IslandTerrainShape.CoastalStrip)
                    throw new ArgumentOutOfRangeException(nameof(value));
                terrainShape = value;
            }
        }
        public int CoastalSectionCount
        {
            get => Mathf.Clamp(coastalSectionCount, 1, 4096);
            set => coastalSectionCount = Mathf.Clamp(value, 1, 4096);
        }
        public int CoastalSeamSegments
        {
            get => Mathf.Clamp(coastalSeamSegments, 8, 256);
            set => coastalSeamSegments = Mathf.Clamp(value, 8, 256);
        }
        public float CoastalSeamBlendWidthMetres
        {
            get => FiniteClamp(coastalSeamBlendWidthMetres, 160f, 40f, 500f);
            set => coastalSeamBlendWidthMetres = FiniteClamp(value, 160f, 40f, 500f);
        }
        public float CoastalSeamRidgeHeightMetres
        {
            get => FiniteClamp(coastalSeamRidgeHeightMetres, 40f, 0f, MaximumHeightMetres);
            set => coastalSeamRidgeHeightMetres = FiniteClamp(value, 40f, 0f, MaximumHeightMetres);
        }
        private static float FiniteClamp(float value, float fallback, float minimum, float maximum) =>
            Mathf.Clamp(float.IsNaN(value) || float.IsInfinity(value) ? fallback : value, minimum, maximum);
        public float WorldSizeMetres => IslandWorldManager.IslandSizeMetres;
        public float MaximumHeightMetres
        {
            get => Mathf.Clamp(maximumHeightMetres, 1f, WorldSizeMetres * 0.5f);
            set => maximumHeightMetres = Mathf.Clamp(value, 1f, WorldSizeMetres * 0.5f);
        }
        internal float MaximumHeightNormalized => MaximumHeightMetres / WorldSizeMetres;
        public float WaterRatio
        {
            get => Mathf.Clamp(waterRatio, 0.60f, 0.95f);
            set => waterRatio = Mathf.Clamp(value, 0.60f, 0.95f);
        }
        public float InlandSlopeMultiplier
        {
            get => Mathf.Clamp(inlandSlopeMultiplier, 0.2f, 4f);
            set => inlandSlopeMultiplier = Mathf.Clamp(value, 0.2f, 4f);
        }
        public float CoastalSlopeMultiplier
        {
            get => Mathf.Clamp(coastalSlopeMultiplier, 0.1f, 4f);
            set => coastalSlopeMultiplier = Mathf.Clamp(value, 0.1f, 4f);
        }
        public float ContinentalNoiseFrequency
        {
            get => Mathf.Clamp(continentalNoiseFrequency, 0.1f, 128f);
            set => continentalNoiseFrequency = Mathf.Clamp(value, 0.1f, 128f);
        }
        public float SeabedDepthDistanceRatio
        {
            get => SanitizeSeabedRatio(seabedDepthDistanceRatio);
            set => seabedDepthDistanceRatio = SanitizeSeabedRatio(value);
        }
        private static float SanitizeSeabedRatio(float value) =>
            float.IsNaN(value) || float.IsInfinity(value) ? 0.1f : Mathf.Clamp01(value);
        public float ContinentalNoiseStrength
        {
            get => Mathf.Clamp(continentalNoiseStrength, 0f, 4f);
            set => continentalNoiseStrength = Mathf.Clamp(value, 0f, 4f);
        }
        public float DetailNoiseFrequency
        {
            get => Mathf.Clamp(detailNoiseFrequency, 0.1f, 128f);
            set => detailNoiseFrequency = Mathf.Clamp(value, 0.1f, 128f);
        }
        public float DetailNoiseStrength
        {
            get => Mathf.Clamp(detailNoiseStrength, 0f, 4f);
            set => detailNoiseStrength = Mathf.Clamp(value, 0f, 4f);
        }
        public float LandMassOffset
        {
            get => Mathf.Clamp(landMassOffset, -2f, 2f);
            set => landMassOffset = Mathf.Clamp(value, -2f, 2f);
        }
        public float InitialSoilDepthMetres
        {
            get => SanitizeSoilDepth(initialSoilDepthMetres);
            set => initialSoilDepthMetres = SanitizeSoilDepth(value);
        }
        internal static float SanitizeSoilDepth(float value) =>
            float.IsNaN(value) || float.IsInfinity(value) ? 0f : Mathf.Max(0f, value);

        public float HydraulicErosionStrength
        {
            get => Mathf.Clamp(hydraulicErosionStrength, 0f, 8f);
            set => hydraulicErosionStrength = Mathf.Clamp(value, 0f, 8f);
        }
        public float SedimentDepositionStrength
        {
            get => Mathf.Clamp(sedimentDepositionStrength, 0f, 4f);
            set => sedimentDepositionStrength = Mathf.Clamp(value, 0f, 4f);
        }
        public float DepositionMaximumSlopeDegrees
        {
            get => Mathf.Clamp(depositionMaximumSlopeDegrees, 1f, 45f);
            set => depositionMaximumSlopeDegrees = Mathf.Clamp(value, 1f, 45f);
        }

        internal MotuNative.Options ToNativeOptions(IslandRiverSettings rivers)
        {
            return new MotuNative.Options
            {
                maxZ = MaximumHeightNormalized,
                waterRatio = WaterRatio,
                slopeMultiplier = InlandSlopeMultiplier,
                coastalSlopeMultiplier = CoastalSlopeMultiplier,
                seabedDepthDistanceRatio = SeabedDepthDistanceRatio,
                continentalNoiseFrequency = ContinentalNoiseFrequency,
                detailNoiseFrequency = DetailNoiseFrequency,
                initialSoilDepthMetres = InitialSoilDepthMetres,
                hydraulicErosionStrength = HydraulicErosionStrength,
                hydraulicDepositionStrength = SedimentDepositionStrength,
                hydraulicDepositionSlopeDegrees = DepositionMaximumSlopeDegrees,
                riverSourceCatchmentHectares = rivers.SourceCatchmentHectares,
                riverSourceSteepCatchmentMultiplier =
                    rivers.SteepSourceCatchmentMultiplier,
                riverSourceElevationBoost = rivers.SourceElevationBoost,
                riverSourceWidthMetres = ToNativeRiverMetres(rivers.SourceWidthMetres),
                riverMaximumWidthMetres = ToNativeRiverMetres(rivers.MaximumWidthMetres),
                riverSourceDepthMetres = ToNativeRiverMetres(rivers.SourceDepthMetres),
                riverMaximumDepthMetres = ToNativeRiverMetres(rivers.MaximumDepthMetres),
                continentalNoiseStrength = ContinentalNoiseStrength,
                detailNoiseStrength = DetailNoiseStrength,
                landMassOffset = LandMassOffset,
            };
        }

        internal CoastalStripNative.Options? ToNativeCoastalStripOptions(Vector2Int worldCell, bool inlandSouth = false)
        {
            if (TerrainShape == IslandTerrainShape.Island) return null;
            if (TerrainShape != IslandTerrainShape.CoastalStrip)
                throw new ArgumentOutOfRangeException(nameof(TerrainShape), "Unknown terrain shape.");
            var count = CoastalSectionCount;
            // Positive modulo keeps the west side of section zero continuous too.
            var index = (inlandSouth ? -(long)worldCell.x : worldCell.x) % count;
            if (index < 0) index += count;
            return new CoastalStripNative.Options
            {
                sectionIndex = (uint)index,
                sectionCount = (uint)count,
                seamSegments = (uint)CoastalSeamSegments,
                seamBlendWidth = CoastalSeamBlendWidthMetres / WorldSizeMetres,
                seamRidgeHeight = CoastalSeamRidgeHeightMetres / WorldSizeMetres,
            };
        }

        internal MotuNative.ForestOptions ToNativeForestOptions(IslandForestSettings forest)
        {
            return new MotuNative.ForestOptions
            {
                patchSizeMetres = ToNativeForestMetres(forest.ForestPatchSizeMetres),
                noiseThreshold = forest.ForestNoiseThreshold,
                noiseOctaves = 4,
                snowlineMetres = ToNativeForestMetres(forest.SnowlineMetres),
                // Placement chooses a variant before the climate selects its species.
                prototypeCount = (byte)forest.HybridOptions.variantsPerSpecies,
                minimumScale = forest.MinimumTreeScale,
                maximumScale = forest.MaximumTreeScale,
                fallenLogDensity = forest.FallenLogDensity,
            };
        }

        internal MotuNative.ReedOptions ToNativeReedOptions(IslandReedSettings reeds)
        {
            return new MotuNative.ReedOptions
            {
                bankWidthMetres = ToNativeForestMetres(reeds.BankWidthMetres),
                patchSizeMetres = ToNativeForestMetres(reeds.PatchSizeMetres),
                coverageThreshold = reeds.CoverageThreshold,
                spacingMetres = ToNativeForestMetres(reeds.SpacingMetres),
                rushRatio = reeds.RushRatio,
                minimumHeightMetres = ToNativeForestMetres(reeds.MinimumHeightMetres),
                maximumHeightMetres = ToNativeForestMetres(reeds.MaximumHeightMetres),
                maximumSlopeDegrees = reeds.MaximumSlopeDegrees,
            };
        }

        internal MotuNative.FernOptions ToNativeFernOptions(IslandFernSettings ferns)
        {
            return new MotuNative.FernOptions
            {
                barkClearanceMetres = ToNativeForestMetres(ferns.BarkClearanceMetres),
                outerRadiusMetres = ToNativeForestMetres(ferns.OuterRadiusMetres),
                spacingMetres = ToNativeForestMetres(ferns.SpacingMetres),
                patchSizeMetres = ToNativeForestMetres(ferns.PatchSizeMetres),
                coverageThreshold = ferns.CoverageThreshold,
                minimumLengthMetres = ToNativeForestMetres(ferns.MinimumLengthMetres),
                maximumLengthMetres = ToNativeForestMetres(ferns.MaximumLengthMetres),
                maximumSlopeDegrees = ferns.MaximumSlopeDegrees,
            };
        }

        private float ToNativeRiverMetres(float metres)
        {
            return metres;
        }

        private float ToNativeForestMetres(float metres)
        {
            return metres;
        }
    }
}
