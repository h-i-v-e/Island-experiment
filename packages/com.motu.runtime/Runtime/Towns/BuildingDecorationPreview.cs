using System;
using System.Collections.Generic;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Towns
{
    [ExecuteAlways, DisallowMultipleComponent]
    public sealed class BuildingDecorationPreview : MonoBehaviour
    {
        [SerializeField] private TextAsset buildingRecipe;
        [SerializeField] private TownDecorationCatalogue decorationCatalogue;
        [SerializeField, Tooltip("Optional material bindings for recipes that reference external material JSON files.")]
        private TownRecipeCatalogue materialCatalogue;

        private TownRenderer owner;
        private GameObject building;
        private GameObject props;

        internal TextAsset Recipe { get => buildingRecipe; set => buildingRecipe = value; }
        internal TownDecorationCatalogue Catalogue { get => decorationCatalogue; set => decorationCatalogue = value; }
        internal TownRecipeCatalogue MaterialCatalogue { get => materialCatalogue; set => materialCatalogue = value; }
        internal bool HasBuilding => building != null;
        internal IReadOnlyList<TownDecorationAnchor> Anchors =>
            building != null && building.TryGetComponent<TownDecorationAnchors>(out var mounts)
                ? mounts.Anchors : Array.Empty<TownDecorationAnchor>();

        internal void SetBuilding(TownPrototype prototype)
        {
            var next = TownRenderer.BuildDecorationPreview(transform, prototype);
            Clear();
            (owner, building) = next;
        }

        internal int ShowDecorations(int selected, uint variant, GameObject trialPrefab, Quaternion? trialAxes)
        {
            if (props != null) DestroyUnityObject(props);
            props = null;
            var anchors = Anchors;
            int mounted = 0;
            for (var index = 0; index < anchors.Count; index++)
            {
                var anchor = anchors[index];
                var (prefab, rotation) = decorationCatalogue != null
                    ? decorationCatalogue.SelectPrefab(anchor.Label, index == selected ? variant : 0)
                    : (null, Quaternion.identity);
                if (index == selected && trialPrefab != null)
                {
                    prefab = trialPrefab;
                    rotation = Quaternion.identity;
                }
                if (prefab == null) continue;
                if (props == null)
                {
                    props = new GameObject("Decoration previews") { hideFlags = HideFlags.HideAndDontSave };
                    props.SetActive(false);
                    props.transform.SetParent(building.transform, false);
                }
                var instance = Instantiate(prefab, props.transform, false);
                foreach (var child in instance.GetComponentsInChildren<Transform>(true))
                    child.gameObject.hideFlags = HideFlags.DontSave;
                anchor.Mount(instance.transform, props.transform, prefab.transform.localScale,
                    index == selected ? trialAxes ?? rotation : rotation);
                instance.SetActive(true);
                mounted++;
            }
            if (props != null) props.SetActive(true);
            return mounted;
        }

        internal Bounds BuildingBounds()
        {
            var bounds = new Bounds(transform.position, Vector3.one);
            if (building == null) return bounds;
            var renderers = building.GetComponent<LODGroup>().GetLODs()[0].renderers;
            if (renderers.Length == 0) return bounds;
            bounds = renderers[0].bounds;
            for (var index = 1; index < renderers.Length; index++) bounds.Encapsulate(renderers[index].bounds);
            return bounds;
        }

        internal Pose AnchorPose(int index)
        {
            var anchor = Anchors[index];
            var root = building.transform;
            return new Pose(root.TransformPoint(anchor.LocalPosition), root.rotation * anchor.LocalRotation);
        }

        internal void Clear()
        {
            if (owner != null) owner.DestroyDecorationPreview();
            owner = null;
            building = null;
            props = null;
        }

        private void OnDisable() => Clear();
    }
}
