using UnityEngine;
using Motu.Settings;

namespace Motu.Islands
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "CoherentIslandFactory")]
    public sealed class CoherentIslandFactory : MonoBehaviour, IIslandGenerationRequestFactory
    {
        private const float MIN_HEIGHT = 50, MAX_HEIGHT = 500;

        [SerializeField] private int seed = 42;

        [Tooltip("Coherent world-grid frequency used for island existence and height.")]
        [Min(0.001f)] [SerializeField] private float noiseScale = 0.15f;

        [Tooltip("Cells whose coherent sample falls below this value remain open sea.")]
        [Range(0f, 1f)] [SerializeField] private float islandThreshold = 0.5f;

        [Tooltip("Height multiplier at the lowest accepted coherent sample.")]
        [Min(0f)] [SerializeField] private float minimumHeightScale = 0.35f;

        [Tooltip("Height multiplier at the strongest coherent sample.")]
        [Min(0f)] [SerializeField] private float maximumHeightScale = 1.15f;

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

        private void OnValidate()
        {
            noiseScale = Mathf.Max(noiseScale, 0.001f);
            islandThreshold = Mathf.Clamp01(islandThreshold);
            minimumHeightScale = Mathf.Max(minimumHeightScale, 0f);
            maximumHeightScale = Mathf.Max(maximumHeightScale, minimumHeightScale);
        }

        private static IslandForestSettings CreateForestSettings(System.Random random, float snowLine)
        {
            return new IslandForestSettings
            {
                snowlineMetres = snowLine * MAX_HEIGHT,
                forestNoiseThreshold = Mathf.Lerp(0.4f, 1f, (float)random.NextDouble())
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

        private IslandRenderingSettings CreateRenderingSettings(
            float y
        )
        {
            renderingSettings.SunLatitudeDegrees = Mathf.Lerp(0f, 90f, Mathf.Clamp(-1f, 1f, y));
            return renderingSettings;
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

        private float Prepare(Vector2Int islandGridPosition)
        {
            if ((islandGridPosition.x & 1) == 0 || (islandGridPosition.y & 1) == 0)
            {
                return 0f;
            }
            var noiseOffset = NoiseOffset();
            var height = Mathf.PerlinNoise(
                noiseOffset.x + islandGridPosition.x * noiseScale,
                noiseOffset.y + islandGridPosition.y * noiseScale);
            if (height < islandThreshold)
            {
                return 0f;
            }
            return height;
        }

        public bool HasIsland(Vector2Int islandGridPosition)
        {
            return Prepare(islandGridPosition) != 0f;
        }

        public IslandGenerationRequest CreateIslandGenerationRequest(
            Vector2Int islandGridPosition)
        {
            var height = Prepare(islandGridPosition);
            if (height == 0f)
            {
                return null;
            }
            var snowLine = islandGridPosition.y * 0.03f;
            snowLine = 1 - Mathf.Clamp01(snowLine * snowLine);
            var islandSeed = IslandRandom.SeedForCell(seed, islandGridPosition);
            var random = new System.Random(islandSeed);
            var profile = new IslandGenerationProfile(
                CreateGenerationSettings(random),
                riverSettings,
                CreateForestSettings(random, snowLine),
                reedSettings, fernSettings,
                CreateRenderingSettings(islandGridPosition.y * 0.03f),
                new IslandDebugSettings()
            );
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

        private Vector2 NoiseOffset()
        {
            return new Vector2(
                HashToNoiseCoordinate(IslandRandom.Mix(unchecked((uint)seed))),
                HashToNoiseCoordinate(IslandRandom.Mix(unchecked((uint)seed * 7u))));
        }
    }
}
