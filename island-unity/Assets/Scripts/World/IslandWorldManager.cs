using System;
using System.Collections.Generic;
using System.Threading;
using UnityEngine;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;

namespace Motu.World
{
    [DefaultExecutionOrder(-1000)]
    [DisallowMultipleComponent]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "IslandWorldManager")]
    public sealed partial class IslandWorldManager : MonoBehaviour, IWorldSurfaceQuery
    {
        public const float IslandSizeMetres = 2000f;

        private sealed class IslandRuntimeEntry
        {
            internal IslandDescriptor Descriptor { get; }
            internal IslandGenerator Generator { get; set; }
            internal IslandGenerationRequest GenerationRequest { get; }
            internal float RetryAfterTime { get; set; }
            internal bool Queued { get; set; }
            internal bool Generating { get; set; }
            internal int GenerationToken { get; set; }
            internal CancellationTokenSource GenerationCancellation { get; set; }

            internal IslandRuntimeEntry(
                IslandGenerationRequest generationRequest)
            {
                GenerationRequest = generationRequest
                    ?? throw new ArgumentNullException(nameof(generationRequest));
                Descriptor = generationRequest.Descriptor;
            }
        }

        [Header("Island Source")]
        [Tooltip("Required component that decides whether each grid cell contains an island and returns its complete generation request.")]
        [SerializeField] private MonoBehaviour islandGenerationRequestFactoryComponent;

        [Header("World Environment")]
        [Tooltip("Optional shared world environment asset. Inline values remain the scene fallback.")]
        [SerializeField] private WorldEnvironmentConfiguration worldEnvironmentConfiguration;
        [SerializeField] private WorldEnvironmentSettings worldEnvironmentSettings =
            new WorldEnvironmentSettings();
        [SerializeField] private IslandCloudSettings worldClouds = new IslandCloudSettings();
        [Tooltip("Optional scene component that updates all runtime wind, wave and cloud properties. Derive your script from WorldWeatherDriver.")]
        [SerializeField] private WorldWeatherDriver weatherDriver;

        private WorldWeatherState? pendingWeather;

        [Header("World Grid Discovery")]
        [Min(1000f)] [SerializeField] private float discoveryRadiusMetres = 16000f;
        [Min(1000f)] [SerializeField] private float generationRadiusMetres = 10500f;
        [Min(0.05f)] [SerializeField] private float discoveryRefreshSeconds = 0.5f;

        [Header("Generation Priority")]
        [Min(0f)] [SerializeField] private float velocityLookAheadSeconds = 24f;
        [Min(0f)] [SerializeField] private float maximumLookAheadMetres = 8000f;
        [Range(0.1f, 1f)] [SerializeField] private float forwardPriorityMultiplier = 0.65f;
        [Min(0f)] [SerializeField] private float cancellationHysteresisMetres = 1800f;
        [Range(0.5f, 16f)] [SerializeField] private float installationBudgetMilliseconds = 4f;

        [Header("Player Routing")]
        [SerializeField] private Transform streamingTarget;

        [Header("Island Residency")]
        [Min(100f)] [SerializeField] private float activeRadiusMetres = 6000f;
        [Min(0f)] [SerializeField] private float activeHysteresisMetres = 500f;
        [Min(100f)] [SerializeField] private float unloadRadiusMetres = 12500f;
        [Range(1, 8)] [SerializeField] private int maximumLoadedIslandCount = 3;
        [Min(1f)] [SerializeField] private float failedGenerationRetrySeconds = 15f;

        private readonly Dictionary<Vector2Int, IslandRuntimeEntry> managedIslands =
            new Dictionary<Vector2Int, IslandRuntimeEntry>();
        private readonly HashSet<Vector2Int> discoveryScanCells =
            new HashSet<Vector2Int>();
        private readonly List<Vector2Int> removalCells = new List<Vector2Int>();
        private CancellationTokenSource shutdown;
        private WorldEnvironmentController worldEnvironment;
        private IslandRuntimeEntry currentGeneration;
        private IslandGenerator focusedIsland;
        private bool queueRunning;
        private bool destroyed;
        private Vector3 lastQueryPosition;
        private Vector3 projectedGenerationPosition;
        internal Vector3 smoothedVelocity;
        private Vector3 previousTargetPosition;
        private bool hasQueryPosition;
        internal bool hasPreviousTargetPosition;
        internal float nextDiscoveryTime;
        private IIslandGenerationRequestFactory islandGenerationRequestFactory;

        public IslandGenerator FocusedIsland => focusedIsland;
        public IIslandGenerationRequestFactory IslandGenerationRequestFactory
        {
            get => islandGenerationRequestFactory;
            set => islandGenerationRequestFactory = value;
        }
        public int ResidentIslandLimit => Mathf.Max(maximumLoadedIslandCount, 1);
        public int NativeHandleCount => NativeIslandHandle.ActiveCount;
        public WorldEnvironmentSettings GlobalEnvironmentSettings => EnvironmentSettings;
        public WorldEnvironmentController EnvironmentController => worldEnvironment;
        public Vector2 LogicalPlayerPosition => new Vector2(
            lastQueryPosition.x,
            lastQueryPosition.z);
        public int KnownIslandCount => managedIslands.Count;
        public int LoadedIslandCount
        {
            get
            {
                var count = 0;
                foreach (var entry in managedIslands.Values)
                {
                    if (entry.Generator != null && entry.Generator.HasRuntime)
                    {
                        count++;
                    }
                }
                return count;
            }
        }
        public int QueuedIslandCount
        {
            get
            {
                var count = 0;
                foreach (var entry in managedIslands.Values)
                {
                    if (entry.Queued)
                    {
                        count++;
                    }
                }
                return count;
            }
        }
        public int GeneratingIslandCount => currentGeneration != null ? 1 : 0;

        public bool HasIsland(Vector2Int islandGridPosition)
        {
            return IslandGenerationRequestFactory != null
                && IslandGenerationRequestFactory.HasIsland(islandGridPosition);
        }

        private WorldEnvironmentSettings EnvironmentSettings =>
            worldEnvironmentConfiguration != null
                ? worldEnvironmentConfiguration.Environment
                : worldEnvironmentSettings ??= new WorldEnvironmentSettings();
        private IslandCloudSettings CloudSettings =>
            worldEnvironmentConfiguration != null
                ? worldEnvironmentConfiguration.Clouds
                : worldClouds ??= new IslandCloudSettings();

        public void ConfigureIslandGenerationRequestFactory(MonoBehaviour factoryComponent)
        {
            islandGenerationRequestFactoryComponent = factoryComponent;
            islandGenerationRequestFactory = factoryComponent as IIslandGenerationRequestFactory
                ?? throw new ArgumentException(
                    $"{factoryComponent?.GetType().Name ?? "null"} must implement "
                    + $"{nameof(IIslandGenerationRequestFactory)}.",
                    nameof(factoryComponent));
        }

        public void ConfigureWorldEnvironment(Light sunlight, Material seaMaterial)
        {
            worldEnvironmentSettings.AssignSceneReferences(sunlight, seaMaterial);
        }

        public WorldWeatherDriver WeatherDriver
        {
            get => weatherDriver;
            set
            {
                weatherDriver = value;
                if (worldEnvironment != null) worldEnvironment.WeatherDriver = value;
            }
        }

        public WorldWeatherState Weather => worldEnvironment != null && worldEnvironment.IsInstalled
            ? worldEnvironment.Weather
            : pendingWeather ?? WorldWeatherState.FromEnvironment(EnvironmentSettings);

        public void ApplyWeather(WorldWeatherState weather)
        {
            if (worldEnvironment != null && worldEnvironment.IsInstalled)
            {
                worldEnvironment.ApplyWeather(weather);
            }
            else
            {
                pendingWeather = weather.Validated();
            }
        }

        public void SetWind(Vector2 direction, float speedMetresPerSecond)
        {
            var updated = Weather;
            updated.WindDirection = direction;
            updated.WindSpeedMetresPerSecond = speedMetresPerSecond;
            ApplyWeather(updated);
        }

        private void Awake()
        {
            shutdown = new CancellationTokenSource();
            ResolveIslandGenerationRequestFactory();
            worldEnvironment = WorldEnvironmentController.FindOrCreate();
            worldEnvironment.Initialize(
                EnvironmentSettings,
                CloudSettings,
                IslandSizeMetres,
                IslandSizeMetres * 2.1f,
                ResolveStreamingTarget());
            worldEnvironment.WeatherDriver = weatherDriver;
            if (pendingWeather.HasValue)
            {
                worldEnvironment.ApplyWeather(pendingWeather.Value);
                pendingWeather = null;
            }
        }

        private void ResolveIslandGenerationRequestFactory()
        {
            if (islandGenerationRequestFactory != null)
            {
                return;
            }
            if (islandGenerationRequestFactoryComponent == null)
            {
                throw new InvalidOperationException(
                    $"IslandWorldManager requires an {nameof(IIslandGenerationRequestFactory)} component.");
            }
            islandGenerationRequestFactory =
                islandGenerationRequestFactoryComponent as IIslandGenerationRequestFactory;
            if (islandGenerationRequestFactory == null)
            {
                throw new InvalidOperationException(
                    $"{islandGenerationRequestFactoryComponent.GetType().Name} must implement "
                    + $"{nameof(IIslandGenerationRequestFactory)}.");
            }
        }

        private void Update()
        {
            var target = ResolveStreamingTarget();
            if (target == null)
            {
                return;
            }

            UpdateTravelPrediction(target.position);
            lastQueryPosition = target.position;
            hasQueryPosition = true;
            if (Time.unscaledTime >= nextDiscoveryTime)
            {
                nextDiscoveryTime = Time.unscaledTime + discoveryRefreshSeconds;
                RefreshDiscovery(lastQueryPosition, projectedGenerationPosition);
            }
            QueueDesiredGeneration();
            CancelObsoleteGeneration();
            UpdateResidencyAndFocus(lastQueryPosition);
            worldEnvironment?.SetFollowTarget(target);
            if (!queueRunning && HasReadyQueuedIsland())
            {
                ProcessGenerationQueue();
            }
        }

        private void LateUpdate()
        {
            foreach (var entry in managedIslands.Values)
            {
                var generator = entry.Generator;
                if (generator != null
                    && generator.HasRuntime)
                {
                    generator.SyncSharedWorldLighting(worldEnvironment);
                }
            }
        }

        private void OnDestroy()
        {
            destroyed = true;
            shutdown?.Cancel();
            foreach (var entry in managedIslands.Values)
            {
                entry.GenerationToken++;
                entry.GenerationCancellation?.Cancel();
            }
            shutdown?.Dispose();
            shutdown = null;
            managedIslands.Clear();
            discoveryScanCells.Clear();
            removalCells.Clear();
        }

        public void SetStreamingTarget(Transform target)
        {
            streamingTarget = target;
            // Entry and teleportation are not travel velocity. Discover around the
            // new target immediately instead of projecting beyond a large jump.
            hasPreviousTargetPosition = false;
            smoothedVelocity = Vector3.zero;
            nextDiscoveryTime = 0f;
            worldEnvironment?.SetFollowTarget(target);
            if (target != null)
            {
                PrepareStreamingAt(target.position);
            }
            else
            {
                SetFocusedIsland(null, default);
            }
        }

        public void PrepareStreamingAt(Vector3 worldPosition)
        {
            lastQueryPosition = worldPosition;
            projectedGenerationPosition = worldPosition;
            hasQueryPosition = true;
            UpdateResidencyAndFocus(worldPosition);
        }

        public bool TrySnapToTerrain(
            Vector3 approximateWorldPoint,
            out Vector3 worldPoint)
        {
            PrepareStreamingAt(approximateWorldPoint);
            if (focusedIsland != null
                && focusedIsland.TrySnapToTerrain(approximateWorldPoint, out worldPoint))
            {
                return true;
            }
            worldPoint = approximateWorldPoint;
            return false;
        }

        public float GetTerrainOrSeaHeight(Vector3 approximateWorldPoint)
        {
            if (TrySnapToTerrain(approximateWorldPoint, out var terrainPoint))
            {
                return Mathf.Max(SeaLevelWorldY(), terrainPoint.y);
            }
            return SeaLevelWorldY();
        }

        public void SetFirstPersonViewActive(bool active)
        {
            worldEnvironment?.SetFirstPersonViewActive(active);
        }

        private void UpdateResidencyAndFocus(Vector3 worldPosition)
        {
            foreach (var entry in managedIslands.Values)
            {
                var generator = entry.Generator;
                if (generator == null || generator.IsGenerating || !generator.HasRuntime)
                {
                    continue;
                }
                var distance = DistanceToDescriptor(entry.Descriptor, worldPosition);
                if (DistanceFromIslandEdge(distance, entry.Descriptor)
                    > EffectiveUnloadRadius())
                {
                    if (focusedIsland == generator)
                    {
                        focusedIsland = null;
                    }
                    generator.Clear();
                    continue;
                }

                var dormant = generator.Runtime.State == IslandRuntimeState.Dormant;
                var shouldWake = dormant && distance <= activeRadiusMetres;
                var shouldSleep = !dormant
                    && generator != focusedIsland
                    && distance > activeRadiusMetres + activeHysteresisMetres;
                if (shouldWake || shouldSleep)
                {
                    generator.SetRuntimeDormant(shouldSleep);
                }
            }

            var selected = SelectFocusedIsland(worldPosition);
            SetFocusedIsland(selected, worldPosition);
        }

        private IslandGenerator SelectFocusedIsland(Vector3 worldPosition)
        {
            return managedIslands.TryGetValue(WorldToCell(worldPosition), out var entry)
                && entry.Generator != null
                && entry.Generator.HasRuntime
                    ? entry.Generator
                    : null;
        }

        private void SetFocusedIsland(IslandGenerator selected, Vector3 worldPosition)
        {
            if (selected != null && selected.Runtime.State == IslandRuntimeState.Dormant)
            {
                selected.SetRuntimeDormant(false);
            }
            if (focusedIsland != selected)
            {
                focusedIsland?.SetStreamingTarget(null);
                focusedIsland?.ClearStreamingFocus();
                focusedIsland = selected;
            }
            if (focusedIsland != null)
            {
                focusedIsland.SetStreamingTarget(streamingTarget);
                focusedIsland.PrepareStreamingAt(worldPosition);
            }
            worldEnvironment?.SetFollowTarget(streamingTarget);
        }

        private void DestroyGenerator(IslandRuntimeEntry entry)
        {
            var generator = entry.Generator;
            if (generator == null)
            {
                return;
            }
            generator.Clear();
            entry.Generator = null;
            if (Application.isPlaying)
            {
                Destroy(generator.gameObject);
            }
            else
            {
                DestroyImmediate(generator.gameObject);
            }
        }

        private bool IsPreferredResident(IslandRuntimeEntry candidate)
        {
            var availableSlots = ResidentIslandLimit;
            if (availableSlots <= 0)
            {
                return false;
            }

            var betterCandidates = 0;
            foreach (var other in managedIslands.Values)
            {
                if (other == candidate || !IsInsideGenerationCorridor(other, 0f))
                {
                    continue;
                }
                if (CompareGenerationPriority(other, candidate) < 0
                    && ++betterCandidates >= availableSlots)
                {
                    return false;
                }
            }
            return true;
        }

        private int CompareGenerationPriority(
            IslandRuntimeEntry first,
            IslandRuntimeEntry second)
        {
            var comparison = GenerationPriority(first).CompareTo(
                GenerationPriority(second));
            return comparison != 0
                ? comparison
                : string.CompareOrdinal(
                    first.Descriptor.IslandId,
                    second.Descriptor.IslandId);
        }

        private bool ReserveResidentCapacity(
            IslandRuntimeEntry incoming,
            out bool releasedRuntime)
        {
            releasedRuntime = false;
            if (incoming.Generator != null && incoming.Generator.HasRuntime)
            {
                return true;
            }
            if (LoadedIslandCount < ResidentIslandLimit)
            {
                return true;
            }

            IslandRuntimeEntry eviction = null;
            foreach (var entry in managedIslands.Values)
            {
                if (entry == incoming
                    || entry.Generator == null
                    || !entry.Generator.HasRuntime
                    || entry.Generator == focusedIsland)
                {
                    continue;
                }
                if (eviction == null || CompareGenerationPriority(entry, eviction) > 0)
                {
                    eviction = entry;
                }
            }
            if (eviction == null)
            {
                return false;
            }

            eviction.Generator.Clear();
            releasedRuntime = true;
            return LoadedIslandCount < ResidentIslandLimit;
        }

        private Transform ResolveStreamingTarget()
        {
            if (streamingTarget != null)
            {
                return streamingTarget;
            }
            return Camera.main != null ? Camera.main.transform : null;
        }

        private float SeaLevelWorldY()
        {
            return EnvironmentSettings.SeaLevelMetres;
        }

        private float EffectiveUnloadRadius()
        {
            return Mathf.Max(
                Mathf.Max(unloadRadiusMetres, generationRadiusMetres),
                activeRadiusMetres + activeHysteresisMetres + 1f);
        }

        public static Vector2Int WorldToCell(Vector3 worldPosition)
        {
            var halfCell = IslandSizeMetres * 0.5;
            return new Vector2Int(
                (int)Math.Floor((worldPosition.x + halfCell) / IslandSizeMetres),
                (int)Math.Floor((worldPosition.z + halfCell) / IslandSizeMetres));
        }

        public static Vector3 CellCentre(Vector2Int worldCell, float worldY = 0f)
        {
            return new Vector3(
                worldCell.x * IslandSizeMetres,
                worldY,
                worldCell.y * IslandSizeMetres);
        }

        private static float DistanceToDescriptor(
            IslandDescriptor descriptor,
            Vector3 position)
        {
            var x = descriptor.LogicalXMetres - position.x;
            var z = descriptor.LogicalZMetres - position.z;
            return (float)Math.Sqrt(x * x + z * z);
        }

        internal static float DistanceFromIslandEdge(
            float centreDistance,
            IslandDescriptor descriptor)
        {
            return Mathf.Max(
                0f,
                centreDistance - descriptor.EstimatedBoundingRadiusMetres);
        }

        internal static float HorizontalDistance(Vector3 first, Vector3 second)
        {
            var x = first.x - second.x;
            var z = first.z - second.z;
            return Mathf.Sqrt(x * x + z * z);
        }


    }
}
