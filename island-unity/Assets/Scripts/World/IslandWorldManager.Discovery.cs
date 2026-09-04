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
            if (managedIslands.ContainsKey(cell))
            {
                continue;
            }

            var request = CreateGenerationRequestForCell(cell);
            if (request == null)
            {
                continue;
            }
            var managed = new IslandRuntimeEntry(request);
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

    private IslandGenerationRequest CreateGenerationRequestForCell(
        Vector2Int cell)
    {
        var randomSeed = IslandDescriptor.ProceduralSeed(worldSeed, cell);
        var request = IslandGenerationRequestFactory.CreateIslandGenerationRequest(
            randomSeed,
            cell);
        if (request == null)
        {
            return null;
        }
        ValidateGenerationRequest(request, randomSeed, cell);
        return request;
    }

    private void PruneDistantDescriptors(Vector3 current, Vector3 projected)
    {
        removalCells.Clear();
        var retentionRadius = discoveryRadiusMetres + IslandCellSizeMetres * 2f;
        foreach (var pair in managedIslands)
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
            var entry = managedIslands[cell];
            DestroyGenerator(entry);
            managedIslands.Remove(cell);
        }
    }

    private void QueueDesiredGeneration()
    {
        foreach (var entry in managedIslands.Values)
        {
            if (IsInsideGenerationCorridor(entry, 0f)
                && IsPreferredResident(entry))
            {
                QueueGeneration(entry);
            }
        }
    }

    private void QueueGeneration(IslandRuntimeEntry entry)
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
        IslandRuntimeEntry entry,
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

                var request = entry.GenerationRequest;

                var generationToken = ++entry.GenerationToken;
                var cancellation = CancellationTokenSource.CreateLinkedTokenSource(
                    shutdown.Token);
                entry.GenerationCancellation = cancellation;
                entry.Generating = true;
                currentGeneration = entry;
                var generated = await generator.GenerateAsync(
                    request,
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
                    if (!IsInsideGenerationCorridor(
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

    private static void ValidateGenerationRequest(
        IslandGenerationRequest request,
        int expectedSeed,
        Vector2Int expectedCell)
    {
        if (request.RandomSeed != expectedSeed
            || request.IslandGridPosition != expectedCell)
        {
            throw new InvalidOperationException(
                $"{nameof(IIslandGenerationRequestFactory)} must preserve the supplied "
                + $"seed ({expectedSeed}) and grid position ({expectedCell}).");
        }
    }

    private IslandRuntimeEntry SelectNextQueuedIsland()
    {
        IslandRuntimeEntry selected = null;
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

    private float GenerationPriority(IslandRuntimeEntry entry)
    {
        var currentDistance = DistanceToDescriptor(entry.Descriptor, lastQueryPosition);
        var forwardDistance = DistanceToDescriptor(
            entry.Descriptor,
            projectedGenerationPosition) * forwardPriorityMultiplier;
        return Mathf.Min(currentDistance, forwardDistance);
    }

    private IslandGenerator EnsureGenerator(IslandRuntimeEntry entry)
    {
        if (entry.Generator != null)
        {
            return entry.Generator;
        }
        var islandObject = new GameObject(
            $"Island {entry.Descriptor.WorldCell.x},{entry.Descriptor.WorldCell.y}");
        islandObject.SetActive(false);
        islandObject.transform.SetParent(transform, true);
        islandObject.transform.SetPositionAndRotation(
            new Vector3(
                (float)entry.Descriptor.LogicalXMetres,
                SeaLevelWorldY(),
                (float)entry.Descriptor.LogicalZMetres),
            Quaternion.identity);
        var generator = islandObject.AddComponent<IslandGenerator>();
        entry.GenerationRequest.ApplyProfileTo(generator);
        generator.ConfigureWorldManagement();
        generator.SetStreamingTarget(null);
        entry.Generator = generator;
        islandObject.SetActive(true);
        return generator;
    }

}
