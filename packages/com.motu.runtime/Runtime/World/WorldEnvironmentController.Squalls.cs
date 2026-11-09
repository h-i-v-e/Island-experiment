using System;
using Motu.Rendering;
using Motu.Settings;
using UnityEngine;

namespace Motu.World
{
    public sealed partial class WorldEnvironmentController
    {
        [Tooltip("Optional ship around which squalls spawn. When empty, the environment's follow target is used.")]
        [SerializeField] private Transform squallSpawnTarget;
        [Tooltip("Thunder recording and propagation settings. Defaults to Resources/SquallAudio.")]
        [SerializeField] private SquallAudioProfile squallAudioProfile;
        private SquallThunderAudio squallThunder;
        private bool searchedSquallAudio;
        private readonly WorldSquallState[] squalls = new WorldSquallState[SquallWeatherSettings.MaximumSupportedSqualls];
        private readonly SquallVisualRenderer[] squallVisuals = new SquallVisualRenderer[SquallWeatherSettings.MaximumSupportedSqualls];
        private readonly float[] squallRetirementTimes = new float[SquallWeatherSettings.MaximumSupportedSqualls];
        private System.Random squallRandom;
        private int squallSequenceSeed, activeSquallCount;
        private float squallSpawnCountdown, squallLifetime;

        /// <summary>The first active cell, for existing single-cell consumers. Use GetSquall to enumerate all cells.</summary>
        public WorldSquallState Squall => activeSquallCount > 0 ? GetSquall(0) : default;
        public int ActiveSquallCount => activeSquallCount;
        public Transform SquallSpawnTarget { get => squallSpawnTarget; set => squallSpawnTarget = value; }
        private Transform SquallTarget => squallSpawnTarget != null ? squallSpawnTarget : followTarget;

        /// <summary>Local rainfall shared by visible drops and screen-space surface wetness.</summary>
        public float SquallRainIntensityAt(Vector3 worldPosition)
        {
            if (!weather.Squalls.Enabled) return 0;
            var rain = 0f;
            foreach (var cell in squalls)
            {
                if (!cell.Active) continue;
                var drift = WorldSquallRain.Drift(weather.WindDirection * WindSpeedAt(cell.Center));
                rain += WorldSquallRain.IntensityAt(cell, weather.Squalls, worldPosition, drift,
                    cell.Center.y + Mathf.Max(15, weather.Squalls.CloudBaseMetres));
            }
            return rain;
        }

        public WorldSquallState GetSquall(int index)
        {
            if (index < 0 || index >= activeSquallCount) throw new ArgumentOutOfRangeException(nameof(index));
            for (var slot = 0; slot < squalls.Length; slot++)
                if (squalls[slot].Active && index-- == 0) return squalls[slot];
            throw new InvalidOperationException("Squall count does not match its active cells.");
        }

        /// <summary>The strongest local cell wins; overlapping storms never multiply their gains.</summary>
        public float SquallWindMultiplierAt(Vector2 worldPosition)
        {
            if (!isActiveAndEnabled) return 1f;
            var multiplier = 1f;
            for (var slot = 0; slot < squalls.Length; slot++)
                if (squalls[slot].Active) multiplier = Mathf.Max(multiplier, squalls[slot].Field.MultiplierAt(worldPosition));
            return multiplier;
        }

        public float WindSpeedAt(Vector3 worldPosition) => isActiveAndEnabled
            ? WindSpeedMetresPerSecond * SquallWindMultiplierAt(new Vector2(worldPosition.x, worldPosition.z)) : 0f;

        private void InitializeSqualls()
        {
            if (squallRandom != null && squallSequenceSeed == environmentSeed) return;
            squallSequenceSeed = environmentSeed;
            squallRandom = new System.Random(environmentSeed ^ 0x51A117);
            Array.Clear(squalls, 0, squalls.Length);
            Array.Clear(squallRetirementTimes, 0, squallRetirementTimes.Length);
            activeSquallCount = 0;
            ScheduleNextSquall();
        }

        private float SquallRange(float minimum, float maximum) =>
            Mathf.Lerp(minimum, maximum, (float)squallRandom.NextDouble());

        private void ScheduleNextSquall()
        {
            var settings = weather.Squalls;
            squallSpawnCountdown = SquallRange(settings.MinimumSpawnIntervalSeconds, settings.MaximumSpawnIntervalSeconds);
        }

        /// <summary>Spawns a cell at a random horizon bearing, keeping new rain cores apart.</summary>
        public bool SpawnSquall() => SpawnSquall(out _);

        public bool SpawnSquall(out WorldSquallState spawned)
        {
            spawned = default;
            if (!CanSpawnSquall() || SquallTarget == null) return false;
            InitializeSqualls();
            var settings = weather.Squalls;
            var diameter = SquallRange(settings.MinimumDiameterMetres, settings.MaximumDiameterMetres);
            for (var attempt = 0; attempt < 8; attempt++)
            {
                var bearing = SquallRange(0f, Mathf.PI * 2f);
                var radial = new Vector3(Mathf.Sin(bearing), 0f, Mathf.Cos(bearing));
                var center = SquallTarget.position + radial
                    * SquallRange(settings.MinimumSpawnDistanceMetres, settings.MaximumSpawnDistanceMetres);
                if (SquallSpawnIsClear(center, diameter)) return SpawnSquall(center, diameter, out spawned);
            }
            return false;
        }

        private bool SquallSpawnIsClear(Vector3 center, float diameter)
        {
            for (var slot = 0; slot < squalls.Length; slot++)
            {
                var cell = squalls[slot];
                if (!cell.Active) continue;
                var separation = new Vector2(center.x - cell.Center.x, center.z - cell.Center.z);
                var minimum = (diameter + cell.DiameterMetres) * .5f + 100f;
                if (separation.sqrMagnitude < minimum * minimum) return false;
            }
            return true;
        }

        /// <summary>Places a cell explicitly; authored cells may overlap, subject to the active limit.</summary>
        public bool SpawnSquall(Vector3 worldCenter, float diameterMetres) =>
            SpawnSquall(worldCenter, diameterMetres, out _);

        public bool SpawnSquall(Vector3 worldCenter, float diameterMetres, out WorldSquallState spawned)
        {
            spawned = default;
            if (!CanSpawnSquall()) return false;
            WeatherValueValidation.RequireFinite(worldCenter.x, nameof(worldCenter));
            WeatherValueValidation.RequireFinite(worldCenter.y, nameof(worldCenter));
            WeatherValueValidation.RequireFinite(worldCenter.z, nameof(worldCenter));
            WeatherValueValidation.RequireFinite(diameterMetres, nameof(diameterMetres));
            InitializeSqualls();
            for (var slot = 0; slot < squalls.Length; slot++)
            {
                if (squalls[slot].Active) continue;
                squallLifetime = weather.Squalls.LifetimeSeconds;
                squallRetirementTimes[slot] = 0f;
                squalls[slot] = MakeSquall(worldCenter, Mathf.Clamp(diameterMetres,
                    SquallWeatherSettings.MinimumSupportedDiameterMetres, SquallWeatherSettings.MaximumSupportedDiameterMetres), 0f,
                    squallRandom.Next(), 0f, SquallWeatherStrength(SquallWeatherActivity(), activeSquallCount));
                activeSquallCount++;
                spawned = squalls[slot];
                PublishSquallField();
                UpdateSquallVisuals(0f);
                return true;
            }
            return false;
        }

        private bool CanSpawnSquall() => hostState != null && IsInstalled && isActiveAndEnabled
            && weather.Squalls.Enabled && activeSquallCount < weather.Squalls.MaximumActiveSqualls;

        /// <summary>Clears all active squalls and restarts the standalone automatic clock.</summary>
        public void ClearSquall()
        {
            Array.Clear(squalls, 0, squalls.Length);
            Array.Clear(squallRetirementTimes, 0, squallRetirementTimes.Length);
            activeSquallCount = 0;
            if (squallRandom != null) ScheduleNextSquall();
            PublishSquallField();
            UpdateSquallVisuals(0f);
        }

        internal void AdvanceSqualls(float deltaTime)
        {
            if (deltaTime < 0f || !float.IsFinite(deltaTime)) throw new ArgumentOutOfRangeException(nameof(deltaTime));
            InitializeSqualls();
            if (!weather.Squalls.Enabled)
            {
                if (activeSquallCount > 0) ClearSquall();
                return;
            }
            var hadActiveCells = activeSquallCount > 0;
            var activity = SquallWeatherActivity();
            var rank = 0;
            for (var slot = 0; slot < squalls.Length; slot++)
                if (squalls[slot].Active) MoveSquall(slot, deltaTime, SquallWeatherStrength(activity, rank++));
            if (hadActiveCells && activeSquallCount == 0) ScheduleNextSquall();
            else AdvanceSquallSpawning(deltaTime);
            PublishSquallField();
        }

        private void AdvanceSquallSpawning(float deltaTime)
        {
            var driver = WeatherDriver;
            if (driver != null && driver.ControlsSquallSpawning)
            {
                if (driver.isActiveAndEnabled) driver.UpdateSquallSpawning(this, deltaTime);
                return;
            }
            // A standalone environment (including the weather playground) keeps
            // its explicit automatic preview controls when no driver owns them.
            if (!weather.Squalls.AutomaticSpawning || SquallTarget == null
                || activeSquallCount >= weather.Squalls.MaximumActiveSqualls) return;
            squallSpawnCountdown -= deltaTime;
            if (squallSpawnCountdown <= 0f && SpawnSquall()) ScheduleNextSquall();
        }

        private float SquallWeatherActivity() => WeatherDriver != null && WeatherDriver.ControlsSquallSpawning
            ? Mathf.Clamp01(WeatherDriver.GetSquallActivity(weather)) : 1f;

        private float SquallWeatherStrength(float activity, int rank) => activity
            * Mathf.SmoothStep(0f, 1f, Mathf.Clamp01(activity * weather.Squalls.MaximumActiveSqualls - rank));

        private void MoveSquall(int slot, float deltaTime, float targetWeatherStrength)
        {
            var cell = squalls[slot];
            var weatherStrength = weather.Squalls.FadeSeconds > 0f
                ? Mathf.MoveTowards(cell.WeatherStrength, targetWeatherStrength, deltaTime / weather.Squalls.FadeSeconds)
                : targetWeatherStrength;
            // Clearing weather can dissolve even a nearby storm. Encounter
            // protection only prevents lifetime expiry while weather supports it.
            if (weatherStrength <= 0f && targetWeatherStrength <= 0f)
            {
                squalls[slot] = default;
                activeSquallCount--;
                return;
            }
            var age = cell.AgeSeconds + deltaTime;
            var travel = WorldSquallMotion.Displacement(cell.Seed, cell.AgeSeconds, age, weather.Squalls);
            var offset = WorldSquallMotion.AlongWind(travel, weather.WindDirection) * weather.WindSpeedMetresPerSecond;
            var center = cell.Center + new Vector3(offset.x, 0f, offset.y);
            if (age < squallLifetime || CanEncounterSquall(center, cell.DiameterMetres)) squallRetirementTimes[slot] = 0f;
            else squallRetirementTimes[slot] += Mathf.Min(deltaTime, age - squallLifetime);
            if (age >= squallLifetime && !CanEncounterSquall(center, cell.DiameterMetres)
                && squallRetirementTimes[slot] >= weather.Squalls.FadeSeconds)
            {
                squalls[slot] = default;
                activeSquallCount--;
                return;
            }
            squalls[slot] = MakeSquall(center, cell.DiameterMetres, age, cell.Seed,
                squallRetirementTimes[slot], weatherStrength);
        }

        private bool CanEncounterSquall(Vector3 center, float diameter)
        {
            if (SquallTarget == null) return false;
            var offset = center - SquallTarget.position;
            var range = weather.Squalls.VisibilityDistanceMetres + diameter;
            return offset.x * offset.x + offset.z * offset.z <= range * range;
        }

        private WorldSquallState MakeSquall(Vector3 center, float diameter, float age, int seed,
            float retirementTime, float weatherStrength)
        {
            var settings = weather.Squalls;
            center.y = seaLevel;
            var fade = settings.FadeSeconds;
            var strength = fade <= 0f ? 1f : Mathf.SmoothStep(0f, 1f, Mathf.Clamp01(age / fade))
                * (1f - Mathf.SmoothStep(0f, 1f, Mathf.Clamp01(retirementTime / fade)));
            strength *= weatherStrength;
            var speed = WorldSquallMotion.Speed(seed, age, settings);
            var activity = speed / Mathf.Max(.001f, settings.MaximumSpeedMultiplier);
            var windMultiplier = Mathf.Lerp(1f, settings.WindMultiplier, activity);
            var heading = WorldSquallMotion.AlongWind(WorldSquallMotion.Direction(seed, age, settings), weather.WindDirection);
            var velocity = heading * (weather.WindSpeedMetresPerSecond * speed);
            var field = new WorldSquallField(new Vector2(center.x, center.z), diameter * .5f,
                settings.EdgeFraction, windMultiplier, strength, velocity);
            return new WorldSquallState(center, diameter, age, strength, seed, field, speed, weatherStrength);
        }

        private void RefreshSquallSettings()
        {
            if (!weather.Squalls.Enabled && activeSquallCount > 0) { ClearSquall(); return; }
            squallLifetime = weather.Squalls.LifetimeSeconds;
            squallSpawnCountdown = Mathf.Min(squallSpawnCountdown, weather.Squalls.MaximumSpawnIntervalSeconds);
            var kept = 0;
            for (var slot = 0; slot < squalls.Length; slot++)
            {
                var cell = squalls[slot];
                if (!cell.Active) continue;
                if (kept++ >= weather.Squalls.MaximumActiveSqualls)
                {
                    squalls[slot] = default;
                    activeSquallCount--;
                }
                else squalls[slot] = MakeSquall(cell.Center, cell.DiameterMetres, cell.AgeSeconds,
                    cell.Seed, squallRetirementTimes[slot], cell.WeatherStrength);
            }
            PublishSquallField();
        }

        private void PublishSquallField()
        {
            if (hostState == null) return;
            var peak = 1f;
            var published = 0;
            for (var slot = 0; slot < squalls.Length; slot++)
            {
                if (!squalls[slot].Active) continue;
                WorldSquallShader.Publish(published++, squalls[slot].Field);
                peak = Mathf.Max(peak, Mathf.Lerp(1f, weather.Squalls.WindMultiplier, squalls[slot].Strength));
            }
            // Keep the first legacy field aligned with Squall/GetSquall(0)
            // even after an earlier physical slot has retired.
            while (published < squalls.Length) WorldSquallShader.Publish(published++, default);
            ocean?.ApplySquallWindMultiplier(peak);
        }

        private void UpdateSquallVisuals(float deltaTime)
        {
            var viewer = followTarget != null ? followTarget.position : transform.position;
            if (hostState != null) EnsureSquallAudio();
            var listener = squallThunder?.ListenerPosition(viewer) ?? viewer;
            squallThunder?.Advance(deltaTime, listener, squalls, weather.Squalls.VisibilityDistanceMetres);
            for (var slot = 0; slot < squalls.Length; slot++)
            {
                if (!squalls[slot].Active && squallVisuals[slot] == null) continue;
                squallVisuals[slot] ??= new SquallVisualRenderer(transform);
                var rainWind = weather.WindDirection * WindSpeedAt(squalls[slot].Center);
                squallVisuals[slot].UpdateVisuals(hostState != null ? squalls[slot] : default,
                    weather.Squalls, listener, deltaTime, squallThunder, rainWind);
            }
        }

        private void EnsureSquallAudio()
        {
            if (squallThunder != null) return;
            if (squallAudioProfile == null && !searchedSquallAudio)
            {
                squallAudioProfile = Resources.Load<SquallAudioProfile>("SquallAudio");
                searchedSquallAudio = true;
            }
            if (squallAudioProfile != null && squallAudioProfile.HasThunder)
                squallThunder = new SquallThunderAudio(transform, squallAudioProfile);
        }

        private void SuspendSqualls()
        {
            for (var slot = 0; slot < squallVisuals.Length; slot++)
                squallVisuals[slot]?.UpdateVisuals(default, weather.Squalls, transform.position, 0f);
            ocean?.ApplySquallWindMultiplier(1f);
            squallThunder?.StopAll();
        }

        private void DisposeSqualls()
        {
            squallThunder?.Dispose();
            squallThunder = null;
            for (var slot = 0; slot < squallVisuals.Length; slot++)
            {
                squallVisuals[slot]?.Dispose();
                squallVisuals[slot] = null;
            }
            Array.Clear(squalls, 0, squalls.Length);
            activeSquallCount = 0;
        }

        [ContextMenu("Spawn squall on horizon")]
        private void SpawnSquallFromInspector() => SpawnSquall();
        [ContextMenu("Clear all squalls")]
        private void ClearSquallFromInspector() => ClearSquall();
    }
}
