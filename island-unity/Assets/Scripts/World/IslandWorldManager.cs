using System;
using System.Collections.Generic;
using System.Threading;
using UnityEngine;

[DefaultExecutionOrder(-1000)]
[DisallowMultipleComponent]
public sealed class IslandWorldManager : MonoBehaviour, IWorldSurfaceQuery
{
    [Serializable]
    public sealed class AuthoredIsland
    {
        [SerializeField] private IslandGenerator generator;
        [Tooltip("Stable save/discovery identity. The object name and seed are used when empty.")]
        [SerializeField] private string stableId;
        [SerializeField] private Vector2Int worldCell;
        [SerializeField] private bool generateOnStart = true;

        public IslandGenerator Generator => generator;
        public bool GenerateOnStart => generateOnStart;

        internal IslandDescriptor CreateDescriptor()
        {
            if (generator == null)
            {
                throw new InvalidOperationException(
                    "An authored island entry requires an IslandGenerator.");
            }
            var id = string.IsNullOrWhiteSpace(stableId)
                ? $"authored-{generator.gameObject.name}-{generator.Generation.Seed}"
                : stableId.Trim();
            return IslandDescriptor.Authored(id, worldCell, generator);
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
    [Tooltip("Generator whose settings own the global sky, ocean, weather, and solar clock. Defaults to the first entry.")]
    [SerializeField] private IslandGenerator environmentAuthority;

    [Header("Procedural Ocean Discovery")]
    [SerializeField] private bool enableProceduralDiscovery = true;
    [SerializeField] private int worldSeed = 8675309;
    [Min(2500f)] [SerializeField] private float oceanCellSizeMetres = 5200f;
    [Range(0f, 1f)] [SerializeField] private float islandCellOccupancy = 0.32f;
    [Range(0f, 0.45f)] [SerializeField] private float islandCellJitterFraction = 0.18f;
    [Min(0f)] [SerializeField] private float minimumIslandGapMetres = 700f;
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
    [Min(50f)] [SerializeField] private float detailFocusRadiusMetres = 1400f;
    [Min(0f)] [SerializeField] private float focusHysteresisMetres = 250f;

    [Header("Island Residency")]
    [Min(100f)] [SerializeField] private float activeRadiusMetres = 6000f;
    [Min(0f)] [SerializeField] private float activeHysteresisMetres = 500f;
    [Min(100f)] [SerializeField] private float unloadRadiusMetres = 12500f;
    [Range(1, 8)] [SerializeField] private int maximumLoadedIslandCount = 3;
    [SerializeField] private bool unloadAuthoredBeyondRadius;
    [Min(1f)] [SerializeField] private float failedGenerationRetrySeconds = 15f;

    private readonly List<ManagedIsland> managedIslands = new List<ManagedIsland>();
    private readonly Dictionary<Vector2Int, ManagedIsland> discoveredIslands =
        new Dictionary<Vector2Int, ManagedIsland>();
    private readonly HashSet<Vector2Int> discoveryScanCells =
        new HashSet<Vector2Int>();
    private readonly List<Vector2Int> removalCells = new List<Vector2Int>();
    private CancellationTokenSource shutdown;
    private ManagedIsland authorityIsland;
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
    public int DiscoveredIslandCount => discoveredIslands.Count;
    public int KnownIslandCount => managedIslands.Count;
    public int LoadedIslandCount
    {
        get
        {
            var count = 0;
            foreach (var entry in managedIslands)
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
            foreach (var entry in managedIslands)
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

    public void ConfigureProceduralDiscovery(bool enabled, int seed)
    {
        enableProceduralDiscovery = enabled;
        worldSeed = seed;
        unloadAuthoredBeyondRadius = enabled;
    }

    private void Awake()
    {
        shutdown = new CancellationTokenSource();
        DiscoverChildGeneratorsWhenUnconfigured();
        ConfigureAuthoredGenerators();
        if (environmentAuthority != null)
        {
            generatorTemplateJson = JsonUtility.ToJson(environmentAuthority);
        }
    }

    private void Start()
    {
        QueueGeneration(authorityIsland);
        foreach (var entry in managedIslands)
        {
            if (entry != authorityIsland && entry.InitialGenerationPending)
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
            && environmentAuthority != null
            && Time.unscaledTime >= nextDiscoveryTime)
        {
            nextDiscoveryTime = Time.unscaledTime + discoveryRefreshSeconds;
            RefreshDiscovery(lastQueryPosition, projectedGenerationPosition);
        }
        QueueDesiredGeneration();
        CancelObsoleteGeneration();
        UpdateResidencyAndFocus(lastQueryPosition);
        environmentAuthority?.SetWorldEnvironmentFollowTarget(target);
        if (!queueRunning && HasReadyQueuedIsland())
        {
            ProcessGenerationQueue();
        }
    }

    private void LateUpdate()
    {
        if (environmentAuthority == null)
        {
            return;
        }
        foreach (var entry in managedIslands)
        {
            var generator = entry.Generator;
            if (generator != null
                && generator != environmentAuthority
                && generator.HasRuntime)
            {
                generator.SyncSharedWorldLighting(environmentAuthority);
            }
        }
    }

    private void OnDestroy()
    {
        destroyed = true;
        shutdown?.Cancel();
        foreach (var entry in managedIslands)
        {
            entry.GenerationToken++;
            entry.GenerationCancellation?.Cancel();
        }
        shutdown?.Dispose();
        shutdown = null;
        discoveredIslands.Clear();
        managedIslands.Clear();
        discoveryScanCells.Clear();
        removalCells.Clear();
    }

    public void SetStreamingTarget(Transform target)
    {
        streamingTarget = target;
        environmentAuthority?.SetWorldEnvironmentFollowTarget(target);
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
        foreach (var entry in managedIslands)
        {
            var generator = entry.Generator;
            if (generator == null
                || generator == focusedIsland
                || !generator.HasActiveRuntime
                || !ContainsXZ(generator, approximateWorldPoint))
            {
                continue;
            }
            generator.PrepareStreamingAt(approximateWorldPoint);
            if (generator.TrySnapToTerrain(approximateWorldPoint, out worldPoint))
            {
                return true;
            }
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
        environmentAuthority?.SetFirstPersonViewActive(active);
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

    private void ConfigureAuthoredGenerators()
    {
        if (environmentAuthority == null)
        {
            foreach (var entry in islands)
            {
                if (entry?.Generator != null)
                {
                    environmentAuthority = entry.Generator;
                    break;
                }
            }
        }
        if (environmentAuthority == null)
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
            var descriptor = authored.CreateDescriptor();
            if (!ids.Add(descriptor.IslandId))
            {
                throw new InvalidOperationException(
                    $"Duplicate authored island ID '{descriptor.IslandId}'.");
            }
            generator.ConfigureWorldManagement(generator == environmentAuthority);
            generator.SetStreamingTarget(null);
            var managed = new ManagedIsland(descriptor, generator, authored);
            managedIslands.Add(managed);
            if (generator == environmentAuthority)
            {
                authorityIsland = managed;
            }
        }
        if (authorityIsland == null)
        {
            throw new InvalidOperationException(
                "The environment authority must also be present in the authored island list.");
        }
    }

    private void UpdateTravelPrediction(Vector3 targetPosition)
    {
        if (hasPreviousTargetPosition && Time.unscaledDeltaTime > 0f)
        {
            var instantaneous = (targetPosition - previousTargetPosition)
                / Time.unscaledDeltaTime;
            instantaneous.y = 0f;
            var response = 1f - Mathf.Exp(-Time.unscaledDeltaTime * 3f);
            smoothedVelocity = Vector3.Lerp(smoothedVelocity, instantaneous, response);
        }
        else
        {
            smoothedVelocity = Vector3.zero;
            hasPreviousTargetPosition = true;
        }
        previousTargetPosition = targetPosition;
        var lookAhead = Vector3.ClampMagnitude(
            smoothedVelocity * velocityLookAheadSeconds,
            maximumLookAheadMetres);
        projectedGenerationPosition = targetPosition + lookAhead;
    }

    private void RefreshDiscovery(Vector3 currentPosition, Vector3 projectedPosition)
    {
        discoveryScanCells.Clear();
        AddDiscoveryCells(currentPosition);
        AddDiscoveryCells(projectedPosition);
        foreach (var cell in discoveryScanCells)
        {
            if (discoveredIslands.ContainsKey(cell)
                || !TryCreateDiscoveredDescriptor(cell, out var descriptor))
            {
                continue;
            }
            var managed = new ManagedIsland(descriptor, null, null);
            discoveredIslands.Add(cell, managed);
            managedIslands.Add(managed);
        }
        PruneDistantDescriptors(currentPosition, projectedPosition);
    }

    private void AddDiscoveryCells(Vector3 centre)
    {
        var centreCell = new Vector2Int(
            Mathf.RoundToInt(centre.x / oceanCellSizeMetres),
            Mathf.RoundToInt(centre.z / oceanCellSizeMetres));
        var cellRadius = Mathf.CeilToInt(discoveryRadiusMetres / oceanCellSizeMetres) + 1;
        var maximumDistance = discoveryRadiusMetres
            + oceanCellSizeMetres * 0.75f;
        for (var z = -cellRadius; z <= cellRadius; z++)
        {
            for (var x = -cellRadius; x <= cellRadius; x++)
            {
                var cell = centreCell + new Vector2Int(x, z);
                var cellPosition = new Vector3(
                    cell.x * oceanCellSizeMetres,
                    centre.y,
                    cell.y * oceanCellSizeMetres);
                if (HorizontalDistance(cellPosition, centre) <= maximumDistance)
                {
                    discoveryScanCells.Add(cell);
                }
            }
        }
    }

    private bool TryCreateDiscoveredDescriptor(
        Vector2Int cell,
        out IslandDescriptor descriptor)
    {
        descriptor = default;
        var radius = environmentAuthority.WorldSizeMetres * 0.5f;
        var candidate = IslandDescriptor.ProceduralCandidate(
            worldSeed,
            cell,
            oceanCellSizeMetres,
            islandCellOccupancy,
            islandCellJitterFraction,
            radius);
        if (!candidate.Occupied)
        {
            return false;
        }

        var maximumJitter = oceanCellSizeMetres * islandCellJitterFraction;
        var conflictDistance = radius * 2f + minimumIslandGapMetres;
        var neighbourRange = Mathf.Max(
            1,
            Mathf.CeilToInt(
                (conflictDistance + maximumJitter * 2f)
                / oceanCellSizeMetres));
        for (var z = -neighbourRange; z <= neighbourRange; z++)
        {
            for (var x = -neighbourRange; x <= neighbourRange; x++)
            {
                var otherCell = cell + new Vector2Int(x, z);
                if (otherCell == cell)
                {
                    continue;
                }
                var other = IslandDescriptor.ProceduralCandidate(
                    worldSeed,
                    otherCell,
                    oceanCellSizeMetres,
                    islandCellOccupancy,
                    islandCellJitterFraction,
                    radius);
                if (!other.Occupied
                    || DescriptorDistance(
                        candidate.Descriptor,
                        other.Descriptor) >= conflictDistance
                    || CandidateWins(candidate, other))
                {
                    continue;
                }
                return false;
            }
        }

        foreach (var entry in managedIslands)
        {
            if (!entry.IsAuthored)
            {
                continue;
            }
            var required = radius
                + entry.Descriptor.EstimatedBoundingRadiusMetres
                + minimumIslandGapMetres;
            if (DescriptorDistance(candidate.Descriptor, entry.Descriptor) < required)
            {
                return false;
            }
        }

        descriptor = candidate.Descriptor;
        return true;
    }

    private static bool CandidateWins(
        ProceduralIslandCandidate candidate,
        ProceduralIslandCandidate other)
    {
        if (candidate.PlacementPriority != other.PlacementPriority)
        {
            return candidate.PlacementPriority < other.PlacementPriority;
        }
        if (candidate.Descriptor.WorldCell.x != other.Descriptor.WorldCell.x)
        {
            return candidate.Descriptor.WorldCell.x < other.Descriptor.WorldCell.x;
        }
        return candidate.Descriptor.WorldCell.y < other.Descriptor.WorldCell.y;
    }

    private void PruneDistantDescriptors(Vector3 current, Vector3 projected)
    {
        removalCells.Clear();
        var retentionRadius = discoveryRadiusMetres + oceanCellSizeMetres * 2f;
        foreach (var pair in discoveredIslands)
        {
            var entry = pair.Value;
            if (entry.Queued
                || entry.Generating
                || (entry.Generator != null && entry.Generator.HasRuntime)
                || DistanceToDescriptor(entry.Descriptor, current) <= retentionRadius
                || DistanceToDescriptor(entry.Descriptor, projected) <= retentionRadius)
            {
                continue;
            }
            removalCells.Add(pair.Key);
        }
        foreach (var cell in removalCells)
        {
            var entry = discoveredIslands[cell];
            DestroyDiscoveredGenerator(entry);
            discoveredIslands.Remove(cell);
            managedIslands.Remove(entry);
        }
    }

    private void QueueDesiredGeneration()
    {
        foreach (var entry in managedIslands)
        {
            if (entry == authorityIsland
                || entry.InitialGenerationPending
                || (IsInsideGenerationCorridor(entry, 0f)
                    && IsPreferredResident(entry)))
            {
                QueueGeneration(entry);
            }
        }
    }

    private void QueueGeneration(ManagedIsland entry)
    {
        if (entry == null
            || entry.Queued
            || entry.Generating
            || (entry.Generator != null && entry.Generator.HasRuntime)
            || Time.unscaledTime < entry.RetryAfterTime)
        {
            return;
        }
        entry.Queued = true;
        if (!queueRunning)
        {
            ProcessGenerationQueue();
        }
    }

    private void CancelObsoleteGeneration()
    {
        foreach (var entry in managedIslands)
        {
            if (entry == authorityIsland || entry.InitialGenerationPending)
            {
                continue;
            }
            var remainsRelevant = IsInsideGenerationCorridor(
                    entry,
                    cancellationHysteresisMetres)
                && IsPreferredResident(entry);
            if (entry.Queued && !remainsRelevant)
            {
                entry.Queued = false;
                entry.GenerationToken++;
            }
            if (entry.Generating && !remainsRelevant)
            {
                entry.GenerationToken++;
                entry.GenerationCancellation?.Cancel();
            }
        }
    }

    private bool IsInsideGenerationCorridor(
        ManagedIsland entry,
        float hysteresis)
    {
        var radius = generationRadiusMetres
            + entry.Descriptor.EstimatedBoundingRadiusMetres
            + hysteresis;
        return DistanceToDescriptor(entry.Descriptor, lastQueryPosition) <= radius
            || DistanceToDescriptor(entry.Descriptor, projectedGenerationPosition) <= radius;
    }

    private bool HasReadyQueuedIsland()
    {
        foreach (var entry in managedIslands)
        {
            if (entry.Queued && Time.unscaledTime >= entry.RetryAfterTime)
            {
                return true;
            }
        }
        return false;
    }

    private async void ProcessGenerationQueue()
    {
        queueRunning = true;
        try
        {
            while (!destroyed)
            {
                var entry = SelectNextQueuedIsland();
                if (entry == null)
                {
                    break;
                }
                entry.Queued = false;
                if (entry.Generator != null && entry.Generator.HasRuntime)
                {
                    continue;
                }
                if (entry != authorityIsland
                    && (environmentAuthority == null
                        || !environmentAuthority.HasInstalledWorldEnvironment))
                {
                    entry.RetryAfterTime = Time.unscaledTime
                        + failedGenerationRetrySeconds;
                    continue;
                }

                if (!ReserveResidentCapacity(entry, out var releasedRuntime))
                {
                    entry.RetryAfterTime = Time.unscaledTime + 1f;
                    continue;
                }
                if (releasedRuntime)
                {
                    // Unity object destruction completes at the frame boundary.
                    // Yield before allocating the next prepared island so old
                    // meshes and textures cannot accumulate generation by generation.
                    await System.Threading.Tasks.Task.Yield();
                }

                var generator = EnsureGenerator(entry);
                if (generator == null)
                {
                    entry.RetryAfterTime = Time.unscaledTime
                        + failedGenerationRetrySeconds;
                    continue;
                }

                var generationToken = ++entry.GenerationToken;
                var cancellation = CancellationTokenSource.CreateLinkedTokenSource(
                    shutdown.Token);
                entry.GenerationCancellation = cancellation;
                entry.Generating = true;
                currentGeneration = entry;
                var generated = await generator.GenerateAsync(
                    entry.Descriptor,
                    cancellation.Token,
                    installationBudgetMilliseconds);
                var stale = generationToken != entry.GenerationToken;
                var wasCancelled = cancellation.IsCancellationRequested;
                entry.Generating = false;
                entry.GenerationCancellation = null;
                currentGeneration = null;
                cancellation.Dispose();

                if (stale)
                {
                    if (generated)
                    {
                        generator.Clear();
                    }
                    continue;
                }
                if (generated)
                {
                    entry.InitialGenerationPending = false;
                    if (!entry.IsAuthored
                        && !IsInsideGenerationCorridor(
                            entry,
                            cancellationHysteresisMetres))
                    {
                        generator.Clear();
                    }
                    if (hasQueryPosition)
                    {
                        UpdateResidencyAndFocus(lastQueryPosition);
                    }
                }
                else if (!wasCancelled && !destroyed)
                {
                    entry.RetryAfterTime = Time.unscaledTime
                        + failedGenerationRetrySeconds;
                }
            }
        }
        catch (OperationCanceledException)
        {
        }
        finally
        {
            currentGeneration = null;
            queueRunning = false;
        }
    }

    private ManagedIsland SelectNextQueuedIsland()
    {
        ManagedIsland selected = null;
        var selectedPriority = float.PositiveInfinity;
        foreach (var entry in managedIslands)
        {
            if (!entry.Queued || Time.unscaledTime < entry.RetryAfterTime)
            {
                continue;
            }
            var priority = GenerationPriority(entry);
            if (priority < selectedPriority)
            {
                selected = entry;
                selectedPriority = priority;
            }
        }
        return selected;
    }

    private float GenerationPriority(ManagedIsland entry)
    {
        if (entry == authorityIsland)
        {
            return float.MinValue;
        }
        var currentDistance = DistanceToDescriptor(entry.Descriptor, lastQueryPosition);
        var forwardDistance = DistanceToDescriptor(
            entry.Descriptor,
            projectedGenerationPosition) * forwardPriorityMultiplier;
        var priority = Mathf.Min(currentDistance, forwardDistance);
        if (entry.InitialGenerationPending)
        {
            priority -= discoveryRadiusMetres * 2f;
        }
        return priority;
    }

    private IslandGenerator EnsureGenerator(ManagedIsland entry)
    {
        if (entry.Generator != null)
        {
            return entry.Generator;
        }
        if (string.IsNullOrEmpty(generatorTemplateJson)
            || environmentAuthority == null)
        {
            Debug.LogError(
                $"Cannot instantiate discovered island '{entry.Descriptor.IslandId}' without an environment-authority template.",
                this);
            return null;
        }

        var islandObject = new GameObject(
            $"Discovered Island {entry.Descriptor.WorldCell.x},{entry.Descriptor.WorldCell.y}");
        islandObject.SetActive(false);
        islandObject.transform.SetParent(transform, true);
        islandObject.transform.SetPositionAndRotation(
            new Vector3(
                (float)entry.Descriptor.LogicalXMetres,
                SeaLevelWorldY(),
                (float)entry.Descriptor.LogicalZMetres),
            Quaternion.Euler(0f, entry.Descriptor.RotationDegrees, 0f));
        var generator = islandObject.AddComponent<IslandGenerator>();
        JsonUtility.FromJsonOverwrite(generatorTemplateJson, generator);
        generator.Generation.Seed = entry.Descriptor.Seed;
        generator.ConfigureWorldManagement(false);
        generator.SetStreamingTarget(null);
        entry.Generator = generator;
        islandObject.SetActive(true);
        return generator;
    }

    private void UpdateResidencyAndFocus(Vector3 worldPosition)
    {
        foreach (var entry in managedIslands)
        {
            var generator = entry.Generator;
            if (generator == null || generator.IsGenerating || !generator.HasRuntime)
            {
                continue;
            }
            var distance = DistanceToDescriptor(entry.Descriptor, worldPosition);
            var canUnload = entry != authorityIsland
                && (!entry.IsAuthored || unloadAuthoredBeyondRadius);
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
        var retainedRadius = detailFocusRadiusMetres + focusHysteresisMetres;
        if (focusedIsland != null
            && focusedIsland.HasRuntime
            && HorizontalDistance(focusedIsland.transform.position, worldPosition)
                <= retainedRadius)
        {
            return focusedIsland;
        }

        IslandGenerator closest = null;
        var closestDistance = float.PositiveInfinity;
        foreach (var entry in managedIslands)
        {
            var generator = entry.Generator;
            if (generator == null || !generator.HasRuntime)
            {
                continue;
            }
            var distance = HorizontalDistance(generator.transform.position, worldPosition);
            if (distance <= detailFocusRadiusMetres && distance < closestDistance)
            {
                closest = generator;
                closestDistance = distance;
            }
        }
        return closest;
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
        environmentAuthority?.SetWorldEnvironmentFollowTarget(streamingTarget);
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
        if (candidate == authorityIsland || candidate.InitialGenerationPending)
        {
            return true;
        }
        var availableSlots = ResidentIslandLimit - 1;
        if (availableSlots <= 0)
        {
            return false;
        }

        var betterCandidates = 0;
        foreach (var other in managedIslands)
        {
            if (other == candidate
                || other == authorityIsland
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
        foreach (var entry in managedIslands)
        {
            if (entry == incoming
                || entry == authorityIsland
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
        return environmentAuthority != null
            ? environmentAuthority.transform.position.y
            : 0f;
    }

    private float EffectiveUnloadRadius()
    {
        return Mathf.Max(
            unloadRadiusMetres,
            activeRadiusMetres + activeHysteresisMetres + 1f);
    }

    private static bool ContainsXZ(IslandGenerator generator, Vector3 point)
    {
        var local = generator.transform.InverseTransformPoint(point);
        var halfSize = generator.WorldSizeMetres * 0.5f;
        return Mathf.Abs(local.x) <= halfSize && Mathf.Abs(local.z) <= halfSize;
    }

    private static float DistanceToDescriptor(
        IslandDescriptor descriptor,
        Vector3 position)
    {
        var x = descriptor.LogicalXMetres - position.x;
        var z = descriptor.LogicalZMetres - position.z;
        return (float)Math.Sqrt(x * x + z * z);
    }

    private static double DescriptorDistance(
        IslandDescriptor first,
        IslandDescriptor second)
    {
        var x = first.LogicalXMetres - second.LogicalXMetres;
        var z = first.LogicalZMetres - second.LogicalZMetres;
        return Math.Sqrt(x * x + z * z);
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

        var first = IslandDescriptor.ProceduralCandidate(
            8128,
            new Vector2Int(-7, 11),
            5200f,
            1f,
            0.18f,
            1000f);
        var repeated = IslandDescriptor.ProceduralCandidate(
            8128,
            new Vector2Int(-7, 11),
            5200f,
            1f,
            0.18f,
            1000f);
        if (!first.Occupied
            || !repeated.Occupied
            || first.Descriptor.IslandId != repeated.Descriptor.IslandId
            || first.Descriptor.Seed != repeated.Descriptor.Seed
            || first.Descriptor.LogicalXMetres != repeated.Descriptor.LogicalXMetres
            || first.Descriptor.LogicalZMetres != repeated.Descriptor.LogicalZMetres
            || first.PlacementPriority != repeated.PlacementPriority)
        {
            throw new InvalidOperationException(
                "Procedural island descriptors are not deterministic.");
        }

        var sourceObject = new GameObject("Island generator template validation");
        var cloneObject = new GameObject("Island generator clone validation");
        sourceObject.SetActive(false);
        cloneObject.SetActive(false);
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
                    "Discovered island generators cannot reproduce the authority profile.");
            }
        }
        finally
        {
            DestroyImmediate(sourceObject);
            DestroyImmediate(cloneObject);
        }
    }
#endif
}
