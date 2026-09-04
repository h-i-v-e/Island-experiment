using System;
using System.Collections.Generic;
using System.Threading;
using UnityEngine;

public sealed partial class IslandWorldManager
{
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
            if (managedIslands.ContainsKey(cell)
                || !TryCreateDiscoveredDescriptor(cell, out var descriptor))
            {
                continue;
            }
            var managed = new ManagedIsland(descriptor, null, null);
            managedIslands.Add(cell, managed);
        }
        PruneDistantDescriptors(currentPosition, projectedPosition);
    }

    private void AddDiscoveryCells(Vector3 centre)
    {
        var centreCell = WorldToCell(centre);
        var cellRadius = Mathf.CeilToInt(
            discoveryRadiusMetres / IslandCellSizeMetres) + 1;
        var maximumDistance = discoveryRadiusMetres
            + IslandCellSizeMetres * 0.75f;
        for (var z = -cellRadius; z <= cellRadius; z++)
        {
            for (var x = -cellRadius; x <= cellRadius; x++)
            {
                var cell = centreCell + new Vector2Int(x, z);
                var cellPosition = CellCentre(cell, centre.y);
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
        return IslandDescriptor.TryCreateProcedural(
            worldSeed,
            cell,
            IslandCellSizeMetres,
            islandCellOccupancy,
            out descriptor);
    }

    private void PruneDistantDescriptors(Vector3 current, Vector3 projected)
    {
        removalCells.Clear();
        var retentionRadius = discoveryRadiusMetres + IslandCellSizeMetres * 2f;
        foreach (var pair in managedIslands)
        {
            var entry = pair.Value;
            if (entry.IsAuthored
                || entry.Queued
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
            var entry = managedIslands[cell];
            DestroyDiscoveredGenerator(entry);
            managedIslands.Remove(cell);
        }
    }

    private void QueueDesiredGeneration()
    {
        foreach (var entry in managedIslands.Values)
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
        foreach (var entry in managedIslands.Values)
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
        foreach (var entry in managedIslands.Values)
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
        foreach (var entry in managedIslands.Values)
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
            Quaternion.identity);
        var generator = islandObject.AddComponent<IslandGenerator>();
        JsonUtility.FromJsonOverwrite(generatorTemplateJson, generator);
        generator.Generation.Seed = entry.Descriptor.Seed;
        generator.ConfigureWorldManagement(false);
        generator.SetStreamingTarget(null);
        entry.Generator = generator;
        islandObject.SetActive(true);
        return generator;
    }

}
