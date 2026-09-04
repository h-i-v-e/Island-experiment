using UnityEngine;

public static class CelestialLighting
{
    internal const float LunarSynodicPeriodDays = 29.53059f;

    private const float NightSkyExposure = 0.045f;
    private static readonly Color MiddaySunColour = new Color(1f, 0.94f, 0.82f, 1f);
    private static readonly Color SunsetSunColour = new Color(1f, 0.20f, 0.035f, 1f);
    private static readonly Color DayAmbientColour = new Color(0.42f, 0.46f, 0.52f, 1f);
    private static readonly Color TwilightAmbientColour = new Color(0.08f, 0.15f, 0.30f, 1f);
    private static readonly Color NightAmbientColour = new Color(0.012f, 0.025f, 0.065f, 1f);

    public static float EvaluateClockRateMultiplier(
        float timeHours,
        float midnightToNoonRateRatio = 10f)
    {
        midnightToNoonRateRatio = Mathf.Clamp(
            midnightToNoonRateRatio,
            1f,
            20f);
        var angle = Mathf.Repeat(timeHours, 24f) * Mathf.PI / 12f;
        var midnightWeight = 0.5f + 0.5f * Mathf.Cos(angle);
        var relativeRate = Mathf.Lerp(
            1f,
            midnightToNoonRateRatio,
            midnightWeight);
        return relativeRate / Mathf.Sqrt(midnightToNoonRateRatio);
    }

    public static SolarState EvaluateSun(
        float timeHours,
        float latitudeDegrees,
        float middayIntensity)
    {
        var latitude = Mathf.Clamp(latitudeDegrees, -80f, 80f) * Mathf.Deg2Rad;
        var angle = (Mathf.Repeat(timeHours, 24f) - 6f) * Mathf.PI / 12f;
        var sinAngle = Mathf.Sin(angle);
        var localDirection = new Vector3(
            Mathf.Cos(angle),
            sinAngle * Mathf.Cos(latitude),
            -sinAngle * Mathf.Sin(latitude)).normalized;
        var elevation = localDirection.y;
        var daylight = SmoothRange(0f, 0.04f, elevation);
        var highSun = SmoothRange(0.02f, 0.35f, elevation);
        var sunColour = Color.Lerp(SunsetSunColour, MiddaySunColour, highSun);
        var sunIntensity = Mathf.Clamp(middayIntensity, 0f, 4f)
            * daylight
            * Mathf.Lerp(0.22f, 1f, SmoothRange(0f, 0.4f, elevation));

        var twilight = SmoothRange(-0.14f, 0.02f, elevation);
        var fullDay = SmoothRange(0.02f, 0.35f, elevation);
        var sunHaloStrength = SmoothRange(-0.08f, 0.02f, elevation)
            * (1f - SmoothRange(0.12f, 0.32f, elevation));
        var nightStrength = 1f - SmoothRange(-0.14f, 0f, elevation);
        var ambientColour = Color.Lerp(
            NightAmbientColour,
            TwilightAmbientColour,
            twilight);
        ambientColour = Color.Lerp(ambientColour, DayAmbientColour, fullDay);

        return new SolarState(
            localDirection,
            sunColour,
            ambientColour,
            sunIntensity,
            SmoothRange(-0.020f, 0.005f, elevation),
            Mathf.Lerp(
                NightSkyExposure,
                1f,
                SmoothRange(-0.16f, 0.20f, elevation)),
            sunHaloStrength,
            nightStrength);
    }

    public static MoonState EvaluateMoon(
        float timeHours,
        float solarLatitudeDegrees,
        float equatorOffsetDegrees,
        float phase,
        float fullMoonIntensity,
        float sunElevation)
    {
        var solarLatitude = Mathf.Clamp(solarLatitudeDegrees, -80f, 80f);
        var moonLatitude = Mathf.MoveTowards(
            solarLatitude,
            0f,
            Mathf.Clamp(equatorOffsetDegrees, 0f, 45f));
        var latitude = moonLatitude * Mathf.Deg2Rad;
        var phaseAngle = Mathf.Repeat(phase, 1f) * Mathf.PI * 2f;
        var orbitAngle = (Mathf.Repeat(timeHours, 24f) - 6f)
            * Mathf.PI / 12f
            - phaseAngle;
        var sinAngle = Mathf.Sin(orbitAngle);
        var cosAngle = Mathf.Cos(orbitAngle);
        var cosLatitude = Mathf.Cos(latitude);
        var sinLatitude = Mathf.Sin(latitude);
        var localDirection = new Vector3(
            cosAngle,
            sinAngle * cosLatitude,
            -sinAngle * sinLatitude).normalized;
        var orbitTangent = new Vector3(
            -sinAngle,
            cosAngle * cosLatitude,
            -cosAngle * sinLatitude).normalized;
        var phaseCosine = Mathf.Cos(phaseAngle);
        var localLightDirection = (
            localDirection * phaseCosine
            + orbitTangent * Mathf.Sin(phaseAngle)).normalized;
        var illumination = Mathf.Clamp01((1f - phaseCosine) * 0.5f);
        var altitudeVisibility = SmoothRange(-0.020f, 0.005f, localDirection.y);
        var daylightVisibility = Mathf.Lerp(
            0.18f,
            1f,
            1f - SmoothRange(-0.05f, 0.25f, sunElevation));
        var lightIntensity = Mathf.Clamp01(fullMoonIntensity)
            * Mathf.Pow(illumination, 1.5f)
            * SmoothRange(0f, 0.18f, localDirection.y);
        return new MoonState(
            localDirection,
            localLightDirection,
            moonLatitude,
            illumination,
            altitudeVisibility * daylightVisibility,
            lightIntensity);
    }

    private static float SmoothRange(float minimum, float maximum, float value)
    {
        var t = Mathf.InverseLerp(minimum, maximum, value);
        return t * t * (3f - 2f * t);
    }

    public readonly struct SolarState
    {
        public Vector3 LocalDirection { get; }
        public Color SunColour { get; }
        public Color AmbientColour { get; }
        public float SunIntensity { get; }
        public float SunVisibility { get; }
        public float SkyExposure { get; }
        public float SunHaloStrength { get; }
        public float NightStrength { get; }

        internal SolarState(
            Vector3 localDirection,
            Color sunColour,
            Color ambientColour,
            float sunIntensity,
            float sunVisibility,
            float skyExposure,
            float sunHaloStrength,
            float nightStrength)
        {
            LocalDirection = localDirection;
            SunColour = sunColour;
            AmbientColour = ambientColour;
            SunIntensity = sunIntensity;
            SunVisibility = sunVisibility;
            SkyExposure = skyExposure;
            SunHaloStrength = sunHaloStrength;
            NightStrength = nightStrength;
        }
    }

    public readonly struct MoonState
    {
        public Vector3 LocalDirection { get; }
        public Vector3 LocalLightDirection { get; }
        public float OrbitLatitudeDegrees { get; }
        public float Illumination { get; }
        public float Visibility { get; }
        public float LightIntensity { get; }

        internal MoonState(
            Vector3 localDirection,
            Vector3 localLightDirection,
            float orbitLatitudeDegrees,
            float illumination,
            float visibility,
            float lightIntensity)
        {
            LocalDirection = localDirection;
            LocalLightDirection = localLightDirection;
            OrbitLatitudeDegrees = orbitLatitudeDegrees;
            Illumination = illumination;
            Visibility = visibility;
            LightIntensity = lightIntensity;
        }
    }
}
