using System;
using UnityEngine;
using UnityEngine.Serialization;

namespace Motu.Settings
{
    [Serializable]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandCloudSettings")]
    public sealed class IslandCloudSettings
    {
        [SerializeField] private bool enabled = true;

        [Tooltip("Seed mixed with the global environment seed when generating the seamless weather field.")]
        [SerializeField] private int seed = 173;

        [Tooltip("Power-of-two resolution of the portable packed weather map.")]
        [Range(32, 1024)]
        [SerializeField] private int weatherMapResolution = 256;

        [Tooltip("Fraction of the weather field occupied by clouds.")]
        [Range(0f, 1f)]
        [SerializeField] private float coverage = 0.48f;

        [Tooltip("Cloud mass per metre. Denser or deeper clouds absorb more light.")]
        [Range(0f, 8f)]
        [SerializeField] private float density = 2.1f;

        [Tooltip("Height of the cloud layer above sea level in metres.")]
        [Range(50f, 1800f)]
        [SerializeField] private float altitudeMetres = 650f;

        [Tooltip("Vertical thickness of the ray-marched cloud layer in metres.")]
        [Range(25f, 1000f)]
        [SerializeField] private float verticalThicknessMetres = 280f;

        [Tooltip("World-space width and depth represented by one weather-map repeat.")]
        [Range(100f, 8000f)]
        [SerializeField] private float worldSizeMetres = 2200f;

        [Tooltip("Size of the broad, non-repeating cloud pattern relative to the weather map.")]
        [Range(2f, 16f)]
        [SerializeField] private float broadNoiseScale = 6f;

        [Tooltip("How strongly the broad cloud pattern breaks up repeated weather-map features.")]
        [Range(0f, 1f)]
        [SerializeField] private float broadNoiseStrength = 0.65f;

        [Tooltip("Influence of the medium-scale weather channel.")]
        [Range(0f, 1f)]
        [SerializeField] private float detailStrength = 0.52f;

        [Tooltip("Amount by which fine noise erodes cloud edges.")]
        [Range(0f, 1f)]
        [SerializeField] private float erosionStrength = 0.38f;

        [Tooltip("Size of the three-dimensional cloud billows in metres.")]
        [Range(50f, 1000f)]
        [SerializeField] private float billowSizeMetres = 250f;

        [Tooltip("Size of the fine shapes that erode the cloud edges in metres.")]
        [Range(10f, 300f)]
        [SerializeField] private float erosionSizeMetres = 50f;

        [Tooltip("Cloud profile: 0 is layered stratus, 0.5 is cumulus, 1 is towering storm cloud.")]
        [Range(0f, 1f)]
        [SerializeField] private float cloudType = 0.5f;

        [Tooltip("Strength of the three-dimensional billows within the weather pattern.")]
        [Range(0f, 1f)]
        [SerializeField] private float billowStrength = 0.65f;

        [Tooltip("How far sunlight penetrates clouds. Larger values soften dark interiors.")]
        [Range(0.25f, 3f)]
        [SerializeField] private float sunlightPenetration = 1f;

        [Tooltip("Difference in wind drift between the base and top of each cloud.")]
        [Range(0f, 1f)]
        [SerializeField] private float windShear = 0.15f;

        [Tooltip("Speed at which cloud shapes slowly evolve as they drift.")]
        [Range(0f, 1f)]
        [SerializeField] private float evolution = 0.1f;

        [Tooltip("Cloud rendering quality: 0 Low, 1 Balanced, 2 High.")]
        [Range(0, 2)]
        [SerializeField] private int quality = 1;

        [SerializeField] private Color dayColour = new Color(0.92f, 0.94f, 0.96f, 1f);
        [SerializeField] private Color sunsetColour = new Color(0.92f, 0.48f, 0.24f, 1f);
        [SerializeField] private Color nightColour = new Color(0.035f, 0.055f, 0.10f, 1f);

        [Tooltip("Maximum attenuation of direct light beneath dense clouds.")]
        [Range(0f, 1f)]
        [SerializeField] private float shadowStrength = 0.72f;

        [Tooltip("Weaker attenuation applied to ambient illumination beneath dense clouds.")]
        [Range(0f, 0.5f)]
        [SerializeField] private float ambientShadowStrength = 0.12f;

        [Tooltip("Strength with which cloud density obscures celestial discs and halos.")]
        [Range(0f, 2f)]
        [SerializeField] private float celestialObscurationStrength = 1f;

        [Tooltip("Source elevation below which long cloud-shadow projections fade out.")]
        [Range(0.01f, 0.35f)]
        [SerializeField] private float lowElevationShadowFade = 0.09f;

        public bool Enabled { get => enabled; set => enabled = value; }
        public int Seed { get => seed; set => seed = value; }
        public int WeatherMapResolution
        {
            get => Mathf.Clamp(Mathf.ClosestPowerOfTwo(weatherMapResolution), 32, 1024);
            set => weatherMapResolution = Mathf.Clamp(Mathf.ClosestPowerOfTwo(value), 32, 1024);
        }
        public float Coverage { get => Mathf.Clamp01(coverage); set => coverage = Mathf.Clamp01(value); }
        public float Density { get => Mathf.Clamp(density, 0f, 8f); set => density = Mathf.Clamp(value, 0f, 8f); }
        public float AltitudeMetres { get => Mathf.Clamp(altitudeMetres, 50f, 1800f); set => altitudeMetres = Mathf.Clamp(value, 50f, 1800f); }
        public float VerticalThicknessMetres
        {
            get => verticalThicknessMetres > 0f
                ? Mathf.Clamp(verticalThicknessMetres, 25f, 1000f)
                : 280f;
            set => verticalThicknessMetres = Mathf.Clamp(value, 25f, 1000f);
        }
        public float WorldSizeMetres { get => Mathf.Clamp(worldSizeMetres, 100f, 8000f); set => worldSizeMetres = Mathf.Clamp(value, 100f, 8000f); }
        public float BroadNoiseScale { get => Mathf.Clamp(broadNoiseScale, 2f, 16f); set => broadNoiseScale = Mathf.Clamp(value, 2f, 16f); }
        public float BroadNoiseStrength { get => Mathf.Clamp01(broadNoiseStrength); set => broadNoiseStrength = Mathf.Clamp01(value); }
        public float DetailStrength { get => Mathf.Clamp01(detailStrength); set => detailStrength = Mathf.Clamp01(value); }
        public float ErosionStrength { get => Mathf.Clamp01(erosionStrength); set => erosionStrength = Mathf.Clamp01(value); }
        public float BillowSizeMetres
        {
            get => billowSizeMetres == 0f ? 250f : Mathf.Clamp(billowSizeMetres, 50f, 1000f);
            set => billowSizeMetres = Mathf.Clamp(value, 50f, 1000f);
        }
        public float ErosionSizeMetres
        {
            get => erosionSizeMetres == 0f ? 50f : Mathf.Clamp(erosionSizeMetres, 10f, 300f);
            set => erosionSizeMetres = Mathf.Clamp(value, 10f, 300f);
        }
        public float CloudType { get => Mathf.Clamp01(cloudType); set => cloudType = Mathf.Clamp01(value); }
        public float BillowStrength { get => Mathf.Clamp01(billowStrength); set => billowStrength = Mathf.Clamp01(value); }
        public float SunlightPenetration
        {
            get => sunlightPenetration == 0f ? 1f : Mathf.Clamp(sunlightPenetration, 0.25f, 3f);
            set => sunlightPenetration = Mathf.Clamp(value, 0.25f, 3f);
        }
        public float WindShear { get => Mathf.Clamp01(windShear); set => windShear = Mathf.Clamp01(value); }
        public float Evolution { get => Mathf.Clamp01(evolution); set => evolution = Mathf.Clamp01(value); }
        public int Quality { get => Mathf.Clamp(quality, 0, 2); set => quality = Mathf.Clamp(value, 0, 2); }
        public Color DayColour { get => dayColour; set => dayColour = value; }
        public Color SunsetColour { get => sunsetColour; set => sunsetColour = value; }
        public Color NightColour { get => nightColour; set => nightColour = value; }
        public float ShadowStrength { get => Mathf.Clamp01(shadowStrength); set => shadowStrength = Mathf.Clamp01(value); }
        public float AmbientShadowStrength { get => Mathf.Clamp(ambientShadowStrength, 0f, 0.5f); set => ambientShadowStrength = Mathf.Clamp(value, 0f, 0.5f); }
        public float CelestialObscurationStrength { get => Mathf.Clamp(celestialObscurationStrength, 0f, 2f); set => celestialObscurationStrength = Mathf.Clamp(value, 0f, 2f); }
        public float LowElevationShadowFade { get => Mathf.Clamp(lowElevationShadowFade, 0.01f, 0.35f); set => lowElevationShadowFade = Mathf.Clamp(value, 0.01f, 0.35f); }
    }
}
