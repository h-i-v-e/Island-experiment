using System;
using UnityEngine;
using UnityEngine.Rendering;

public sealed class RiverParticlePool : MonoBehaviour
{
    private const int PoolSize = 32;
    private const float ActivationRadius = 180f;
    private const float RetirementRadius = 220f;
    private const float RequeryMovement = 5f;
    private const float ReplacementMargin = 10f;
    private const float SurfaceClearance = 0.1f;

    private sealed class Slot
    {
        internal readonly GameObject gameObject;
        internal readonly ParticleSystem particles;
        internal int candidateIndex = -1;

        internal Slot(GameObject gameObject, ParticleSystem particles)
        {
            this.gameObject = gameObject;
            this.particles = particles;
        }
    }

    private readonly Slot[] slots = new Slot[PoolSize];
    private readonly int[] nearestIndices = new int[PoolSize];
    private readonly float[] nearestDistances = new float[PoolSize];

    private RiverEmitterIndex index;
    private Material sharedMaterial;
    private Vector3 lastQueryPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
    private Vector2Int lastLod2 = new Vector2Int(-1, -1);
    private bool riversVisible;
    private bool debugDraw;
    private Vector3 currentPlayerPosition;
    private bool disposed;

    internal int CandidateCount => index?.Count ?? 0;
    internal int PoolCount => slots.Length;
    internal int CreatedSystemCount
    {
        get
        {
            var count = 0;
            for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
            {
                if (slots[slotIndex] != null)
                {
                    count++;
                }
            }
            return count;
        }
    }

    internal void Initialize(
        IslandViewer.PreparedRiverEmitter[] candidates,
        float worldSize,
        bool visible)
    {
        index = new RiverEmitterIndex(candidates, worldSize);
        riversVisible = visible;
        CreateSharedVisuals();
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            slots[slotIndex] = CreateSlot(slotIndex);
        }
    }

    internal void SetPlayerPosition(Vector3 worldPosition, Vector2Int activeLod2)
    {
        currentPlayerPosition = worldPosition;
        if (!riversVisible || index == null || index.Count == 0)
        {
            return;
        }
        var movement = worldPosition - lastQueryPosition;
        if (activeLod2 == lastLod2
            && movement.sqrMagnitude < RequeryMovement * RequeryMovement)
        {
            return;
        }
        RefreshAssignments(worldPosition, activeLod2);
        lastQueryPosition = worldPosition;
        lastLod2 = activeLod2;
    }

    internal void SetDebugDraw(bool enabled)
    {
        debugDraw = enabled;
    }

    internal void ClearPlayerFocus()
    {
        lastQueryPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
        lastLod2 = new Vector2Int(-1, -1);
        ClearAssignments();
    }

    internal void SetRiversVisible(bool visible)
    {
        riversVisible = visible;
        if (!visible)
        {
            ClearAssignments();
            return;
        }
        lastQueryPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
    }

    internal void DisposePool()
    {
        if (disposed)
        {
            return;
        }
        disposed = true;
        ClearAssignments();
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            if (slots[slotIndex] != null)
            {
                DestroyUnityObject(slots[slotIndex].gameObject);
                slots[slotIndex] = null;
            }
        }
        DestroyUnityObject(sharedMaterial);
        sharedMaterial = null;
        index = null;
    }

    private void RefreshAssignments(Vector3 playerPosition, Vector2Int activeLod2)
    {
        var retirementSquared = RetirementRadius * RetirementRadius;
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            var candidateIndex = slots[slotIndex].candidateIndex;
            if (candidateIndex < 0)
            {
                continue;
            }
            var candidate = index.CandidateAt(candidateIndex);
            if ((candidate.position - playerPosition).sqrMagnitude > retirementSquared
                || !IsInActiveLod1Area(candidate.position, activeLod2))
            {
                ReleaseSlot(slots[slotIndex]);
            }
        }

        var nearestCount = 0;
        var activationSquared = ActivationRadius * ActivationRadius;
        index.CellsIntersecting(
            playerPosition,
            ActivationRadius,
            out var minimumX,
            out var maximumX,
            out var minimumY,
            out var maximumY);
        for (var y = minimumY; y <= maximumY; y++)
        {
            for (var x = minimumX; x <= maximumX; x++)
            {
                index.GetCellRange(x, y, out var start, out var end);
                for (var orderIndex = start; orderIndex < end; orderIndex++)
                {
                    var candidateIndex = index.CandidateIndexAt(orderIndex);
                    if (IsAssigned(candidateIndex))
                    {
                        continue;
                    }
                    var candidate = index.CandidateAt(candidateIndex);
                    if (!IsInActiveLod1Area(candidate.position, activeLod2))
                    {
                        continue;
                    }
                    var distanceSquared = (candidate.position - playerPosition).sqrMagnitude;
                    if (distanceSquared <= activationSquared)
                    {
                        InsertNearest(
                            candidateIndex,
                            distanceSquared,
                            ref nearestCount);
                    }
                }
            }
        }

        var nearestOffset = 0;
        for (var slotIndex = 0;
             slotIndex < slots.Length && nearestOffset < nearestCount;
             slotIndex++)
        {
            if (slots[slotIndex].candidateIndex < 0)
            {
                AssignSlot(slots[slotIndex], nearestIndices[nearestOffset++]);
            }
        }
        while (nearestOffset < nearestCount)
        {
            var farthestSlot = FarthestAssignedSlot(playerPosition, out var farthestDistance);
            if (farthestSlot < 0)
            {
                break;
            }
            var replacementDistance = Mathf.Sqrt(nearestDistances[nearestOffset]);
            if (replacementDistance + ReplacementMargin >= Mathf.Sqrt(farthestDistance))
            {
                break;
            }
            AssignSlot(slots[farthestSlot], nearestIndices[nearestOffset++]);
        }
    }

    private void InsertNearest(int candidateIndex, float distanceSquared, ref int count)
    {
        var insertion = count;
        while (insertion > 0
            && (distanceSquared < nearestDistances[insertion - 1]
                || (Mathf.Approximately(distanceSquared, nearestDistances[insertion - 1])
                    && candidateIndex < nearestIndices[insertion - 1])))
        {
            insertion--;
        }
        if (insertion >= PoolSize)
        {
            return;
        }
        var newCount = Mathf.Min(count + 1, PoolSize);
        for (var indexToMove = newCount - 1; indexToMove > insertion; indexToMove--)
        {
            nearestIndices[indexToMove] = nearestIndices[indexToMove - 1];
            nearestDistances[indexToMove] = nearestDistances[indexToMove - 1];
        }
        nearestIndices[insertion] = candidateIndex;
        nearestDistances[insertion] = distanceSquared;
        count = newCount;
    }

    private int FarthestAssignedSlot(Vector3 playerPosition, out float distanceSquared)
    {
        var result = -1;
        distanceSquared = float.NegativeInfinity;
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            var candidateIndex = slots[slotIndex].candidateIndex;
            if (candidateIndex < 0)
            {
                continue;
            }
            var candidateDistance =
                (index.CandidateAt(candidateIndex).position - playerPosition).sqrMagnitude;
            if (candidateDistance > distanceSquared)
            {
                distanceSquared = candidateDistance;
                result = slotIndex;
            }
        }
        return result;
    }

    private bool IsAssigned(int candidateIndex)
    {
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            if (slots[slotIndex].candidateIndex == candidateIndex)
            {
                return true;
            }
        }
        return false;
    }

    private bool IsInActiveLod1Area(Vector3 position, Vector2Int activeLod2)
    {
        var cell = index.CellAt(position);
        var parent = new Vector2Int(cell.x / 8, cell.y / 8);
        return Mathf.Abs(parent.x - activeLod2.x) <= 1
            && Mathf.Abs(parent.y - activeLod2.y) <= 1;
    }

    private void AssignSlot(Slot slot, int candidateIndex)
    {
        slot.particles.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
        var candidate = index.CandidateAt(candidateIndex);
        slot.candidateIndex = candidateIndex;
        slot.gameObject.transform.SetPositionAndRotation(
            candidate.position + Vector3.up * SurfaceClearance,
            Quaternion.FromToRotation(Vector3.forward, candidate.direction));

        var main = slot.particles.main;
        var maximumSpeed = Mathf.Lerp(0.3f, 1.35f, candidate.strength);
        main.startSpeed = new ParticleSystem.MinMaxCurve(maximumSpeed * 0.4f, maximumSpeed);
        main.startSize = Mathf.Lerp(0.035f, 0.12f, candidate.strength);
        main.startColor = new Color(0.82f, 0.93f, 1f, Mathf.Lerp(0.22f, 0.58f, candidate.strength));
        var emission = slot.particles.emission;
        emission.rateOverTime = Mathf.Lerp(24f, 96f, candidate.strength);
        slot.particles.Play(true);
    }

    private static void ReleaseSlot(Slot slot)
    {
        if (slot.particles != null)
        {
            slot.particles.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
        }
        slot.candidateIndex = -1;
    }

    private void ClearAssignments()
    {
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            if (slots[slotIndex] != null)
            {
                ReleaseSlot(slots[slotIndex]);
            }
        }
    }

    private Slot CreateSlot(int slotIndex)
    {
        var slotObject = new GameObject($"Rough water emitter {slotIndex}");
        slotObject.transform.SetParent(transform, false);
        var particles = slotObject.AddComponent<ParticleSystem>();
        var main = particles.main;
        main.loop = true;
        main.playOnAwake = false;
        main.simulationSpace = ParticleSystemSimulationSpace.World;
        main.startLifetime = new ParticleSystem.MinMaxCurve(0.7f, 1.6f);
        main.gravityModifier = 0.6f;
        main.maxParticles = 128;
        var emission = particles.emission;
        emission.rateOverTime = 0f;
        var shape = particles.shape;
        shape.enabled = true;
        shape.shapeType = ParticleSystemShapeType.Cone;
        shape.angle = 80f;
        shape.radius = 0.08f;
        shape.randomDirectionAmount = 0.18f;
        var noise = particles.noise;
        noise.enabled = true;
        noise.strength = 0.18f;
        noise.frequency = 1.2f;
        noise.scrollSpeed = 0.18f;
        noise.damping = true;
        var renderer = particles.GetComponent<ParticleSystemRenderer>();
        renderer.sharedMaterial = sharedMaterial;
        renderer.renderMode = ParticleSystemRenderMode.Billboard;
        renderer.shadowCastingMode = ShadowCastingMode.Off;
        renderer.receiveShadows = false;
        particles.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
        return new Slot(slotObject, particles);
    }

    private void CreateSharedVisuals()
    {
        var shader = Shader.Find("Motu/River Spray Particle");
        if (shader == null)
        {
            throw new InvalidOperationException("The river spray particle shader is unavailable.");
        }
        sharedMaterial = new Material(shader)
        {
            name = "Rough water particle material"
        };
    }

    private void OnDrawGizmos()
    {
        if (!debugDraw || !HasActiveIndex())
        {
            return;
        }
        Gizmos.color = new Color(0.25f, 0.75f, 1f, 0.7f);
        Gizmos.DrawWireSphere(currentPlayerPosition, ActivationRadius);
        var retirementSquared = RetirementRadius * RetirementRadius;
        for (var candidateIndex = 0; candidateIndex < index.Count; candidateIndex++)
        {
            var candidate = index.CandidateAt(candidateIndex);
            if ((candidate.position - currentPlayerPosition).sqrMagnitude > retirementSquared)
            {
                continue;
            }
            Gizmos.color = IsAssigned(candidateIndex)
                ? Color.yellow
                : new Color(0.35f, 0.8f, 1f, 0.65f);
            Gizmos.DrawSphere(candidate.position, IsAssigned(candidateIndex) ? 0.45f : 0.2f);
            Gizmos.DrawRay(candidate.position, candidate.direction * 2f);
        }
    }

    private bool HasActiveIndex()
    {
        return index != null && index.Count > 0 && riversVisible;
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(value);
        }
        else
        {
            DestroyImmediate(value);
        }
    }

    private void OnDestroy()
    {
        DisposePool();
    }
}
