using System;
using UnityEngine;
using Motu.Settings;

namespace Motu.Islands
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandGenerationRequestFactoryBase")]
    public abstract class IslandGenerationRequestFactoryBase : MonoBehaviour,
        IIslandGenerationRequestFactory
    {
        [Header("Lifecycle and Generation")]
        [SerializeField] private IslandGenerationSettings generation =
            new IslandGenerationSettings();

        [Header("Rivers")]
        [SerializeField] private IslandRiverSettings rivers = new IslandRiverSettings();

        [Header("Forest")]
        [SerializeField] private IslandForestSettings forest = new IslandForestSettings();

        [Header("Riverbank Reeds and Rushes")]
        [SerializeField] private IslandReedSettings reeds = new IslandReedSettings();

        [Header("Tree Trunk Ferns")]
        [SerializeField] private IslandFernSettings ferns = new IslandFernSettings();

        [Header("Rendering and Texture Overrides")]
        [SerializeField] private IslandRenderingSettings rendering =
            new IslandRenderingSettings();

        [Header("Material Palette")]
        [Tooltip("Base dirt colour shared by recipe textures and terrain shader fallbacks.")]
        [SerializeField] private Color dirtColour = new Color(0.09f, 0.055f, 0.026f, 1f);

        [Tooltip("Base stone colour shared by recipe textures and terrain shader fallbacks.")]
        [SerializeField] private Color stoneColour = new Color(0.30f, 0.32f, 0.29f, 1f);

        [Tooltip("Base sand colour shared by the beach recipe and terrain shader fallback.")]
        [SerializeField] private Color sandColour = new Color(0.62f, 0.57f, 0.34f, 1f);

        [Tooltip("Derive deterministic dirt, stone, and sand variations from each island seed.")]
        [SerializeField] private bool randomizeMaterialColours = true;

        [Tooltip("Maximum engine-side colour variation selected independently for each island.")]
        [Range(0f, 0.35f)]
        [SerializeField] private float materialColourVariation = 0.14f;

        [Header("Debug")]
        [SerializeField] private IslandDebugSettings debugSettings =
            new IslandDebugSettings();

        public IslandGenerationSettings GenerationSettings => generation;
        public IslandRiverSettings RiverSettings => rivers;
        public IslandForestSettings ForestSettings => forest;
        public IslandReedSettings ReedSettings => reeds;
        public IslandFernSettings FernSettings => ferns;
        public IslandRenderingSettings RenderingSettings => rendering;
        public IslandDebugSettings DebugSettings => debugSettings;
        public Color DirtColour { get => dirtColour; set => dirtColour = ClampColour(value); }
        public Color StoneColour { get => stoneColour; set => stoneColour = ClampColour(value); }
        public Color SandColour { get => sandColour; set => sandColour = ClampColour(value); }
        public bool RandomizeMaterialColours
        {
            get => randomizeMaterialColours;
            set => randomizeMaterialColours = value;
        }
        public float MaterialColourVariation
        {
            get => Mathf.Clamp(materialColourVariation, 0f, 0.35f);
            set => materialColourVariation = Mathf.Clamp(value, 0f, 0.35f);
        }

        public abstract IslandGenerationRequest CreateIslandGenerationRequest(
            Vector2Int islandGridPosition);

        public abstract bool HasIsland(Vector2Int islandGridPosition);

        protected IslandGenerationProfile CreateProfile()
        {
            return CreateProfile(generation);
        }

        protected IslandGenerationProfile CreateProfile(
            IslandGenerationSettings generationSettings)
        {
            return new IslandGenerationProfile(
                generationSettings,
                rivers,
                forest,
                reeds,
                ferns,
                rendering,
                debugSettings);
        }

        protected IslandGenerationRequest CreateRequest(
            int randomSeed,
            Vector2Int islandGridPosition,
            IslandGenerationProfile profile,
            string stableId = null)
        {
            return new IslandGenerationRequest(
                randomSeed,
                islandGridPosition,
                profile,
                SelectMaterialColours(randomSeed, islandGridPosition),
                stableId);
        }

        protected virtual IslandMaterialColours SelectMaterialColours(
            int islandSeed,
            Vector2Int islandGridPosition)
        {
            _ = islandGridPosition;
            var dirt = ClampColour(dirtColour);
            var stone = ClampColour(stoneColour);
            var sand = ClampColour(sandColour);
            var variation = MaterialColourVariation;
            if (!randomizeMaterialColours || variation <= 0f)
            {
                return new IslandMaterialColours(dirt, stone, sand);
            }

            var random = new System.Random(unchecked(islandSeed * 1103515245 + 12345));
            dirt = VaryColour(dirt, random, variation, 0.45f);
            stone = VaryColour(stone, random, variation * 0.72f, 0.18f);
            sand = VaryColour(sand, random, variation * 0.65f, 0.30f);
            return new IslandMaterialColours(dirt, stone, sand);
        }

        private static Color VaryColour(
            Color colour,
            System.Random random,
            float amount,
            float warmth)
        {
            var brightness = 1f + ((float)random.NextDouble() * 2f - 1f) * amount;
            var temperature = ((float)random.NextDouble() * 2f - 1f) * amount * warmth;
            var greenShift = ((float)random.NextDouble() * 2f - 1f) * amount * 0.18f;
            return ClampColour(new Color(
                colour.r * brightness * (1f + temperature),
                colour.g * brightness * (1f + greenShift),
                colour.b * brightness * (1f - temperature),
                1f));
        }

        private static Color ClampColour(Color colour)
        {
            return new Color(
                Mathf.Clamp01(colour.r),
                Mathf.Clamp01(colour.g),
                Mathf.Clamp01(colour.b),
                1f);
        }

        public void ConfigureRenderingReferences(
            Material terrain,
            Material grass,
            Material river,
            Material sea,
            Material rock,
            Material treeFoliage = null)
        {
            rendering.AssignMaterialTemplates(
                terrain,
                grass,
                river,
                sea,
                rock,
                treeFoliage);
        }
    }
}
