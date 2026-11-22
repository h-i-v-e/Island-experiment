using System;
using System.Collections.Generic;
using Motu.Islands;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Towns
{
    [Serializable]
    public struct TownDecorationAnchor
    {
        [SerializeField] private string id, label;
        [SerializeField] private Vector3 localPosition;
        [SerializeField] private Quaternion localRotation;

        public string Id => id;
        public string Label => label;
        public Vector3 LocalPosition => localPosition;
        public Quaternion LocalRotation => localRotation;

        internal void Mount(Transform prop, Transform parent, Vector3 scale, Quaternion rotationOffset)
        {
            prop.SetParent(parent, false);
            prop.SetLocalPositionAndRotation(LocalPosition, LocalRotation * rotationOffset);
            prop.localScale = scale;
        }

        internal TownDecorationAnchor(TownDecorationAnchorData source)
        {
            id = source.id;
            label = source.label;
            localPosition = ImportVector(source.position);
            var normal = ImportVector(source.normal);
            var forward = ImportVector(source.forward);
            if (Vector3.Cross(normal, forward).sqrMagnitude < 0.000001f)
                throw new InvalidOperationException("The native building decoration anchor has an invalid orientation: " + id);
            localRotation = Quaternion.LookRotation(forward, normal);
        }

        private static Vector3 ImportVector(float[] values)
        {
            if (values?.Length != 3 || !float.IsFinite(values[0]) || !float.IsFinite(values[1]) || !float.IsFinite(values[2]))
                throw new InvalidOperationException("The native building decoration anchor has an invalid vector.");
            return TownRenderer.Vector(values);
        }
    }

    [ExecuteAlways, AddComponentMenu(""), DisallowMultipleComponent, RequireComponent(typeof(LODGroup))]
    public sealed class TownDecorationAnchors : MonoBehaviour
    {
        [SerializeField, HideInInspector] private TownDecorationAnchor[] anchors = Array.Empty<TownDecorationAnchor>();
        [SerializeField] private TownDecorationCatalogue catalogue;
        [SerializeField, HideInInspector] private uint prefabSeed;

        private readonly List<(GameObject prefab, GameObject instance)> props = new();
        private TownDecorationPool pool;
        private uint? placedSeed;
        private bool budgeted;
        private bool refreshBudgetProps;
        private int budgetLevel = TownBuildingLod.PaintedShell;
        private LODGroup group;
        private LOD[] lods;
        private int cameraLevel = -1;

        public IReadOnlyList<TownDecorationAnchor> Anchors => anchors;

        internal void Import(TownDecorationAnchorData[] data)
        {
            anchors = new TownDecorationAnchor[data.Length];
            for (var index = 0; index < data.Length; index++)
                anchors[index] = new TownDecorationAnchor(data[index]);
        }

        internal void ConfigurePlaced(TownDecorationCatalogue value, string seed, string entry, Vector3 position, bool useBudget)
        {
            catalogue = value;
            budgeted = useBudget;
            placedSeed = AddPosition(BuildingSeed(seed, entry), position);
        }

        internal void ConfigureBaked(TownDecorationCatalogue value, string seed, string entry)
        {
            catalogue = value;
            prefabSeed = BuildingSeed(seed, entry);
        }

        internal void SetBudgetLevel(int level)
        {
            budgetLevel = level;
            refreshBudgetProps = false;
            ApplyProps(level == 0);
        }

        private void OnEnable()
        {
            refreshBudgetProps = budgeted && budgetLevel == 0;
        }

        private void LateUpdate()
        {
            if (refreshBudgetProps)
            {
                // Unity finishes activating the building before props are reparented to it.
                refreshBudgetProps = false;
                ApplyProps(closest: true);
            }
            else if (!budgeted && catalogue != null && Application.IsPlaying(gameObject))
                UpdateCameraLevel(Camera.main);
        }

        private void OnDisable()
        {
            // Unity blocks reparenting children while a GameObject hierarchy is deactivating.
            ReleaseProps(reuse: false);
            pool?.Release();
            pool = null;
            lods = null;
            if (cameraLevel == -1) return;
            if (group != null) group.ForceLOD(-1);
            cameraLevel = -1;
        }

        internal void UpdateCameraLevel(Camera camera)
        {
            // Standalone baked prefabs use the same level for the shell and the props.
            // Coastal instances use the budget instead of looking at a second viewer.
            if (budgeted || catalogue == null || !isActiveAndEnabled) return;
            if (group == null) group = GetComponent<LODGroup>();
            var count = group.lodCount;
            if (lods?.Length != count) lods = group.GetLODs();
            var level = camera != null && group.enabled && count != 0 ? CameraLevel(group, lods, camera) : -1;
            if (level == cameraLevel) return;
            cameraLevel = level;
            group.ForceLOD(level);
            ApplyProps(level == 0);
        }

        private static int CameraLevel(LODGroup group, LOD[] lods, Camera camera)
        {
            var scale = group.transform.lossyScale;
            var worldSize = group.size * Mathf.Max(Mathf.Abs(scale.x), Mathf.Abs(scale.y), Mathf.Abs(scale.z));
            var viewHeight = camera.orthographic
                ? 2f * camera.orthographicSize
                : 2f * Vector3.Distance(group.transform.TransformPoint(group.localReferencePoint), camera.transform.position)
                    * Mathf.Tan(0.5f * camera.fieldOfView * Mathf.Deg2Rad);
            var height = worldSize * QualitySettings.lodBias / Mathf.Max(viewHeight, Mathf.Epsilon);
            var firstLevel = Mathf.Clamp(QualitySettings.maximumLODLevel, 0, lods.Length - 1);
            for (var index = firstLevel; index < lods.Length; index++)
                if (height >= lods[index].screenRelativeTransitionHeight) return index;
            return -1;
        }

        private void ApplyProps(bool closest)
        {
            if (!closest)
            {
                ReleaseProps(reuse: true);
                return;
            }
            if (!isActiveAndEnabled || catalogue == null || props.Count != 0) return;

            var identity = placedSeed ??= AddPosition(prefabSeed, transform.position);
            foreach (var anchor in anchors)
            {
                if (!isActiveAndEnabled) break;
                var (prefab, rotationOffset) = catalogue.SelectPrefab(anchor.Label, IslandRandom.Mix(AddText(identity, anchor.Id)));
                if (prefab == null) continue;
                pool ??= TownDecorationPool.Acquire(gameObject.scene);
                var instance = pool.Rent(prefab);
                anchor.Mount(instance.transform, transform, prefab.transform.localScale, rotationOffset);
                props.Add((prefab, instance));
                instance.SetActive(true);
            }
        }

        private void ReleaseProps(bool reuse)
        {
            while (props.Count != 0)
            {
                var index = props.Count - 1;
                var (prefab, instance) = props[index];
                props.RemoveAt(index);
                if (reuse)
                {
                    pool.Return(prefab, instance);
                }
                else if (instance != null)
                {
                    instance.SetActive(false);
                    DestroyUnityObject(instance);
                }
            }
        }

        private static uint BuildingSeed(string seed, string entry) => AddText(AddText(2166136261u, seed), entry);

        private static uint AddPosition(uint hash, Vector3 position)
        {
            hash = Add(hash, unchecked((uint)BitConverter.SingleToInt32Bits(position.x)));
            hash = Add(hash, unchecked((uint)BitConverter.SingleToInt32Bits(position.y)));
            return Add(hash, unchecked((uint)BitConverter.SingleToInt32Bits(position.z)));
        }

        private static uint AddText(uint hash, string text)
        {
            if (text == null) return Add(hash, 0);
            hash = Add(hash, (uint)text.Length);
            foreach (var character in text) hash = Add(hash, character);
            return hash;
        }

        private static uint Add(uint hash, uint value) => unchecked((hash ^ value) * 16777619u);
    }
}
