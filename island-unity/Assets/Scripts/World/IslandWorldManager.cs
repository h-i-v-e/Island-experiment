using System;
using System.Collections.Generic;
using System.Threading;
using UnityEngine;
using UnityEngine.Serialization;

[DefaultExecutionOrder(-1000)]
[DisallowMultipleComponent]
public sealed partial class IslandWorldManager : MonoBehaviour, IWorldSurfaceQuery
{
    public const float IslandCellSizeMetres = 2000f;

    [Serializable]
    public sealed class AuthoredIsland
    {
        [SerializeField] private IslandGenerator generator;
        [Tooltip("Stable save/discovery identity. The object name and seed are used when empty.")]
        [SerializeField] private string stableId;
        [SerializeField] private bool generateOnStart = true;

        public IslandGenerator Generator => generator;
        public bool GenerateOnStart => generateOnStart;

        internal IslandDescriptor CreateDescriptor(Vector2Int worldCell)
        {
            if (generator == null)
            {
                throw new InvalidOperationException(
                    "An authored island entry requires an IslandGenerator.");
            }
            var id = string.IsNullOrWhiteSpace(stableId)
                ? $"authored-{generator.gameObject.name}-{generator.Generation.Seed}"
                : stableId.Trim();
            return IslandDescriptor.Authored(
                id,
                worldCell,
                generator,
                IslandCellSizeMetres);
        }

        internal static AuthoredIsland FromGenerator(IslandGenerator value)
        {
            return new AuthoredIsland { generator = value };
        }
    }

    private sealed class ManagedIsland
    {
        internal IslandDescriptor Descriptor { get; }
        internal AuthoredIsland Authored { get; }
        internal bool IsAuthored => Authored != null;
        internal bool InitialGenerationPending { get; set; }
        internal IslandGenerator Generator { get; set; }
        internal float RetryAfterTime { get; set; }
        internal bool Queued { get; set; }
        internal bool Generating { get; set; }
        internal int GenerationToken { get; set; }
        internal CancellationTokenSource GenerationCancellation { get; set; }

        internal ManagedIsland(
            IslandDescriptor descriptor,
            IslandGenerator generator,
            AuthoredIsland authored)
        {
            Descriptor = descriptor;
            Generator = generator;
            Authored = authored;
            InitialGenerationPending = authored != null && authored.GenerateOnStart;
        }
    }

    [Header("Authored Islands")]
    [SerializeField] private List<AuthoredIsland> islands = new List<AuthoredIsland>();
    [Tooltip("Generator used only as the base parameter template for discovered islands. Defaults to the first authored island.")]
    [FormerlySerializedAs("environmentAuthority")]
    [SerializeField] private IslandGenerator islandTemplate;

    [Header("World Environment")]
    [Tooltip("Optional shared world environment asset. Inline values remain the scene fallback.")]
    [SerializeField] private WorldEnvironmentConfiguration worldEnvironmentConfiguration;
    [SerializeField] private WorldEnvironmentSettings worldEnvironmentSettings =
        new WorldEnvironmentSettings();
    [SerializeField] private IslandCloudSettings worldClouds = new IslandCloudSettings();

    [Header("Per-Island Variation")]
    [Tooltip("Deterministic variation applied independently to every island's generation parameters.")]
    [SerializeField] private IslandParameterVariationSettings islandParameterVariation =
        new IslandParameterVariationSettings();

    [Header("Procedural Ocean Discovery")]
    [SerializeField] private bool enableProceduralDiscovery = true;
    [SerializeField] private int worldSeed = 8675309;
    [Range(0f, 1f)] [SerializeField] private float islandCellOccupancy = 0.32f;
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
    [SerializeField] private bool unloadAuthoredBeyondRadius;
    [Min(1f)] [SerializeField] private float failedGenerationRetrySeconds = 15f;

    private readonly Dictionary<Vector2Int, ManagedIsland> managedIslands =
        new Dictionary<Vector2Int, ManagedIsland>();
    private readonly HashSet<Vector2Int> discoveryScanCells =
        new HashSet<Vector2Int>();
    private readonly List<Vector2Int> removalCells = new List<Vector2Int>();
    private CancellationTokenSource shutdown;
    private WorldEnvironmentController worldEnvironment;
    private ManagedIsland currentGeneration;
    private IslandGenerator focusedIsland;
    private string generatorTemplateJson;
    private bool queueRunning;
    private bool destroyed;
    private Vector3 lastQueryPosition;
    private Vector3 projectedGenerationPosition;
    private Vector3 smoothedVelocity;
    private Vector3 previousTargetPosition;
    private bool hasQueryPosition;
    private bool hasPreviousTargetPosition;
    private float nextDiscoveryTime;

    public IslandGenerator FocusedIsland => focusedIsland;
    public int WorldSeed => worldSeed;
    public int ResidentIslandLimit => Mathf.Max(maximumLoadedIslandCount, 1);
    public int NativeHandleCount => NativeIslandHandle.ActiveCount;
    public Vector2 LogicalPlayerPosition => new Vector2(
        lastQueryPosition.x,
        lastQueryPosition.z);
    public int AuthoredIslandCount => islands.Count;
    public int DiscoveredIslandCount
    {
        get
        {
            var count = 0;
            foreach (var entry in managedIslands.Values)
            {
                if (!entry.IsAuthored)
                {
                    count++;
                }
            }
            return count;
        }
    }
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
    private WorldEnvironmentSettings EnvironmentSettings =>
        worldEnvironmentConfiguration != null
            ? worldEnvironmentConfiguration.Environment
            : worldEnvironmentSettings ??= new WorldEnvironmentSettings();
    private IslandCloudSettings CloudSettings =>
        worldEnvironmentConfiguration != null
            ? worldEnvironmentConfiguration.Clouds
            : worldClouds ??= new IslandCloudSettings();

    public void ConfigureProceduralDiscovery(bool enabled, int seed)
    {
        enableProceduralDiscovery = enabled;
        worldSeed = seed;
        unloadAuthoredBeyondRadius = enabled;
    }

    public void ConfigureIslandTemplate(IslandGenerator template)
    {
        islandTemplate = template;
    }

    public void ConfigureWorldEnvironment(Light sunlight, Material seaMaterial)
    {
        worldEnvironmentSettings.AssignSceneReferences(sunlight, seaMaterial);
    }

    private void Awake()
    {
        shutdown = new CancellationTokenSource();
        DiscoverChildGeneratorsWhenUnconfigured();
        ResolveIslandTemplate();
        if (islandTemplate != null)
        {
            // Capture the unvaried profile. Every authored and discovered island
            // derives independently from this same baseline.
            generatorTemplateJson = JsonUtility.ToJson(islandTemplate);
        }
        ConfigureAuthoredGenerators();
        worldEnvironment = WorldEnvironmentController.FindOrCreate();
        worldEnvironment.Initialize(
            EnvironmentSettings,
            CloudSettings,
            worldSeed,
            IslandCellSizeMetres,
            IslandCellSizeMetres * 2.1f,
            ResolveStreamingTarget());
    }

    private void Start()
    {
        foreach (var entry in managedIslands.Values)
        {
            if (entry.InitialGenerationPending)
            {
                QueueGeneration(entry);
            }
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
        if (enableProceduralDiscovery
            && Time.unscaledTime >= nextDiscoveryTime)
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

    private void DiscoverChildGeneratorsWhenUnconfigured()
    {
        if (islands.Count != 0)
        {
            return;
        }
        foreach (var generator in GetComponentsInChildren<IslandGenerator>(true))
        {
            islands.Add(AuthoredIsland.FromGenerator(generator));
        }
    }

    private void ResolveIslandTemplate()
    {
        if (islandTemplate != null)
        {
            return;
        }
        foreach (var entry in islands)
        {
            if (entry?.Generator != null)
            {
                islandTemplate = entry.Generator;
                return;
            }
        }
    }

    private void ConfigureAuthoredGenerators()
    {
        if (islandTemplate == null)
        {
            Debug.LogWarning("IslandWorldManager has no authored IslandGenerator entries.", this);
            return;
        }

        var ids = new HashSet<string>(StringComparer.Ordinal);
        foreach (var authored in islands)
        {
            var generator = authored?.Generator;
            if (generator == null)
            {
                continue;
            }
            var worldCell = WorldToCell(generator.transform.position);
            var descriptor = authored.CreateDescriptor(worldCell);
            if (!ids.Add(descriptor.IslandId))
            {
                throw new InvalidOperationException(
                    $"Duplicate authored island ID '{descriptor.IslandId}'.");
            }
            if (managedIslands.ContainsKey(worldCell))
            {
                throw new InvalidOperationException(
                    $"Multiple authored islands occupy world cell {worldCell}.");
            }
            generator.ApplyIslandProfile(descriptor.Seed, islandParameterVariation);
            generator.ConfigureWorldManagement();
            generator.SetStreamingTarget(null);
            var managed = new ManagedIsland(descriptor, generator, authored);
            managedIslands.Add(worldCell, managed);
        }
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
            var canUnload = !entry.IsAuthored || unloadAuthoredBeyondRadius;
            if (canUnload && distance > EffectiveUnloadRadius())
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

    private void DestroyDiscoveredGenerator(ManagedIsland entry)
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

    private bool IsPreferredResident(ManagedIsland candidate)
    {
        if (candidate.InitialGenerationPending)
        {
            return true;
        }
        var availableSlots = ResidentIslandLimit;
        if (availableSlots <= 0)
        {
            return false;
        }

        var betterCandidates = 0;
        foreach (var other in managedIslands.Values)
        {
            if (other == candidate
                || (!other.InitialGenerationPending
                    && !IsInsideGenerationCorridor(other, 0f)))
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

    private int CompareGenerationPriority(ManagedIsland first, ManagedIsland second)
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
        ManagedIsland incoming,
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

        ManagedIsland eviction = null;
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
            unloadRadiusMetres,
            activeRadiusMetres + activeHysteresisMetres + 1f);
    }

    public static Vector2Int WorldToCell(Vector3 worldPosition)
    {
        var halfCell = IslandCellSizeMetres * 0.5;
        return new Vector2Int(
            (int)Math.Floor((worldPosition.x + halfCell) / IslandCellSizeMetres),
            (int)Math.Floor((worldPosition.z + halfCell) / IslandCellSizeMetres));
    }

    public static Vector3 CellCentre(Vector2Int worldCell, float worldY = 0f)
    {
        return new Vector3(
            worldCell.x * IslandCellSizeMetres,
            worldY,
            worldCell.y * IslandCellSizeMetres);
    }

    private static float DistanceToDescriptor(
        IslandDescriptor descriptor,
        Vector3 position)
    {
        var x = descriptor.LogicalXMetres - position.x;
        var z = descriptor.LogicalZMetres - position.z;
        return (float)Math.Sqrt(x * x + z * z);
    }

    private static float HorizontalDistance(Vector3 first, Vector3 second)
    {
        var x = first.x - second.x;
        var z = first.z - second.z;
        return Mathf.Sqrt(x * x + z * z);
    }

#if UNITY_EDITOR
    public static void ValidateRoutingPolicy()
    {
        if (NativeIslandHandle.ActiveCount != 0)
        {
            throw new InvalidOperationException(
                "Native island handles remained allocated after generation validation.");
        }
        const float active = 6000f;
        const float hysteresis = 500f;
        const float generation = 10500f;
        const float unload = 12500f;
        const float discovery = 16000f;
        if (!(active < active + hysteresis
            && active + hysteresis < generation
            && generation < unload
            && unload < discovery))
        {
            throw new InvalidOperationException(
                "Island residency and discovery thresholds are not correctly ordered.");
        }
        if (HorizontalDistance(new Vector3(3f, 80f, 4f), Vector3.zero) != 5f)
        {
            throw new InvalidOperationException(
                "Island routing distance must ignore elevation.");
        }

        var expectedCell = new Vector2Int(-7, 11);
        var expectedCentre = CellCentre(expectedCell);
        if (WorldToCell(expectedCentre) != expectedCell
            || WorldToCell(new Vector3(999.99f, 0f, -999.99f)) != Vector2Int.zero
            || WorldToCell(new Vector3(1000f, 0f, -1000.01f))
                != new Vector2Int(1, -1))
        {
            throw new InvalidOperationException(
                "World positions do not map deterministically to 2 km island cells.");
        }

        var firstOccupied = IslandDescriptor.TryCreateProcedural(
            8128,
            expectedCell,
            IslandCellSizeMetres,
            1f,
            out var first);
        var repeatedOccupied = IslandDescriptor.TryCreateProcedural(
            8128,
            expectedCell,
            IslandCellSizeMetres,
            1f,
            out var repeated);
        if (!firstOccupied
            || !repeatedOccupied
            || first.IslandId != repeated.IslandId
            || first.Seed != repeated.Seed
            || first.LogicalXMetres != expectedCentre.x
            || first.LogicalZMetres != expectedCentre.z
            || first.LogicalXMetres != repeated.LogicalXMetres
            || first.LogicalZMetres != repeated.LogicalZMetres)
        {
            throw new InvalidOperationException(
                "Procedural island descriptors are not deterministic.");
        }

        var sourceObject = new GameObject("Island generator template validation");
        var cloneObject = new GameObject("Island generator clone validation");
        var repeatObject = new GameObject("Island generator repeat validation");
        sourceObject.SetActive(false);
        cloneObject.SetActive(false);
        repeatObject.SetActive(false);
        try
        {
            var source = sourceObject.AddComponent<IslandGenerator>();
            source.Generation.Seed = 13579;
            var clone = cloneObject.AddComponent<IslandGenerator>();
            JsonUtility.FromJsonOverwrite(JsonUtility.ToJson(source), clone);
            if (clone.Generation.Seed != source.Generation.Seed
                || clone.WorldSizeMetres != source.WorldSizeMetres)
            {
                throw new InvalidOperationException(
                    "Discovered island generators cannot reproduce the island template profile.");
            }

            var variation = new IslandParameterVariationSettings();
            clone.ApplyIslandProfile(24680, variation);
            var repeatedGenerator = repeatObject.AddComponent<IslandGenerator>();
            JsonUtility.FromJsonOverwrite(JsonUtility.ToJson(source), repeatedGenerator);
            repeatedGenerator.ApplyIslandProfile(24680, variation);
            source.ApplyIslandProfile(13579, variation);
            if (!Mathf.Approximately(
                    clone.Generation.MaximumHeightMetres,
                    repeatedGenerator.Generation.MaximumHeightMetres)
                || !Mathf.Approximately(
                    clone.Generation.WaterRatio,
                    repeatedGenerator.Generation.WaterRatio))
            {
                throw new InvalidOperationException(
                    "Per-island parameter variation is not deterministic for a stable island seed.");
            }
            if (Mathf.Approximately(
                    source.Generation.MaximumHeightMetres,
                    clone.Generation.MaximumHeightMetres)
                && Mathf.Approximately(
                    source.Generation.WaterRatio,
                    clone.Generation.WaterRatio))
            {
                throw new InvalidOperationException(
                    "Distinct island seeds did not produce distinct height or water-ratio profiles.");
            }
        }
        finally
        {
            DestroyImmediate(sourceObject);
            DestroyImmediate(cloneObject);
            DestroyImmediate(repeatObject);
        }
    }
#endif
}
