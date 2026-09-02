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
        internal float RetryAfterTime { get; set; }

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

    [Header("Authored Islands")]
    [SerializeField] private List<AuthoredIsland> islands = new List<AuthoredIsland>();
    [Tooltip("Generator whose settings own the global sky, ocean, weather, and solar clock. Defaults to the first entry.")]
    [SerializeField] private IslandGenerator environmentAuthority;

    [Header("Player Routing")]
    [SerializeField] private Transform streamingTarget;
    [Min(50f)] [SerializeField] private float detailFocusRadiusMetres = 1400f;
    [Min(0f)] [SerializeField] private float focusHysteresisMetres = 250f;

    [Header("Island Residency")]
    [Min(100f)] [SerializeField] private float activeRadiusMetres = 6000f;
    [Min(0f)] [SerializeField] private float activeHysteresisMetres = 500f;
    [Min(100f)] [SerializeField] private float unloadRadiusMetres = 9000f;
    [SerializeField] private bool unloadBeyondRadius;
    [Min(1f)] [SerializeField] private float failedGenerationRetrySeconds = 15f;

    private readonly Queue<AuthoredIsland> generationQueue = new Queue<AuthoredIsland>();
    private readonly HashSet<IslandGenerator> queuedGenerators = new HashSet<IslandGenerator>();
    private CancellationTokenSource shutdown;
    private IslandGenerator focusedIsland;
    private bool queueRunning;
    private bool destroyed;
    private Vector3 lastQueryPosition;
    private bool hasQueryPosition;

    public IslandGenerator FocusedIsland => focusedIsland;
    public int AuthoredIslandCount => islands.Count;
    public int LoadedIslandCount
    {
        get
        {
            var count = 0;
            foreach (var entry in islands)
            {
                if (entry?.Generator != null && entry.Generator.HasRuntime)
                {
                    count++;
                }
            }
            return count;
        }
    }

    private void Awake()
    {
        shutdown = new CancellationTokenSource();
        DiscoverChildGeneratorsWhenUnconfigured();
        ConfigureAuthoredGenerators();
    }

    private void Start()
    {
        if (environmentAuthority != null)
        {
            EnqueueGeneration(FindEntry(environmentAuthority));
        }
        foreach (var entry in islands)
        {
            if (entry != null
                && entry.GenerateOnStart
                && entry.Generator != environmentAuthority)
            {
                EnqueueGeneration(entry);
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
        lastQueryPosition = target.position;
        hasQueryPosition = true;
        UpdateResidencyAndFocus(lastQueryPosition);
        environmentAuthority?.SetWorldEnvironmentFollowTarget(target);
    }

    private void LateUpdate()
    {
        if (environmentAuthority == null)
        {
            return;
        }
        foreach (var entry in islands)
        {
            var generator = entry?.Generator;
            if (generator != null && generator != environmentAuthority && generator.HasRuntime)
            {
                generator.SyncSharedWorldLighting(environmentAuthority);
            }
        }
    }

    private void OnDestroy()
    {
        destroyed = true;
        shutdown?.Cancel();
        shutdown?.Dispose();
        shutdown = null;
        generationQueue.Clear();
        queuedGenerators.Clear();
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
        foreach (var entry in islands)
        {
            var generator = entry?.Generator;
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
        if (FindEntry(environmentAuthority) == null)
        {
            throw new InvalidOperationException(
                "The environment authority must also be present in the authored island list.");
        }

        var ids = new HashSet<string>(StringComparer.Ordinal);
        foreach (var entry in islands)
        {
            var generator = entry?.Generator;
            if (generator == null)
            {
                continue;
            }
            var descriptor = entry.CreateDescriptor();
            if (!ids.Add(descriptor.IslandId))
            {
                throw new InvalidOperationException(
                    $"Duplicate authored island ID '{descriptor.IslandId}'.");
            }
            generator.ConfigureWorldManagement(generator == environmentAuthority);
            generator.SetStreamingTarget(null);
        }
    }

    private void UpdateResidencyAndFocus(Vector3 worldPosition)
    {
        foreach (var entry in islands)
        {
            var generator = entry?.Generator;
            if (generator == null || generator.IsGenerating)
            {
                continue;
            }
            var distance = HorizontalDistance(generator.transform.position, worldPosition);
            if (!generator.HasRuntime)
            {
                if (distance <= activeRadiusMetres + activeHysteresisMetres)
                {
                    EnqueueGeneration(entry);
                }
                continue;
            }
            if (unloadBeyondRadius && distance > EffectiveUnloadRadius())
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
        foreach (var entry in islands)
        {
            var generator = entry?.Generator;
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
        if (selected != null
            && selected.Runtime.State == IslandRuntimeState.Dormant)
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

    private void EnqueueGeneration(AuthoredIsland entry)
    {
        var generator = entry?.Generator;
        if (generator == null
            || generator.HasRuntime
            || generator.IsGenerating
            || Time.unscaledTime < entry.RetryAfterTime
            || !queuedGenerators.Add(generator))
        {
            return;
        }
        generationQueue.Enqueue(entry);
        if (!queueRunning)
        {
            ProcessGenerationQueue();
        }
    }

    private async void ProcessGenerationQueue()
    {
        queueRunning = true;
        try
        {
            while (!destroyed && generationQueue.Count != 0)
            {
                var entry = generationQueue.Dequeue();
                var generator = entry.Generator;
                queuedGenerators.Remove(generator);
                if (generator == null || generator.HasRuntime)
                {
                    continue;
                }
                if (generator != environmentAuthority
                    && (environmentAuthority == null
                        || !environmentAuthority.HasInstalledWorldEnvironment))
                {
                    Debug.LogError(
                        $"Skipped island '{generator.name}' because the environment-authority island failed to load.",
                        this);
                    entry.RetryAfterTime = Time.unscaledTime
                        + failedGenerationRetrySeconds;
                    continue;
                }
                var generated = await generator.GenerateAsync(
                    entry.CreateDescriptor(),
                    shutdown.Token);
                if (generated && hasQueryPosition)
                {
                    UpdateResidencyAndFocus(lastQueryPosition);
                }
                else if (!generated && !destroyed)
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
            queueRunning = false;
        }
    }

    private AuthoredIsland FindEntry(IslandGenerator generator)
    {
        foreach (var entry in islands)
        {
            if (entry?.Generator == generator)
            {
                return entry;
            }
        }
        return null;
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

    private static float HorizontalDistance(Vector3 first, Vector3 second)
    {
        var x = first.x - second.x;
        var z = first.z - second.z;
        return Mathf.Sqrt(x * x + z * z);
    }

#if UNITY_EDITOR
    public static void ValidateRoutingPolicy()
    {
        const float active = 6000f;
        const float hysteresis = 500f;
        const float unload = 9000f;
        if (!(active < active + hysteresis && active + hysteresis < unload))
        {
            throw new InvalidOperationException(
                "Island residency thresholds do not preserve hysteresis.");
        }
        if (HorizontalDistance(new Vector3(3f, 80f, 4f), Vector3.zero) != 5f)
        {
            throw new InvalidOperationException(
                "Island routing distance must ignore elevation.");
        }
    }
#endif
}
