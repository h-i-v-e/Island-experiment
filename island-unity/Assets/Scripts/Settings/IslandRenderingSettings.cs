using System;
using UnityEngine;

namespace Motu.Settings
{
    [Serializable]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandRenderingSettings")]
    public sealed class IslandRenderingSettings
    {
        // Values are copied; authored Unity asset references intentionally remain shared.
        internal IslandRenderingSettings Copy() => (IslandRenderingSettings)MemberwiseClone();

        [Tooltip("Runtime resolution requested from the Rust procedural material library.")]
        [Range(128, 2048)]
        [SerializeField] private int materialTextureResolution = 1024;

        [Tooltip("Optional terrain material template. A per-island copy is created at runtime.")]
        [SerializeField] private Material terrainMaterial;

        [Tooltip("Optional grass material template. A per-island copy is created at runtime.")]
        [SerializeField] private Material grassMaterial;

        [Tooltip("Optional river material template. A per-island copy is created at runtime.")]
        [SerializeField] private Material riverMaterial;

        [Tooltip("Optional sea material template. A per-island copy is created at runtime.")]
        [SerializeField] private Material seaMaterial;

        [Tooltip("Optional reusable geometric-wave profile for the global deep ocean.")]
        [SerializeField] private OceanWaveProfile oceanWaveProfile;

        [Tooltip("Optional stone and boulder material template. A per-island copy is created at runtime.")]
        [SerializeField] private Material rockMaterial;

        [Tooltip("Optional generated-tree foliage material template. A per-island copy is created at runtime.")]
        [SerializeField] private Material treeFoliageMaterial;

        [Tooltip("Optional authored replacement for the generated 3D cliff-detail noise.")]
        [SerializeField] private Texture3D cliffDetailNoise;

        [Tooltip("Optional authored replacement for the generated river and shoreline noise.")]
        [SerializeField] private Texture2D riverNoise;

        [Tooltip("First colour used by broad grass variation.")]
        [SerializeField] private Color grassColourA = new Color(0.18f, 0.46f, 0.14f, 1f);

        [Tooltip("Second colour used by broad grass variation.")]
        [SerializeField] private Color grassColourB = new Color(0.34f, 0.50f, 0.14f, 1f);

        [Tooltip("World-space repeat size of the broad grass colour noise, in metres. The generated texture produces roughly eight broad patches per repeat.")]
        [Min(1f)]
        [SerializeField] private float grassColourNoiseWorldSizeMetres = 2048f;

        [Tooltip("World-space size of coherent sand patches, in metres.")]
        [Min(0.1f)]
        [SerializeField] private float sandPatchSizeMetres = 32f;

        [Tooltip("World-space size of coherent grass patches, in metres.")]
        [Min(0.1f)]
        [SerializeField] private float grassPatchSizeMetres = 32f;

        [Tooltip("Height above sea level over which river colour blends from the sea colour back to the river colour, in metres.")]
        [Min(0f)]
        [SerializeField] private float estuaryBlendHeightMetres = 2f;

        [Tooltip("Enable atmospheric haze while walking in first-person mode.")]
        [SerializeField] private bool showDistanceHaze = true;

        [Tooltip("Warm atmospheric colour accumulated by distant first-person views.")]
        [SerializeField] private Color distanceHazeColour = new Color(0.62f, 0.60f, 0.54f, 1f);

        [Tooltip("Density of the exponential-squared first-person haze.")]
        [Range(0.00005f, 0.003f)]
        [SerializeField] private float distanceHazeDensity = 0.00055f;


        [Tooltip("Real-time minutes taken for one complete sunrise-to-sunrise solar cycle.")]
        [Range(0.25f, 240f)]
        [SerializeField] private float sunCycleDurationMinutes = 20f;

        [Tooltip("How many times faster solar time passes at midnight than at noon. One gives a uniform clock while preserving the configured full-cycle duration.")]
        [Range(1f, 20f)]
        [SerializeField] private float midnightToNoonClockRateRatio = 10f;

        [Tooltip("Solar latitude in degrees. Higher absolute values produce a lower noon sun; the sign selects which side of the island it crosses.")]
        [Range(-80f, 80f)]
        [SerializeField] private float sunLatitudeDegrees = -36f;

        [Tooltip("Solar time used when play begins: 6 is sunrise, 12 is noon, and 18 is sunset.")]
        [Range(0f, 24f)]
        [SerializeField] private float startingSolarTimeHours = 8f;

        [Tooltip("Directional-light intensity when the sun is high in the sky.")]
        [Range(0f, 4f)]
        [SerializeField] private float middaySunIntensity = 1.25f;

        [Tooltip("Moon-orbit tilt toward the equator relative to the solar path.")]
        [Range(0f, 45f)]
        [SerializeField] private float moonEquatorOffsetDegrees = 22f;

        [Tooltip("Moon phase when play begins: 0 is new, 0.25 first quarter, 0.5 full, and 0.75 last quarter.")]
        [Range(0f, 1f)]
        [SerializeField] private float startingMoonPhase = 0.5f;

        [Tooltip("Directional-light intensity produced by a full moon after sunset.")]
        [Range(0f, 1f)]
        [SerializeField] private float fullMoonLightIntensity = 0.14f;

        [Tooltip("Fraction of sky cells containing a visible star at night.")]
        [Range(0f, 1f)]
        [SerializeField] private float starDensity = 0.18f;

        [Tooltip("Brightness of the procedural night stars.")]
        [Range(0f, 4f)]
        [SerializeField] private float starBrightness = 1.35f;

        [Tooltip("Apparent radius of procedural stars within their sky cells.")]
        [Range(0.02f, 0.12f)]
        [SerializeField] private float starSize = 0.052f;

        [Tooltip("Show the carved river surface.")]
        [SerializeField] private bool showRivers = true;

        [Tooltip("Show the sea surface.")]
        [SerializeField] private bool showSea = true;

        [Tooltip("Show generated grass shells.")]
        [SerializeField] private bool showGrass = true;

        [Tooltip("Show streamed stones and boulders.")]
        [SerializeField] private bool showRocks = true;

        public Material TerrainMaterial => terrainMaterial;
        public Material GrassMaterial => grassMaterial;
        public Material RiverMaterial => riverMaterial;
        public Material SeaMaterial => seaMaterial;
        public OceanWaveProfile OceanWaveProfile => oceanWaveProfile;
        public Material RockMaterial => rockMaterial;
        public Material TreeFoliageMaterial => treeFoliageMaterial;
        public Texture3D CliffDetailNoise => cliffDetailNoise;
        public Texture2D RiverNoise => riverNoise;
        public Color GrassColourA { get => grassColourA; set => grassColourA = value; }
        public Color GrassColourB { get => grassColourB; set => grassColourB = value; }
        public float GrassColourNoiseWorldSizeMetres
        {
            get => Mathf.Max(grassColourNoiseWorldSizeMetres, 1f);
            set => grassColourNoiseWorldSizeMetres = Mathf.Max(value, 1f);
        }
        internal float SandPatchSizeMetres => Mathf.Max(sandPatchSizeMetres, 0.1f);
        internal float GrassPatchSizeMetres => Mathf.Max(grassPatchSizeMetres, 0.1f);
        internal float EstuaryBlendHeightMetres => Mathf.Max(estuaryBlendHeightMetres, 0f);
        public bool ShowDistanceHaze
        {
            get => showDistanceHaze;
            set => showDistanceHaze = value;
        }
        public Color DistanceHazeColour
        {
            get => distanceHazeColour;
            set => distanceHazeColour = value;
        }
        public float DistanceHazeDensity
        {
            get => Mathf.Clamp(distanceHazeDensity, 0.00005f, 0.003f);
            set => distanceHazeDensity = Mathf.Clamp(value, 0.00005f, 0.003f);
        }
        public float SunCycleDurationMinutes
        {
            get => Mathf.Clamp(sunCycleDurationMinutes, 0.25f, 240f);
            set => sunCycleDurationMinutes = Mathf.Clamp(value, 0.25f, 240f);
        }
        public float MidnightToNoonClockRateRatio
        {
            get => midnightToNoonClockRateRatio > 0f
                ? Mathf.Clamp(midnightToNoonClockRateRatio, 1f, 20f)
                : 10f;
            set => midnightToNoonClockRateRatio = Mathf.Clamp(value, 1f, 20f);
        }
        public float SunLatitudeDegrees
        {
            get => Mathf.Clamp(sunLatitudeDegrees, -80f, 80f);
            set => sunLatitudeDegrees = Mathf.Clamp(value, -80f, 80f);
        }
        public float StartingSolarTimeHours
        {
            get => Mathf.Repeat(startingSolarTimeHours, 24f);
            set => startingSolarTimeHours = Mathf.Repeat(value, 24f);
        }
        public float MiddaySunIntensity
        {
            get => Mathf.Clamp(middaySunIntensity, 0f, 4f);
            set => middaySunIntensity = Mathf.Clamp(value, 0f, 4f);
        }
        public float MoonEquatorOffsetDegrees
        {
            get => Mathf.Clamp(moonEquatorOffsetDegrees, 0f, 45f);
            set => moonEquatorOffsetDegrees = Mathf.Clamp(value, 0f, 45f);
        }
        public float StartingMoonPhase
        {
            get => Mathf.Repeat(startingMoonPhase, 1f);
            set => startingMoonPhase = Mathf.Repeat(value, 1f);
        }
        public float FullMoonLightIntensity
        {
            get => Mathf.Clamp01(fullMoonLightIntensity);
            set => fullMoonLightIntensity = Mathf.Clamp01(value);
        }
        public float StarDensity
        {
            get => Mathf.Clamp01(starDensity);
            set => starDensity = Mathf.Clamp01(value);
        }
        public float StarBrightness
        {
            get => Mathf.Clamp(starBrightness, 0f, 4f);
            set => starBrightness = Mathf.Clamp(value, 0f, 4f);
        }
        public float StarSize
        {
            get => starSize > 0f ? Mathf.Clamp(starSize, 0.02f, 0.12f) : 0.052f;
            set => starSize = Mathf.Clamp(value, 0.02f, 0.12f);
        }
        public bool ShowRivers { get => showRivers; set => showRivers = value; }
        public bool ShowSea { get => showSea; set => showSea = value; }
        public bool ShowGrass { get => showGrass; set => showGrass = value; }
        public bool ShowRocks { get => showRocks; set => showRocks = value; }
        internal int MaterialTextureResolution => Mathf.Clamp(
            Mathf.ClosestPowerOfTwo(materialTextureResolution),
            128,
            2048);

        internal void AssignMaterialTemplates(
            Material terrain,
            Material grass,
            Material river,
            Material sea,
            Material rock,
            Material treeFoliage = null)
        {
            terrainMaterial = terrain;
            grassMaterial = grass;
            riverMaterial = river;
            seaMaterial = sea;
            rockMaterial = rock;
            treeFoliageMaterial = treeFoliage;
        }
    }
}
