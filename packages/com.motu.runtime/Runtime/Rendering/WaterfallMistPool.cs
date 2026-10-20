using System;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Islands;
using Motu.Settings;
using Motu.World;

namespace Motu.Rendering
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "WaterfallMistPool")]
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
        // WaterfallPatch.face_run(): two 1.25 m refinement edges in a 2000 m island.
        private const float NormalizedFaceRun = 2.5f / IslandWorldManager.IslandSizeMetres;

        private static readonly int DensityId = Shader.PropertyToID("_Density");
        private static readonly int NoiseOffsetId = Shader.PropertyToID("_NoiseOffset");

        private sealed class Slot
        {
            internal readonly GameObject volumeObject;
            internal readonly Transform volumeTransform;
            internal readonly MeshRenderer renderer;
            internal readonly AudioSource audio;
            internal readonly MaterialPropertyBlock properties = new MaterialPropertyBlock();
            internal int footIndex = -1;

            internal Slot(
                GameObject volumeObject,
                Transform volumeTransform,
                MeshRenderer renderer,
                AudioSource audio)
            {
                this.volumeObject = volumeObject;
                this.volumeTransform = volumeTransform;
                this.renderer = renderer;
                this.audio = audio;
            }
        }

        private readonly Slot[] slots = new Slot[PoolSize];
        private readonly int[] nearestIndices = new int[PoolSize];
        private readonly float[] nearestDistances = new float[PoolSize];

        private WaterfallFootIndex index;
        private float faceRun;
        [SerializeField] private WaterfallAudioProfile audioProfile;
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
            faceRun = NormalizedFaceRun * worldSize;
            riversVisible = visible;
            if (audioProfile == null)
                audioProfile = Resources.Load<WaterfallAudioProfile>("WaterfallAudio");
            CreateSharedVisuals();
            for (var slotIndex = 0; slotIndex < slots.Length; slotIndex++)
            {
                slots[slotIndex] = CreateSlot(slotIndex);
            }
        }

        internal void SetPlayerPosition(Vector3 worldPosition, Vector2Int activeLod1)
        {
            currentPlayerPosition = worldPosition;
            if (!isActiveAndEnabled || !riversVisible || index == null || index.Count == 0)
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
                if (DistanceToImpactSquared(foot, playerPosition) > retirementSquared
                    || !IsInActiveLod0Area(foot.position, activeLod1))
                {
                    ReleaseSlot(slots[slotIndex]);
                }
            }

            var nearestCount = 0;
            var activationSquared = ActivationRadius * ActivationRadius;
            index.CellsIntersecting(
                playerPosition,
                ActivationRadius + faceRun,
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
                        var distanceSquared = DistanceToImpactSquared(foot, playerPosition);
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
                var candidateDistance = DistanceToImpactSquared(index.FootAt(footIndex), playerPosition);
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

        private float DistanceToImpactSquared(IslandPreparedWaterfallFoot foot, Vector3 position)
        {
            return TryGetImpactPosition(foot, out var impact)
                ? (impact - position).sqrMagnitude : float.PositiveInfinity;
        }

        private bool TryGetImpactPosition(IslandPreparedWaterfallFoot foot, out Vector3 impact)
        {
            impact = foot.position;
            if (impact.y >= SeaLevel) return true;
            float upperSurface = foot.position.y + foot.drop;
            if (upperSurface <= SeaLevel) return false;

            // Match WaterfallPatch.face_surface_at: the face descends from lip
            // to foot using smoothstep, not a straight segment. Invert that
            // profile at sea level, then move upstream to the actual crossing.
            float target = (upperSurface - SeaLevel) / foot.drop;
            float low = 0f, high = 1f;
            for (int iteration = 0; iteration < 20; iteration++)
            {
                float t = (low + high) * .5f;
                float heightBlend = t * t * (3f - 2f * t);
                if (heightBlend < target) low = t;
                else high = t;
            }
            float progress = (low + high) * .5f;
            impact -= DownstreamDirection(foot) * (faceRun * (1f - progress));
            impact.y = SeaLevel;
            return true;
        }

        private static Vector3 DownstreamDirection(IslandPreparedWaterfallFoot foot)
        {
            var direction = Vector3.ProjectOnPlane(foot.direction, Vector3.up).normalized;
            return direction.sqrMagnitude < .5f ? Vector3.forward : direction;
        }

        private void AssignSlot(Slot slot, int footIndex)
        {
            var foot = index.FootAt(footIndex);
            if (!TryGetImpactPosition(foot, out var impactPosition))
            {
                ReleaseSlot(slot);
                return;
            }
            var impact = Mathf.InverseLerp(0.5f, 8f, foot.drop);
            // Cover the full impact line with overlap at both banks.
            var width = Mathf.Clamp(foot.halfWidth * 2.7f, 3f, 24f);
            var depth = Mathf.Clamp(foot.halfWidth * 1.25f, 1.5f, 8f);
            var height = Mathf.Clamp(
                Mathf.Lerp(1.2f, 2.4f, impact) + width * 0.12f,
                1.3f,
                3.5f);
            var direction = DownstreamDirection(foot);

            slot.footIndex = footIndex;
            // Waterfall records and the player query are island-local. Keep the
            // assigned effect local to the river root as well so translated or
            // rotated islands do not leave their mist and sound near world zero.
            slot.volumeObject.transform.localPosition = impactPosition;
            slot.volumeObject.transform.localRotation = Quaternion.LookRotation(
                direction,
                Vector3.up);
            slot.volumeObject.transform.localScale = Vector3.one;
            slot.volumeTransform.localPosition = new Vector3(
                0f,
                height * 0.28f + SurfaceClearance,
                depth * 0.5f);
            slot.volumeTransform.localRotation = Quaternion.identity;
            slot.volumeTransform.localScale = new Vector3(width, height, depth);
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
            if (slot.audio != null)
            {
                slot.audio.Stop();
                float size = Mathf.InverseLerp(.5f, 6f, foot.halfWidth);
                slot.audio.volume = audioProfile.Volume * Mathf.Lerp(.45f, 1f, impact * .6f + size * .4f);
                // Start each waterfall at a stable, different loop phase to avoid
                // nearby falls reinforcing the same recording in lockstep.
                slot.audio.time = Mathf.Repeat(footIndex * .6180339f, 1f)
                    * Mathf.Max(0f, slot.audio.clip.length - .01f);
                slot.audio.Play();
            }
        }

        private static void ReleaseSlot(Slot slot)
        {
            slot.renderer.enabled = false;
            if (slot.audio != null) slot.audio.Stop();
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
            var volumeObject = new GameObject($"Waterfall foot effects {slotIndex}");
            volumeObject.layer = gameObject.layer;
            volumeObject.transform.SetParent(transform, false);

            var fogObject = new GameObject("Volumetric mist");
            fogObject.layer = gameObject.layer;
            fogObject.transform.SetParent(volumeObject.transform, false);
            var filter = fogObject.AddComponent<MeshFilter>();
            filter.sharedMesh = sharedVolumeMesh;
            var renderer = fogObject.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = sharedMistMaterial;
            renderer.shadowCastingMode = ShadowCastingMode.Off;
            renderer.receiveShadows = true;
            renderer.lightProbeUsage = LightProbeUsage.Off;
            renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
            renderer.allowOcclusionWhenDynamic = false;
            renderer.enabled = false;

            AudioSource audio = null;
            if (audioProfile != null && audioProfile.Loop != null)
            {
                audio = volumeObject.AddComponent<AudioSource>();
                audio.playOnAwake = false;
                audio.clip = audioProfile.Loop;
                audio.loop = true;
                audio.spatialBlend = 1f;
                audio.dopplerLevel = 0f;
                audio.rolloffMode = AudioRolloffMode.Linear;
                audio.minDistance = audioProfile.MinimumDistance;
                audio.maxDistance = audioProfile.MaximumDistance;
                audio.priority = 160;
            }
            return new Slot(volumeObject, fogObject.transform, renderer, audio);
        }

        private void CreateSharedVisuals()
        {
            var mistShader = Motu.Rendering.MotuShaders.Find("Motu/Waterfall Foot Mist");
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
                if (DistanceToImpactSquared(foot, currentPlayerPosition) > retirementSquared)
                {
                    continue;
                }
                Gizmos.color = IsAssigned(footIndex)
                    ? Color.yellow
                    : new Color(1f, 0.45f, 0.9f, 0.75f);
                TryGetImpactPosition(foot, out var impactPosition);
                Gizmos.DrawSphere(impactPosition, IsAssigned(footIndex) ? 0.45f : 0.2f);
                Gizmos.DrawRay(impactPosition, foot.direction * Mathf.Max(foot.halfWidth, 1f));
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

        private void OnDisable()
        {
            ClearPlayerFocus();
        }
    }
}
