using System;
using UnityEngine;
using UnityEngine.Rendering;

public sealed class WaterfallMistPool : MonoBehaviour
{
    private const int PoolSize = 32;
    private const float ActivationRadius = 180f;
    private const float RetirementRadius = 220f;
    private const float RequeryMovement = 5f;
    private const float ReplacementMargin = 10f;
    private const float SurfaceClearance = 0.08f;
    private const float SeaLevel = 0f;
    private const int Lod0NeighborhoodRadius = 1;

    private static readonly int DensityId = Shader.PropertyToID("_Density");
    private static readonly int NoiseOffsetId = Shader.PropertyToID("_NoiseOffset");

    private sealed class Slot
    {
        internal readonly GameObject volumeObject;
        internal readonly MeshRenderer renderer;
        internal readonly MaterialPropertyBlock properties = new MaterialPropertyBlock();
        internal int footIndex = -1;

        internal Slot(GameObject volumeObject, MeshRenderer renderer)
        {
            this.volumeObject = volumeObject;
            this.renderer = renderer;
        }
    }

    private readonly Slot[] slots = new Slot[PoolSize];
    private readonly int[] nearestIndices = new int[PoolSize];
    private readonly float[] nearestDistances = new float[PoolSize];

    private WaterfallFootIndex index;
    private Mesh sharedVolumeMesh;
    private Material sharedMistMaterial;
    private Vector3 lastQueryPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
    private Vector2Int lastLod1 = new Vector2Int(-1, -1);
    private bool riversVisible;
    private bool debugDraw;
    private Vector3 currentPlayerPosition;
    private bool disposed;

    internal int FootCount => index?.Count ?? 0;
    internal int PoolCount => slots.Length;
    internal int CreatedVolumeCount
    {
        get
        {
            var count = 0;
            for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
            {
                if (slots[slotIndex]?.renderer != null)
                {
                    count++;
                }
            }
            return count;
        }
    }
    internal int ActiveVolumeCount
    {
        get
        {
            var count = 0;
            for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
            {
                if (slots[slotIndex]?.renderer.enabled == true)
                {
                    count++;
                }
            }
            return count;
        }
    }

    internal void Initialize(IslandPreparedWaterfallFoot[] feet, float worldSize, bool visible)
    {
        index = new WaterfallFootIndex(feet, worldSize);
        riversVisible = visible;
        CreateSharedVisuals();
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            slots[slotIndex] = CreateSlot(slotIndex);
        }
    }

    internal void SetPlayerPosition(Vector3 worldPosition, Vector2Int activeLod1)
    {
        currentPlayerPosition = worldPosition;
        if (!riversVisible || index == null || index.Count == 0)
        {
            return;
        }
        var movement = worldPosition - lastQueryPosition;
        if (activeLod1 == lastLod1
            && movement.sqrMagnitude < RequeryMovement * RequeryMovement)
        {
            return;
        }
        RefreshAssignments(worldPosition, activeLod1);
        lastQueryPosition = worldPosition;
        lastLod1 = activeLod1;
    }

    internal void SetDebugDraw(bool enabled)
    {
        debugDraw = enabled;
    }

    internal void ClearPlayerFocus()
    {
        lastQueryPosition = new Vector3(float.PositiveInfinity, 0f, 0f);
        lastLod1 = new Vector2Int(-1, -1);
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
                DestroyUnityObject(slots[slotIndex].volumeObject);
                slots[slotIndex] = null;
            }
        }
        DestroyUnityObject(sharedMistMaterial);
        DestroyUnityObject(sharedVolumeMesh);
        sharedMistMaterial = null;
        sharedVolumeMesh = null;
        index = null;
    }

    private void RefreshAssignments(Vector3 playerPosition, Vector2Int activeLod1)
    {
        var retirementSquared = RetirementRadius * RetirementRadius;
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            var footIndex = slots[slotIndex].footIndex;
            if (footIndex < 0)
            {
                continue;
            }
            var foot = index.FootAt(footIndex);
            if ((foot.position - playerPosition).sqrMagnitude > retirementSquared
                || !IsInActiveLod0Area(foot.position, activeLod1))
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
                    var footIndex = index.CandidateIndexAt(orderIndex);
                    if (IsAssigned(footIndex))
                    {
                        continue;
                    }
                    var foot = index.FootAt(footIndex);
                    if (!IsInActiveLod0Area(foot.position, activeLod1))
                    {
                        continue;
                    }
                    var distanceSquared = (foot.position - playerPosition).sqrMagnitude;
                    if (distanceSquared <= activationSquared)
                    {
                        InsertNearest(footIndex, distanceSquared, ref nearestCount);
                    }
                }
            }
        }

        var nearestOffset = 0;
        for (var slotIndex = 0;
             slotIndex < slots.Length && nearestOffset < nearestCount;
             slotIndex++)
        {
            if (slots[slotIndex].footIndex < 0)
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

    private void InsertNearest(int footIndex, float distanceSquared, ref int count)
    {
        var insertion = count;
        while (insertion > 0
            && (distanceSquared < nearestDistances[insertion - 1]
                || (Mathf.Approximately(distanceSquared, nearestDistances[insertion - 1])
                    && footIndex < nearestIndices[insertion - 1])))
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
        nearestIndices[insertion] = footIndex;
        nearestDistances[insertion] = distanceSquared;
        count = newCount;
    }

    private int FarthestAssignedSlot(Vector3 playerPosition, out float distanceSquared)
    {
        var result = -1;
        distanceSquared = float.NegativeInfinity;
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            var footIndex = slots[slotIndex].footIndex;
            if (footIndex < 0)
            {
                continue;
            }
            var candidateDistance =
                (index.FootAt(footIndex).position - playerPosition).sqrMagnitude;
            if (candidateDistance > distanceSquared)
            {
                distanceSquared = candidateDistance;
                result = slotIndex;
            }
        }
        return result;
    }

    private bool IsAssigned(int footIndex)
    {
        for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
        {
            if (slots[slotIndex].footIndex == footIndex)
            {
                return true;
            }
        }
        return false;
    }

    private bool IsInActiveLod0Area(Vector3 position, Vector2Int activeLod1)
    {
        var cell = index.CellAt(position);
        return Mathf.Abs(cell.x - activeLod1.x) <= Lod0NeighborhoodRadius
            && Mathf.Abs(cell.y - activeLod1.y) <= Lod0NeighborhoodRadius;
    }

    private void AssignSlot(Slot slot, int footIndex)
    {
        var foot = index.FootAt(footIndex);
        var impact = Mathf.InverseLerp(0.5f, 8f, foot.drop);
        // Extend beyond both banks so the continuous lower mist covers the
        // entire impact line even when the waterfall width is near a clamp.
        var width = Mathf.Clamp(foot.halfWidth * 2.7f, 3f, 24f);
        var depth = Mathf.Clamp(foot.halfWidth * 1.25f, 1.5f, 8f);
        var height = Mathf.Clamp(
            Mathf.Lerp(1.2f, 2.4f, impact) + width * 0.12f,
            1.3f,
            3.5f);
        var direction = Vector3.ProjectOnPlane(foot.direction, Vector3.up).normalized;
        if (direction.sqrMagnitude < 0.5f)
        {
            direction = Vector3.forward;
        }
        var impactPosition = foot.position;
        impactPosition.y = Mathf.Max(impactPosition.y, SeaLevel);

        slot.footIndex = footIndex;
        slot.volumeObject.transform.SetPositionAndRotation(
            impactPosition
                - direction * (depth * 0.18f)
                + Vector3.up * (height * 0.28f + SurfaceClearance),
            Quaternion.LookRotation(direction, Vector3.up));
        slot.volumeObject.transform.localScale = new Vector3(width, height, depth);
        slot.properties.SetFloat(DensityId, Mathf.Lerp(1.1f, 1.8f, impact));
        slot.properties.SetVector(
            NoiseOffsetId,
            new Vector4(
                Mathf.Repeat(footIndex * 0.6180339f, 1f) * 17f,
                Mathf.Repeat(footIndex * 0.4142136f, 1f) * 17f,
                Mathf.Repeat(footIndex * 0.7320508f, 1f) * 17f,
                0f));
        slot.renderer.SetPropertyBlock(slot.properties);
        slot.renderer.enabled = true;
    }

    private static void ReleaseSlot(Slot slot)
    {
        slot.renderer.enabled = false;
        slot.footIndex = -1;
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
        var volumeObject = new GameObject($"Waterfall foot fog {slotIndex}");
        volumeObject.layer = gameObject.layer;
        volumeObject.transform.SetParent(transform, false);
        var filter = volumeObject.AddComponent<MeshFilter>();
        filter.sharedMesh = sharedVolumeMesh;
        var renderer = volumeObject.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = sharedMistMaterial;
        renderer.shadowCastingMode = ShadowCastingMode.Off;
        renderer.receiveShadows = false;
        renderer.lightProbeUsage = LightProbeUsage.Off;
        renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
        renderer.allowOcclusionWhenDynamic = false;
        renderer.enabled = false;
        return new Slot(volumeObject, renderer);
    }

    private void CreateSharedVisuals()
    {
        var mistShader = Shader.Find("Motu/Waterfall Foot Mist");
        if (mistShader == null)
        {
            throw new InvalidOperationException("The waterfall-foot mist shader is unavailable.");
        }
        sharedMistMaterial = new Material(mistShader)
        {
            name = "Waterfall foot fog material"
        };
        sharedVolumeMesh = CreateUnitCube();
    }

    private static Mesh CreateUnitCube()
    {
        var mesh = new Mesh { name = "Waterfall foot fog volume" };
        mesh.vertices = new[]
        {
            new Vector3(-0.5f, -0.5f, -0.5f),
            new Vector3(0.5f, -0.5f, -0.5f),
            new Vector3(0.5f, 0.5f, -0.5f),
            new Vector3(-0.5f, 0.5f, -0.5f),
            new Vector3(-0.5f, -0.5f, 0.5f),
            new Vector3(0.5f, -0.5f, 0.5f),
            new Vector3(0.5f, 0.5f, 0.5f),
            new Vector3(-0.5f, 0.5f, 0.5f),
        };
        mesh.triangles = new[]
        {
            0, 3, 2, 0, 2, 1,
            4, 5, 6, 4, 6, 7,
            0, 4, 7, 0, 7, 3,
            1, 2, 6, 1, 6, 5,
            0, 1, 5, 0, 5, 4,
            3, 7, 6, 3, 6, 2,
        };
        mesh.RecalculateBounds();
        mesh.UploadMeshData(true);
        return mesh;
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
        for (var footIndex = 0; footIndex < index.Count; footIndex++)
        {
            var foot = index.FootAt(footIndex);
            if ((foot.position - currentPlayerPosition).sqrMagnitude > retirementSquared)
            {
                continue;
            }
            Gizmos.color = IsAssigned(footIndex)
                ? Color.yellow
                : new Color(1f, 0.45f, 0.9f, 0.75f);
            Gizmos.DrawSphere(foot.position, IsAssigned(footIndex) ? 0.45f : 0.2f);
            Gizmos.DrawRay(foot.position, foot.direction * Mathf.Max(foot.halfWidth, 1f));
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
