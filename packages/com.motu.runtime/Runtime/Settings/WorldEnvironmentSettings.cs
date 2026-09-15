using System;
using UnityEngine;

namespace Motu.Settings
{
    [Serializable]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "WorldEnvironmentSettings")]
    public sealed class WorldEnvironmentSettings
    {
        [Tooltip("Seed for global sky and weather variation. Island seeds belong to the request factory.")]
        [SerializeField] private int seed = 8675309;
        [SerializeField] private Color zenithColour = new Color(0.49f, 0.68f, 0.82f, 1f);
        [SerializeField] private Color distanceHazeColour = new Color(0.62f, 0.60f, 0.54f, 1f);
        [Tooltip("Lightening of the sky dome horizon relative to distance haze and LOD3 islands. 0 matches the haze; 0.08 is 8% brighter; 1 doubles the brightness.")]
        [UnityEngine.Serialization.FormerlySerializedAs("islandLod3Darkening")]
        [Range(0f, 1f)]
        [SerializeField] private float skyHorizonLightening = 0.08f;
        [Range(0.00005f, 0.003f)]
        [SerializeField] private float distanceHazeDensity = 0.00055f;
        [SerializeField] private bool showDistanceHaze = true;
        [SerializeField] private bool showSea = true;
        [SerializeField] private float seaLevelMetres;
        [SerializeField] private Material seaMaterial;
        [Tooltip("Optional global coherent noise shared by ocean waves and vegetation wind. Red and green drive structure; blue supplies broad grass colour variation. A suitable texture is generated when unset.")]
        [SerializeField] private Texture2D weatherNoise;
        [SerializeField] private OceanWaveProfile oceanWaveProfile;
        [Tooltip("Direction the wind blows towards, in world X/Z coordinates.")]
        [SerializeField] private Vector2 windDirection = new Vector2(1f, 0.25f);
        [Tooltip("Single shared wind strength in metres per second for grass, trees, reeds, ferns, clouds and waves. Zero is calm.")]
        [Range(0f, 40f)]
        [SerializeField] private float windSpeedMetresPerSecond = 9f;
        [Tooltip("World-space size of the coherent gust field shared by every wind-animated material.")]
        [Range(1f, 64f)]
        [SerializeField] private float windGustSizeMetres = 12f;
        [Tooltip("Height above each tree root that remains pinned against wind.")]
        [Range(0f, 4f)]
        [SerializeField] private float treeWindBasePinHeightMetres = 0.6f;
        [Tooltip("Height above each tree root at which full tree bending is reached.")]
        [Range(1f, 24f)]
        [SerializeField] private float treeWindFullBendHeightMetres = 9f;
        [SerializeField] private Light sunlight;
        [Range(0.25f, 240f)] [SerializeField] private float sunCycleDurationMinutes = 20f;
        [Range(1f, 20f)] [SerializeField] private float midnightToNoonClockRateRatio = 10f;
        [Range(-80f, 80f)] [SerializeField] private float sunLatitudeDegrees = -36f;
        [Range(0f, 24f)] [SerializeField] private float startingSolarTimeHours = 8f;
        [Range(0f, 4f)] [SerializeField] private float middaySunIntensity = 1.25f;
        [Range(0f, 45f)] [SerializeField] private float moonEquatorOffsetDegrees = 22f;
        [Range(0f, 1f)] [SerializeField] private float startingMoonPhase = 0.5f;
        [Range(0f, 1f)] [SerializeField] private float fullMoonLightIntensity = 0.14f;
        [Range(0f, 1f)] [SerializeField] private float starDensity = 0.18f;
        [Range(0f, 4f)] [SerializeField] private float starBrightness = 1.35f;
        [Range(0.02f, 0.12f)] [SerializeField] private float starSize = 0.052f;

        public int Seed => seed;
        public Color ZenithColour => zenithColour;
        public Color DistanceHazeColour => distanceHazeColour;
        public float SkyHorizonLightening
        {
            get => Mathf.Clamp01(skyHorizonLightening);
            set => skyHorizonLightening = Mathf.Clamp01(value);
        }
        public float DistanceHazeDensity => Mathf.Clamp(distanceHazeDensity, 0.00005f, 0.003f);
        public bool ShowDistanceHaze => showDistanceHaze;
        public bool ShowSea => showSea;
        public float SeaLevelMetres => float.IsFinite(seaLevelMetres) ? seaLevelMetres : 0f;
        public Material SeaMaterial => seaMaterial;
        public Texture2D WeatherNoise => weatherNoise;
        public OceanWaveProfile OceanWaveProfile => oceanWaveProfile;
        public Vector2 WindDirection
        {
            get => windDirection;
            set => windDirection = value;
        }
        public float WindSpeedMetresPerSecond
        {
            get => Mathf.Clamp(windSpeedMetresPerSecond, 0f, 40f);
            set => windSpeedMetresPerSecond = Mathf.Clamp(value, 0f, 40f);
        }
        public float WindGustSizeMetres
        {
            get => Mathf.Clamp(windGustSizeMetres, 1f, 64f);
            set => windGustSizeMetres = Mathf.Clamp(value, 1f, 64f);
        }
        public float TreeWindBasePinHeightMetres =>
            Mathf.Clamp(treeWindBasePinHeightMetres, 0f, 4f);
        public float TreeWindFullBendHeightMetres => Mathf.Clamp(
            treeWindFullBendHeightMetres,
            Mathf.Max(TreeWindBasePinHeightMetres + 0.01f, 1f),
            24f);
        public Light Sunlight { get => sunlight; internal set => sunlight = value; }
        public float SunCycleDurationMinutes => Mathf.Clamp(sunCycleDurationMinutes, 0.25f, 240f);
        public float MidnightToNoonClockRateRatio => Mathf.Clamp(midnightToNoonClockRateRatio, 1f, 20f);
        public float SunLatitudeDegrees => Mathf.Clamp(sunLatitudeDegrees, -80f, 80f);
        public float StartingSolarTimeHours => Mathf.Repeat(startingSolarTimeHours, 24f);
        public float MiddaySunIntensity => Mathf.Clamp(middaySunIntensity, 0f, 4f);
        public float MoonEquatorOffsetDegrees => Mathf.Clamp(moonEquatorOffsetDegrees, 0f, 45f);
        public float StartingMoonPhase => Mathf.Repeat(startingMoonPhase, 1f);
        public float FullMoonLightIntensity => Mathf.Clamp01(fullMoonLightIntensity);
        public float StarDensity => Mathf.Clamp01(starDensity);
        public float StarBrightness => Mathf.Clamp(starBrightness, 0f, 4f);
        public float StarSize => Mathf.Clamp(starSize, 0.02f, 0.12f);

        internal void AssignSceneReferences(Light sun, Material sea)
        {
            sunlight = sun;
            seaMaterial = sea;
        }
    }
}
