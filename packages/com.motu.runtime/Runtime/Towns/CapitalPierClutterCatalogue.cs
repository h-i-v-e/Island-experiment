using System;
using System.Collections.Generic;
using UnityEngine;

namespace Motu.Towns
{
    [CreateAssetMenu(menuName = "Motu/Capital Pier Clutter Catalogue", fileName = "CapitalPierClutterCatalogue")]
    public sealed class CapitalPierClutterCatalogue : ScriptableObject
    {
        internal const string ResourcePath = "Towns/CapitalPierClutterCatalogue";

        [Serializable] private sealed class Entry
        {
            public string kind = "cargo";
            public GameObject prefab;
        }

        [SerializeField] private Entry[] entries = Array.Empty<Entry>();

        internal IReadOnlyList<GameObject> MatchingPrefabs(string kind)
        {
            var prefabs = new List<GameObject>();
            foreach (var entry in entries)
            {
                if (entry?.prefab != null && string.Equals(entry.kind, kind, StringComparison.Ordinal)
                    && FitsCargoSlot(entry.prefab)) prefabs.Add(entry.prefab);
            }
            return prefabs;
        }

        private static bool FitsCargoSlot(GameObject prefab)
        {
            // Native anchors put the walkway 0.52 m inboard and the edge 0.62 m outboard.
            // Adjacent anchors are 2.25 m apart along the deck.
            var prefabLocal = Matrix4x4.Scale(prefab.transform.localScale) * prefab.transform.worldToLocalMatrix;
            foreach (var renderer in prefab.GetComponentsInChildren<Renderer>(true))
            {
                var bounds = renderer.localBounds;
                var inPrefab = prefabLocal * renderer.transform.localToWorldMatrix;
                for (var i = 0; i < 8; i++)
                {
                    var corner = bounds.center + Vector3.Scale(bounds.extents,
                        new Vector3((i & 1) == 0 ? -1f : 1f, (i & 2) == 0 ? -1f : 1f, (i & 4) == 0 ? -1f : 1f));
                    var position = inPrefab.MultiplyPoint3x4(corner);
                    if (Mathf.Abs(position.x) > .48f || Mathf.Abs(position.z) > 1f) return false;
                }
            }
            return true;
        }
    }
}
