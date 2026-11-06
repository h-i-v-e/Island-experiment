using UnityEngine;
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

        [Tooltip("Archipelago radius in 2 km grid cells. Also sets the south-to-north solar latitude range.")]
        [Min(1)] [SerializeField] private int archipelegoRadius = 16;
        public int ArchipelagoRadius => Mathf.Max(archipelegoRadius, 1);

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
            int x = islandGridPosition.x, y = islandGridPosition.y;
            int radius = ArchipelagoRadius;
            if (x <= -radius || x >= radius || y <= -radius || y >= radius) return false;
            // Reject the outer square first so even extreme coordinates cannot
            // overflow the sum; retain the authored exclusive circular edge.
            long sqrRadius = (long)radius * radius;
            long sqrDistance = (long)x * x + (long)y * y;
            if (sqrDistance == 0 || sqrDistance >= sqrRadius)
            {
                return false;
            }
            var weight = Mathf.Sin(2f - (2 * sqrDistance / (float)sqrRadius));
            var seed = (uint)this.seed;
            var noiseOffsetA = NoiseOffset(seed);
            var noiseOffsetB = NoiseOffset(seed * 3);
            var height = Mathf.PerlinNoise(
                noiseOffsetA.x + x * noiseScaleCoarse,
                noiseOffsetA.y + y * noiseScaleCoarse) * 0.25f +
            Mathf.PerlinNoise(
                noiseOffsetB.x + x * noiseScaleFine,
                noiseOffsetB.y + y * noiseScaleFine) * 0.75f;
            if (height * weight < islandThreshold)
            {
                return false;
            }
            return true;
        }

        public IslandGenerationRequest CreateIslandGenerationRequest(
            Vector2Int islandGridPosition)
        {
            if (!HasIsland(islandGridPosition))
            {
                return null;
            }
            var snowLine = 1f - Mathf.Max(islandGridPosition.y / (float)ArchipelagoRadius, 0f);
            var islandSeed = IslandRandom.SeedForCell(seed, islandGridPosition);
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
