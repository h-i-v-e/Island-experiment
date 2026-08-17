using System;
using UnityEngine;
using UnityEngine.Rendering;

public sealed class RockDecorationPool : MonoBehaviour
{
    internal const int PoolSize = 128;

    private static readonly int RockTintId = Shader.PropertyToID("_RockTint");

    private sealed class Slot
    {
        internal readonly GameObject gameObject;
        internal readonly MeshFilter filter;
        internal readonly MeshRenderer renderer;
        internal readonly MaterialPropertyBlock properties;
        internal int candidateIndex = -1;

        internal Slot(
            GameObject gameObject,
            MeshFilter filter,
            MeshRenderer renderer,
            MaterialPropertyBlock properties)
        {
            this.gameObject = gameObject;
            this.filter = filter;
            this.renderer = renderer;
            this.properties = properties;
        }
    }

    private readonly Slot[] slots = new Slot[PoolSize];
    private readonly int[] nearestIndices = new int[PoolSize];
    private readonly float[] nearestDistances = new float[PoolSize];

    private RockDecorationIndex index;
    private RockPrototypeLibrary prototypes;
    private Material sharedMaterial;
    private Vector3 currentPlayerPosition;
    private Vector2Int currentLod1 = new Vector2Int(-1, -1);
    private bool hasPlayerFocus;
    private bool rocksVisible;
    private bool disposed;

    internal int CandidateCount => index?.Count ?? 0;
    internal int ActiveCount { get; private set; }
    internal int DroppedCount { get; private set; }
    internal int PrototypeCount => prototypes?.Count ?? 0;

    internal void Initialize(
        IslandViewer.PreparedRockDecoration[] candidates,
        float worldSize,
        Material rockMaterial,
        bool visible)
    {
        if (rockMaterial == null)
        {
            throw new ArgumentNullException(nameof(rockMaterial));
        }
        index = new RockDecorationIndex(candidates, worldSize);
        prototypes = new RockPrototypeLibrary();
        sharedMaterial = rockMaterial;
        rocksVisible = visible;
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            slots[slotIndex] = CreateSlot(slotIndex);
        }
    }

    internal void SetPlayerPosition(Vector3 worldPosition, Vector2Int activeLod1)
    {
        currentPlayerPosition = worldPosition;
        hasPlayerFocus = true;
        if (!rocksVisible || index == null || activeLod1 == currentLod1)
        {
            return;
        }
        currentLod1 = activeLod1;
        RefreshAssignments(worldPosition, activeLod1);
    }

    internal void ClearPlayerFocus()
    {
        hasPlayerFocus = false;
        currentLod1 = new Vector2Int(-1, -1);
        ClearAssignments();
    }

    internal void SetRocksVisible(bool visible)
    {
        rocksVisible = visible;
        if (!visible)
        {
            ClearAssignments();
            return;
        }
        if (hasPlayerFocus)
        {
            var activeLod1 = currentLod1;
            currentLod1 = new Vector2Int(-1, -1);
            SetPlayerPosition(currentPlayerPosition, activeLod1);
        }
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
        prototypes?.Dispose();
        prototypes = null;
        sharedMaterial = null;
        index = null;
    }

    private void RefreshAssignments(Vector3 playerPosition, Vector2Int activeLod1)
    {
        var nearestCount = 0;
        var eligibleCount = 0;
        var minimumX = Mathf.Max(activeLod1.x - 1, 0);
        var maximumX = Mathf.Min(activeLod1.x + 1, RockDecorationIndex.Resolution - 1);
        var minimumY = Mathf.Max(activeLod1.y - 1, 0);
        var maximumY = Mathf.Min(activeLod1.y + 1, RockDecorationIndex.Resolution - 1);
        for (var y = minimumY; y <= maximumY; y++)
        {
            for (var x = minimumX; x <= maximumX; x++)
            {
                index.GetCellRange(x, y, out var start, out var end);
                for (var orderIndex = start; orderIndex < end; orderIndex++)
                {
                    var candidateIndex = index.CandidateIndexAt(orderIndex);
                    var distanceSquared =
                        (index.CandidateAt(candidateIndex).position - playerPosition).sqrMagnitude;
                    InsertNearest(candidateIndex, distanceSquared, ref nearestCount);
                    eligibleCount++;
                }
            }
        }

        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            var candidateIndex = slots[slotIndex].candidateIndex;
            if (candidateIndex >= 0 && !IsSelected(candidateIndex, nearestCount))
            {
                ReleaseSlot(slots[slotIndex]);
            }
        }

        for (var selectedIndex = 0; selectedIndex < nearestCount; selectedIndex++)
        {
            var candidateIndex = nearestIndices[selectedIndex];
            if (IsAssigned(candidateIndex))
            {
                continue;
            }
            var freeSlot = FindFreeSlot();
            if (freeSlot < 0)
            {
                break;
            }
            AssignSlot(slots[freeSlot], candidateIndex);
        }

        ActiveCount = nearestCount;
        DroppedCount = Mathf.Max(eligibleCount - nearestCount, 0);
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

    private bool IsSelected(int candidateIndex, int selectedCount)
    {
        for (var index = 0; index < selectedCount; index++)
        {
            if (nearestIndices[index] == candidateIndex)
            {
                return true;
            }
        }
        return false;
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

    private int FindFreeSlot()
    {
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            if (slots[slotIndex].candidateIndex < 0)
            {
                return slotIndex;
            }
        }
        return -1;
    }

    private void AssignSlot(Slot slot, int candidateIndex)
    {
        var candidate = index.CandidateAt(candidateIndex);
        var position = prototypes.SeatPosition(candidate);
        if (!IsFinite(position))
        {
            return;
        }
        slot.candidateIndex = candidateIndex;
        slot.filter.sharedMesh = prototypes.MeshAt(candidate.prototypeIndex);
        slot.gameObject.transform.SetPositionAndRotation(position, candidate.rotation);
        slot.gameObject.transform.localScale = candidate.scale;
        slot.properties.SetColor(RockTintId, candidate.tint);
        slot.renderer.SetPropertyBlock(slot.properties);
        slot.gameObject.SetActive(true);
    }

    private static void ReleaseSlot(Slot slot)
    {
        slot.gameObject.SetActive(false);
        slot.filter.sharedMesh = null;
        slot.candidateIndex = -1;
    }

    private void ClearAssignments()
    {
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            if (slots[slotIndex] != null && slots[slotIndex].candidateIndex >= 0)
            {
                ReleaseSlot(slots[slotIndex]);
            }
        }
        ActiveCount = 0;
        DroppedCount = 0;
    }

    private Slot CreateSlot(int slotIndex)
    {
        var slotObject = new GameObject($"Rock decoration {slotIndex}");
        slotObject.transform.SetParent(transform, false);
        var filter = slotObject.AddComponent<MeshFilter>();
        var renderer = slotObject.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = sharedMaterial;
        renderer.shadowCastingMode = ShadowCastingMode.On;
        renderer.receiveShadows = true;
        var properties = new MaterialPropertyBlock();
        slotObject.SetActive(false);
        return new Slot(slotObject, filter, renderer, properties);
    }

    private static bool IsFinite(float value)
    {
        return !float.IsNaN(value) && !float.IsInfinity(value);
    }

    private static bool IsFinite(Vector3 value)
    {
        return IsFinite(value.x) && IsFinite(value.y) && IsFinite(value.z);
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
