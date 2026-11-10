using System;
using UnityEngine;

namespace Motu.World
{
    /// <summary>Travelling storm cells with a bounded active count. Distances use world metres.</summary>
    [Serializable]
    public struct SquallWeatherSettings
    {
        public const int MaximumSupportedSqualls = 3;
        public const float MinimumSupportedDiameterMetres = 200f;
        public const float MaximumSupportedDiameterMetres = 2000f;
        public const float MaximumSupportedSpeedMultiplier = 8f;
        [HideInInspector] public int Version;
        [Tooltip("Enable travelling storm clouds, rain, lightning and local wind/wave effects. Turning this off clears active squalls and blocks both automatic and manual spawning.")]
        public bool Enabled;
        [Tooltip("Allow new squalls to form automatically, up to the active limit. A coherent weather driver ties formation to cloud cover and wind; a standalone environment uses a random interval between the minimum and maximum. Turn off to test only manually spawned storms.")]
        public bool AutomaticSpawning;
        [Tooltip("Upper limit on simultaneous squalls, from 1 to 3. Coherent weather may support fewer cells in milder conditions. Overlapping storms use the strongest wind/wave boost rather than multiplying their boosts together.")]
        [Range(1, MaximumSupportedSqualls)] public int MaximumActiveSqualls;
        [Tooltip("Smallest rain and wind footprint for a newly spawned squall, measured across the whole storm in metres (200-2,000 m). Each new cell chooses a size between the minimum and maximum. The lighter cloud fringe extends beyond this footprint.")]
        [Range(MinimumSupportedDiameterMetres, MaximumSupportedDiameterMetres)] public float MinimumDiameterMetres;
        [Tooltip("Largest diameter of a newly spawned rain and wind footprint, in metres (200-2,000 m). A 2,000 m storm has a 1,000 m core radius before its outer cloud fringe. Changing this range does not resize cells already spawned.")]
        [Range(MinimumSupportedDiameterMetres, MaximumSupportedDiameterMetres)] public float MaximumDiameterMetres;
        [Tooltip("Shortest automatic formation interval, in seconds. Coherent weather approaches this interval in the strongest storm-forming conditions; the standalone weather test instead chooses randomly between the two interval limits. Shorter intervals refill available storm slots sooner.")]
        [Min(.1f)] public float MinimumSpawnIntervalSeconds;
        [Tooltip("Longest automatic formation interval, in seconds. Coherent weather approaches this interval just above its formation threshold; fair weather stops formation and resets its progress. Standalone testing uses this as the upper end of its random interval range.")]
        [Min(.1f)] public float MaximumSpawnIntervalSeconds;
        [Tooltip("Nearest permitted distance from the ship, or the test camera, to a new storm centre, in metres. The bearing comes from the upwind spawn arc. Distances are capped at 85% of Visibility Distance so newly spawned storms are within visual range.")]
        [Min(10f)] public float MinimumSpawnDistanceMetres;
        [Tooltip("Farthest permitted distance to a new storm centre, in metres. New cells choose a distance between the two limits; existing cells keep moving normally. This is capped at 85% of Visibility Distance and cannot be less than Minimum Spawn Distance.")]
        [Min(10f)] public float MaximumSpawnDistanceMetres;
        [Tooltip("Full width of the spawning arc, centred directly upwind using the current wind direction. The default 90 degrees means 45 degrees either side of upwind; 0 spawns directly upwind, and 360 allows the entire horizon. Explicitly positioned storms bypass this arc.")]
        [Range(0f, 360f)] public float SpawnArcDegrees;
        [Tooltip("Slowest travel speed as a multiple of ambient wind speed (0-8). A value of 1 travels at wind speed; 0 permits stopped phases. Coherent noise blends between the speed limits, and slower phases reduce lightning frequency and the local wind/wave boost.")]
        [Range(0f, MaximumSupportedSpeedMultiplier)] public float MinimumSpeedMultiplier;
        [Tooltip("Fastest travel speed as a multiple of ambient wind speed, up to 8. For example, 8 at 10 m/s wind gives 80 m/s travel. At this maximum, the storm reaches its configured peak wind boost and lightning rate; its boosted internal wind does not multiply travel speed again.")]
        [Range(0f, MaximumSupportedSpeedMultiplier)] public float MaximumSpeedMultiplier;
        [Tooltip("Time scale of the coherent noise that changes each storm's speed, in seconds. Larger values give longer, steadier fast and slow phases; smaller values change speed more often. This is a smooth noise time scale, not a repeating cycle.")]
        [Min(1f)] public float SpeedVariationSeconds;
        [Tooltip("Maximum wandering angle either side of the current downwind direction (0-45 degrees). At 45 the full heading cone is 90 degrees wide; 0 keeps travel directly downwind. This controls movement after spawning, independently of the upwind spawn arc.")]
        [Range(0f, 45f)] public float MaximumHeadingDeviationDegrees;
        [Tooltip("Time scale of coherent heading changes, in seconds. Larger values give broad, slow bends; smaller values make storms wander more frequently. Each cell has its own smooth heading noise, and the cone follows changes in the ambient wind direction.")]
        [Min(1f)] public float HeadingVariationSeconds;
        [Tooltip("Minimum age in seconds before an out-of-range cell may retire. This is not a fixed despawn timer: encounterable storms remain alive while weather supports them. Improving coherent weather can still dissolve excess cells, even near the ship.")]
        [Min(5f)] public float LifetimeSeconds;
        [Tooltip("Seconds for a cell to fade in, retire, and blend changes in support from the coherent weather. Longer values make changes more gradual; 0 makes them immediate. Retirement starts only when lifetime/distance or changing weather allows it.")]
        [Min(0f)] public float FadeSeconds;
        [Tooltip("Fraction of the core radius used to blend boosted wind and waves back to ambient conditions (0-0.5). At 0.25 the outer quarter is the transition band; 0 gives a hard boundary. Rain has its own gradual fade across the full core, separate from this wind/wave edge.")]
        [Range(0f, .5f)] public float EdgeFraction;
        [Tooltip("Peak local wind-speed multiplier at the centre of a full-strength storm travelling at its maximum speed (1-5). A value of 4 gives four times ambient wind at the peak. Ships and the sea sample this field; waves grow, sharpen and make more foam. Slower or fading cells weaken the boost.")]
        [Range(1f, 5f)] public float WindMultiplier;
        [Tooltip("Height of the storm cloud underside above sea level, in metres. Rendering uses a minimum of 15 m. Rain falls through the volume from this cloud base to the sea, so lowering it brings the cloud and rain ceiling closer to the deck.")]
        [Min(2f)] public float CloudBaseMetres;
        [Tooltip("Baseline vertical thickness of the storm cloud, in metres. Rendering uses at least 10 m before scaling thickness with storm diameter, so larger storms also look deeper. Increase for a taller, bulkier cloud; decrease for a flatter low cloud.")]
        [Min(5f)] public float CloudThicknessMetres;
        [Tooltip("Darkness of the main storm cloud and strength of its projected shadow (0-1). Higher values give a darker cloud and deeper shade over ship, sea and land; zero removes the storm shadow. Time-of-day lighting still affects the final appearance.")]
        [Range(0f, 1f)] public float CloudDarkness;
        [Tooltip("Extra radius of lighter, patchy cloud around the rain core, expressed as a fraction of the core radius (0-1.5). At 0.65 the cloud extends 65% beyond that radius; 0 removes the outer fringe. This softens the silhouette without enlarging the rain or wind footprint.")]
        [Range(0f, 1.5f)] public float CloudFringeFraction;
        [Tooltip("Opacity multiplier applied to distant storm clouds and their rain curtain (0-1). Lower values blend far-off storms into the haze; higher values keep them more solid. Nearby storms regain normal opacity, and the final distance fade still applies before Visibility Distance.")]
        [Range(0f, 1f)] public float DistantOpacity;
        [Tooltip("Rain amount multiplier (0-4): 0 disables rain, 1 is normal, and values above 1 make it heavier. Affects both the storm-wide rain volume and nearby droplets. Rain softens towards the core edge, follows wind drift and drives surface wetness; cloud, wind and lightning remain separate.")]
        [Range(0f, 4f)] public float RainIntensity;
        [Header("Surface wetness")]
        [Tooltip("Strength of rain-driven surface darkening and wet sheen (0-1). Zero hides the effect. This is a camera-local screen-space treatment for opaque ground, rocks and decks, rather than wetness stored separately on every object; it builds and dries using the times below.")]
        [Range(0f, 1f)] public float SurfaceWetness;
        [Tooltip("Seconds of full local rain at intensity 1 needed to reach full wetness. Heavier rain wets surfaces faster; lighter rain accumulates more slowly but can still soak surfaces fully. Existing wetness never dries while any rain continues.")]
        [Min(.1f)] public float WettingSeconds;
        [Tooltip("Seconds for fully wet surfaces to dry after rainfall stops. Drying first waits about 1.35 seconds for the last airborne droplets to finish. New rain interrupts drying; longer values keep the wet sheen after the storm has passed.")]
        [Min(.1f)] public float DryingSeconds;
        [Header("Lightning")]
        [Tooltip("Typical seconds between strikes at the configured maximum travel speed. Frequency scales down with the speed multiplier, and stopped storms do not strike. Individual strikes vary their timing; each visible strike can queue one distance-delayed thunder clap.")]
        [Min(.2f)] public float LightningIntervalSeconds;
        [Tooltip("Brightness of the lightning bolts and their brief illumination of the ship and scenery (0-20). The flash covers the storm footprint and casts local shadows. Higher values intensify strikes without changing frequency. Zero suppresses visible lightning and associated thunder; sound loudness is set in Squall Audio.")]
        [Range(0f, 20f)] public float LightningBrightness;
        [Tooltip("Storm-centre distance in metres at which the storm begins sinking behind the simulated horizon as it recedes. Closer storms remain at normal height; between this distance and Visibility Distance they progressively disappear below the horizon. This is not the spawn distance.")]
        [Min(100f)] public float HorizonStartMetres;
        [Tooltip("Farthest visible storm-centre distance, in metres; clouds and rain fade and sink before this cutoff. Also caps automatic spawn distances at 85% of this range and sets the far thunder-bank boundary at half the range. Thunder hearing distance is configured separately in Squall Audio.")]
        [Min(101f)] public float VisibilityDistanceMetres;

        public static SquallWeatherSettings Default => new SquallWeatherSettings
        {
            Version = 5, Enabled = true, AutomaticSpawning = true, MaximumActiveSqualls = 3,
            MinimumDiameterMetres = MinimumSupportedDiameterMetres, MaximumDiameterMetres = MaximumSupportedDiameterMetres,
            MinimumSpawnIntervalSeconds = 45f, MaximumSpawnIntervalSeconds = 150f,
            MinimumSpawnDistanceMetres = 2400f, MaximumSpawnDistanceMetres = 3200f,
            SpawnArcDegrees = 90f,
            MinimumSpeedMultiplier = 0f, MaximumSpeedMultiplier = MaximumSupportedSpeedMultiplier, SpeedVariationSeconds = 45f,
            MaximumHeadingDeviationDegrees = 45f, HeadingVariationSeconds = 90f,
            LifetimeSeconds = 600f, FadeSeconds = 6f,
            EdgeFraction = .25f, WindMultiplier = 4f,
            CloudBaseMetres = 60f, CloudThicknessMetres = 100f, CloudDarkness = .9f,
            CloudFringeFraction = .65f, DistantOpacity = .18f,
            RainIntensity = 1f, LightningIntervalSeconds = 2.5f, LightningBrightness = 5f,
            SurfaceWetness = .75f, WettingSeconds = 6f, DryingSeconds = 40f,
            HorizonStartMetres = 1000f, VisibilityDistanceMetres = 4000f,
        };

        internal SquallWeatherSettings Validated()
        {
            var result = this;
            if (result.Version < 5) result.Upgrade();
            result.MaximumActiveSqualls = Mathf.Clamp(result.MaximumActiveSqualls, 1, MaximumSupportedSqualls);
            result.MinimumDiameterMetres = Clamp(MinimumDiameterMetres, MinimumSupportedDiameterMetres, MaximumSupportedDiameterMetres);
            result.MaximumDiameterMetres = Clamp(MaximumDiameterMetres, result.MinimumDiameterMetres, MaximumSupportedDiameterMetres);
            result.MinimumSpawnIntervalSeconds = Clamp(MinimumSpawnIntervalSeconds, .1f, 3600f);
            result.MaximumSpawnIntervalSeconds = Clamp(MaximumSpawnIntervalSeconds, result.MinimumSpawnIntervalSeconds, 3600f);
            result.HorizonStartMetres = Clamp(HorizonStartMetres, 100f, 100000f);
            result.VisibilityDistanceMetres = Clamp(VisibilityDistanceMetres, result.HorizonStartMetres + 1f, 200000f);
            float spawnLimit = result.VisibilityDistanceMetres * .85f;
            result.MinimumSpawnDistanceMetres = Clamp(MinimumSpawnDistanceMetres, 10f, spawnLimit);
            result.MaximumSpawnDistanceMetres = Clamp(MaximumSpawnDistanceMetres, result.MinimumSpawnDistanceMetres, spawnLimit);
            result.SpawnArcDegrees = Clamp(result.SpawnArcDegrees, 0f, 360f);
            result.MinimumSpeedMultiplier = Clamp(result.MinimumSpeedMultiplier, 0f, MaximumSupportedSpeedMultiplier);
            result.MaximumSpeedMultiplier = Clamp(result.MaximumSpeedMultiplier, result.MinimumSpeedMultiplier, MaximumSupportedSpeedMultiplier);
            result.SpeedVariationSeconds = Clamp(result.SpeedVariationSeconds, 1f, 600f);
            result.MaximumHeadingDeviationDegrees = Clamp(result.MaximumHeadingDeviationDegrees, 0f, 45f);
            result.HeadingVariationSeconds = Clamp(result.HeadingVariationSeconds, 1f, 600f);
            result.LifetimeSeconds = Clamp(LifetimeSeconds, 5f, 3600f);
            result.FadeSeconds = Clamp(FadeSeconds, 0f, result.LifetimeSeconds * .45f);
            result.EdgeFraction = Clamp(EdgeFraction, 0f, .5f);
            result.WindMultiplier = Clamp(WindMultiplier, 1f, 5f);
            result.CloudBaseMetres = Clamp(CloudBaseMetres, 2f, 500f);
            result.CloudThicknessMetres = Clamp(CloudThicknessMetres, 5f, 1000f);
            result.CloudDarkness = Clamp(CloudDarkness, 0f, 1f);
            result.CloudFringeFraction = Clamp(result.CloudFringeFraction, 0f, 1.5f);
            result.DistantOpacity = Clamp(result.DistantOpacity, 0f, 1f);
            result.RainIntensity = Clamp(RainIntensity, 0f, 4f);
            result.SurfaceWetness = Clamp(result.SurfaceWetness, 0f, 1f);
            result.WettingSeconds = Clamp(result.WettingSeconds, .1f, 120f);
            result.DryingSeconds = Clamp(result.DryingSeconds, .1f, 3600f);
            result.LightningIntervalSeconds = Clamp(LightningIntervalSeconds, .2f, 60f);
            result.LightningBrightness = Clamp(LightningBrightness, 0f, 20f);
            return result;
        }

        private void Upgrade()
        {
            var defaults = Default;
            if (Version == 0)
            {
                MinimumSpeedMultiplier = defaults.MinimumSpeedMultiplier;
                MaximumSpeedMultiplier = defaults.MaximumSpeedMultiplier;
                SpeedVariationSeconds = defaults.SpeedVariationSeconds;
                CloudFringeFraction = defaults.CloudFringeFraction;
                DistantOpacity = defaults.DistantOpacity;
            }
            if (Version < 2 && MaximumActiveSqualls <= 0) MaximumActiveSqualls = defaults.MaximumActiveSqualls;
            if (Version < 3)
            {
                MaximumHeadingDeviationDegrees = defaults.MaximumHeadingDeviationDegrees;
                HeadingVariationSeconds = defaults.HeadingVariationSeconds;
            }
            if (Version < 4)
            {
                SurfaceWetness = defaults.SurfaceWetness;
                WettingSeconds = defaults.WettingSeconds;
                DryingSeconds = defaults.DryingSeconds;
            }
            SpawnArcDegrees = defaults.SpawnArcDegrees;
            Version = defaults.Version;
        }

        private static float Clamp(float value, float minimum, float maximum)
        {
            WeatherValueValidation.RequireFinite(value, nameof(SquallWeatherSettings));
            return Mathf.Clamp(value, minimum, maximum);
        }
    }
}
