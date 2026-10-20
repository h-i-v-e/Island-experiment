using System;
using System.Collections.Generic;
using UnityEngine;

namespace Motu.Islands
{
    public enum LandBirdPerchKind { Tree, Cliff }

    public readonly struct LandBirdPerch
    {
        public Vector3 LocalPosition { get; }
        public float YawDegrees { get; }
        public LandBirdPerchKind Kind { get; }
        public int Tile { get; }
        public LandBirdPerch(Vector3 localPosition, float yawDegrees, LandBirdPerchKind kind, int tile)
        { LocalPosition = localPosition; YawDegrees = yawDegrees; Kind = kind; Tile = tile; }
    }

    /// <summary>Small managed anchor array, available only while its detailed island is resident.
    /// Does not require individual terrain tiles, colliders or wildlife GameObjects to stay loaded.</summary>
    [DisallowMultipleComponent]
    public sealed class IslandBirdPerches : MonoBehaviour
    {
        private static readonly List<IslandBirdPerches> loaded = new List<IslandBirdPerches>();
        private LandBirdPerch[] perches = Array.Empty<LandBirdPerch>();
        public static IReadOnlyList<IslandBirdPerches> Loaded => loaded;
        public IReadOnlyList<LandBirdPerch> Perches => perches;
        private float cruiseHeight;
        public float CruiseHeight => transform.TransformPoint(new Vector3(0, cruiseHeight, 0)).y;
        public bool TreesVisible { get; set; } = true;
        public void Initialize(LandBirdPerch[] anchors, bool treesVisible = true, float terrainMaximumHeight = 0)
        {
            perches = anchors ?? Array.Empty<LandBirdPerch>(); TreesVisible = treesVisible;
            cruiseHeight = terrainMaximumHeight;
            foreach (var perch in perches) cruiseHeight = Mathf.Max(cruiseHeight, perch.LocalPosition.y);
            cruiseHeight += 12;
        }
        public bool CanUse(int index) => isActiveAndEnabled && index >= 0 && index < perches.Length
            && (TreesVisible || perches[index].Kind != LandBirdPerchKind.Tree);
        public Vector3 Position(int index) => transform.TransformPoint(perches[index].LocalPosition);
        public Quaternion Rotation(int index) => transform.rotation * Quaternion.Euler(0, perches[index].YawDegrees, 0);
        private void OnEnable() { if (!loaded.Contains(this)) loaded.Add(this); }
        private void OnDisable() => loaded.Remove(this);
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        private static void ResetRegistry() => loaded.Clear();
        private void OnDrawGizmosSelected()
        {
            foreach (var anchor in perches)
            {
                Gizmos.color = anchor.Kind == LandBirdPerchKind.Tree ? Color.green : Color.cyan;
                var position = transform.TransformPoint(anchor.LocalPosition);
                Gizmos.DrawWireSphere(position, .4f);
                Gizmos.DrawRay(position, Vector3.up * 2);
            }
        }
    }
}
