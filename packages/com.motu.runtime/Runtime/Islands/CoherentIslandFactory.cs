using UnityEngine;
using UnityEngine.Serialization;
using Motu.Settings;
using Motu.Interop;
using Motu.World;

namespace Motu.Islands
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "CoherentIslandFactory")]
    public sealed class CoherentIslandFactory : MonoBehaviour, IIslandGenerationRequestFactory
    {
        private const float MIN_HEIGHT = 50, MAX_HEIGHT = 500;

        [SerializeField] private int seed = 42;
        /// <summary>Set before world discovery or generation starts.</summary>
        public int Seed { get => seed; set => seed = value; }

        [Header("Coastal Terrain Generation")]
        [Tooltip("Shared terrain profile for the northern and southern coasts. The section count sets the world's circumference in repeating 2 km cells; interior islands use their own deterministic profiles.")]
        [FormerlySerializedAs("coastalStripGeneration")]
        [SerializeField] private IslandGenerationSettings terrainGeneration = new IslandGenerationSettings
            { TerrainShape = IslandTerrainShape.CoastalStrip };
        public IslandGenerationSettings TerrainGeneration => terrainGeneration ??=
            new IslandGenerationSettings { TerrainShape = IslandTerrainShape.CoastalStrip };
        public IslandGenerationSettings CoastalStripGeneration => TerrainGeneration;
        public int WorldSectionCount => TerrainGeneration.CoastalSectionCount;

        [Tooltip("North/south half-height in 2 km cells. The outer rows are continuous coasts; no terrain exists beyond them. Also sets the south-to-north solar latitude range.")]
        [InspectorName("World Half-Height (Cells)")]
        [Min(1)] [SerializeField] private int archipelegoRadius = 16;
        public int ArchipelagoRadius => Mathf.Max(archipelegoRadius, 1);
        public int TopCoastRow => ArchipelagoRadius;
        public int BottomCoastRow => -ArchipelagoRadius;
        public Vector2Int StartingCell => new Vector2Int(0, BottomCoastRow + 1);
        public Vector3 StartingPosition(float y = 0f) => IslandWorldManager.CellCentre(StartingCell, y);

        public float GetSunLatitudeDegrees(float worldZ)
        {
            var radiusMetres = ArchipelagoRadius * IslandWorldManager.IslandSizeMetres;
            return Mathf.Lerp(0f, 80f, Mathf.InverseLerp(-radiusMetres, radiusMetres, worldZ));
        }

        [Tooltip("Coherent world-grid frequency used for island existence and height.")]
        [Min(0.001f)] [SerializeField] private float noiseScaleCoarse = 0.15f;

        [Tooltip("Coherent world-grid frequency used for island existence and height.")]
        [Min(0.001f)] [SerializeField] private float noiseScaleFine = 0.5f;

        [Tooltip("Cells whose coherent sample falls below this value remain open sea.")]
        [Range(0f, 1f)] [SerializeField] private float islandThreshold = 0.25f;

        [Header("Navigation (reload islands to apply)")]
        [SerializeField] private IslandNavigationSettings navigationSettings = new IslandNavigationSettings();
        public IslandNavigationSettings NavigationSettings => navigationSettings;

        [SerializeField]
        private IslandRenderingSettings renderingSettings;

        [SerializeField]
        private IslandRiverSettings riverSettings;

        [SerializeField]
        private IslandReedSettings reedSettings;

        [SerializeField]
        private IslandFernSettings fernSettings;

        [SerializeField]
        private Color earthA;

        [SerializeField]
        private Color earthB;

        [SerializeField]
        private Color stoneA;

        [SerializeField]
        private Color stoneB;

        [SerializeField] private IslandCaveSettings caveSettings = new IslandCaveSettings();
        public IslandCaveSettings CaveSettings => caveSettings;

        private IslandForestSettings CreateForestSettings(System.Random random, float snowLine, float latitude)
        {
            return new IslandForestSettings
            {
                ForestLatitudeDegrees = latitude,
                snowlineMetres = snowLine * MAX_HEIGHT,
                forestNoiseThreshold = Mathf.Lerp(0.3f, 0.9f, (float)random.NextDouble())
            };
        }

        private IslandGenerationSettings CreateGenerationSettings(
            System.Random random)
        {
            var maxHeight = IslandRandom.RandomPositiveFloat(random);
            var strength = IslandRandom.RandomPositiveFloat(random);
            var frequency = 0.2f + IslandRandom.RandomPositiveFloat(random) * 4f;
            var output = new IslandGenerationSettings
            {
                Seed = random.Next(),
                MaximumHeightMetres = Mathf.Lerp(MIN_HEIGHT, MAX_HEIGHT, maxHeight),
                WaterRatio = 0.75f + (maxHeight + IslandRandom.RandomPositiveFloat(random)) * 0.1f,
                InlandSlopeMultiplier = Mathf.Lerp(0.2f, 2.0f, IslandRandom.RandomPositiveFloat(random)),
                CoastalSlopeMultiplier = Mathf.Lerp(0.1f, 2.0f, IslandRandom.RandomPositiveFloat(random)),
                ContinentalNoiseFrequency = frequency,
                ContinentalNoiseStrength = strength,
                DetailNoiseFrequency = Mathf.Lerp(frequency, frequency * 8f, IslandRandom.RandomPositiveFloat(random)),
                DetailNoiseStrength = 1f - strength,
                LandMassOffset = Mathf.Min(0f, 0.5f - IslandRandom.RandomPositiveFloat(random)),
                InitialSoilDepthMetres = (1f - maxHeight) * 5f,
                HydraulicErosionStrength = Mathf.Lerp(4f, 8f, IslandRandom.RandomPositiveFloat(random))
            };
            return output;
        }

        private IslandMaterialColours CreateIslandMaterialColours(System.Random random)
        {
            var earth = Color.Lerp(earthA, earthB, IslandRandom.RandomPositiveFloat(random));
            var stone = Color.Lerp(stoneA, stoneB, IslandRandom.RandomPositiveFloat(random));
            return new IslandMaterialColours(
                earth,
                stone,
                (earth + stone) * 0.5f
            );
        }

        public bool HasIsland(Vector2Int islandGridPosition)
        {
            var y = islandGridPosition.y;
            if (y < BottomCoastRow || y > TopCoastRow) return false;
            if (y == BottomCoastRow || y == TopCoastRow) return true;
            var x = CanonicalX(islandGridPosition.x);
            if (x == 0 && y == StartingCell.y) return false;
            var sample = PeriodicNoise(x, y, noiseScaleCoarse, (uint)seed) * 0.25f +
                PeriodicNoise(x, y, noiseScaleFine, unchecked((uint)seed * 3u)) * 0.75f;
            return sample >= islandThreshold;
        }

        public IslandGenerationRequest CreateIslandGenerationRequest(
            Vector2Int islandGridPosition)
        {
            if (!HasIsland(islandGridPosition))
            {
                return null;
            }
            if (islandGridPosition.y == BottomCoastRow || islandGridPosition.y == TopCoastRow)
                return CreateCoastalStripRequest(islandGridPosition);
            var snowLine = 1f - Mathf.Max(islandGridPosition.y / (float)ArchipelagoRadius, 0f);
            var seedCell = new Vector2Int(CanonicalX(islandGridPosition.x), islandGridPosition.y);
            var islandSeed = IslandRandom.SeedForCell(seed, seedCell);
            var random = new System.Random(islandSeed);
            var profile = new IslandGenerationProfile(
                CreateGenerationSettings(random),
                riverSettings,
                CreateForestSettings(random, snowLine, GetSunLatitudeDegrees(islandGridPosition.y * IslandWorldManager.IslandSizeMetres)),
                reedSettings, fernSettings,
                renderingSettings,
                new IslandDebugSettings(), caveSettings, navigationSettings
            );
            // Resolve the land mask before erosion, refinement, rivers or caching.
            var nativeOptions = profile.Generation.ToNativeOptions(profile.Rivers);
            profile.Generation.LandMassOffset = MotuNative.ResolveMotuLandMassOffset(islandSeed, ref nativeOptions);
            return IslandGenerationRequest.FromOwnedProfile(
                islandSeed,
                islandGridPosition,
                profile,
                CreateIslandMaterialColours(random)
                );
        }

        private IslandGenerationRequest CreateCoastalStripRequest(Vector2Int cell)
        {
            var inlandSouth = cell.y == BottomCoastRow;
            var coastSeed = inlandSouth ? unchecked(seed ^ 0x6D2B79F5) : seed;
            var generation = CoastalStripGeneration.Copy();
            generation.TerrainShape = IslandTerrainShape.CoastalStrip;
            generation.Seed = coastSeed;
            // Do not apply per-cell height, noise, seed or sea-level variation:
            // each tile samples one shared periodic original field.
            var random = new System.Random(coastSeed);
            var snowLine = 1f - Mathf.Max(cell.y / (float)ArchipelagoRadius, 0f);
            var profile = new IslandGenerationProfile(generation, riverSettings,
                CreateForestSettings(random, snowLine,
                    GetSunLatitudeDegrees(cell.y * IslandWorldManager.IslandSizeMetres)),
                reedSettings, fernSettings, renderingSettings, new IslandDebugSettings(),
                caveSettings, navigationSettings);
            return IslandGenerationRequest.FromOwnedProfile(coastSeed, cell, profile,
                CreateIslandMaterialColours(random), coastalInlandSouth: inlandSouth);
        }

        private int CanonicalX(int x)
        {
            var width = WorldSectionCount;
            var section = x % width;
            if (section < 0) section += width;
            return section >= (width + 1) / 2 ? section - width : section;
        }

        private float PeriodicNoise(int x, int y, float scale, uint noiseSeed)
        {
            // Reduce before converting to float so distant aliases sample exactly
            // the same smoothly repeating field, including across the closing seam.
            var phase = x / (double)WorldSectionCount * (System.Math.PI * 2d);
            var radius = WorldSectionCount * (double)scale / (System.Math.PI * 2d);
            var offset = NoiseOffset(noiseSeed);
            return Mathf.PerlinNoise(
                offset.x + (float)(System.Math.Cos(phase) * radius),
                offset.y + (float)(System.Math.Sin(phase) * radius + y * (double)scale));
        }

        private static float HashToNoiseCoordinate(uint hash)
        {
            return (hash & 0x00ffffffu) / 16777216f * 4096f;
        }

        private static Vector2 NoiseOffset(uint hash)
        {
            return new Vector2(
                HashToNoiseCoordinate(IslandRandom.Mix(unchecked(hash))),
                HashToNoiseCoordinate(IslandRandom.Mix(unchecked(hash * 7u)))
            );
        }
    }
}
