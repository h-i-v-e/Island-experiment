using System;
using UnityEngine;

namespace Motu.Towns
{
    public enum TownDecorationAxis
    {
        Default,
        [InspectorName("+X")] PositiveX,
        [InspectorName("-X")] NegativeX,
        [InspectorName("+Y")] PositiveY,
        [InspectorName("-Y")] NegativeY,
        [InspectorName("+Z")] PositiveZ,
        [InspectorName("-Z")] NegativeZ,
    }

    [Serializable]
    public sealed class TownDecorationPrefabAxes
    {
        [Tooltip("A prefab from this binding's Prefabs list.")]
        public GameObject prefab;
        [Tooltip("The prefab's local axis facing away from the surface. Default uses the label's Facing Axis.")]
        public TownDecorationAxis facingAxis;
        [Tooltip("The prefab's local axis aligned with the anchor's forward arrow. Default uses the label's Forward Axis.")]
        public TownDecorationAxis forwardAxis;

        internal static Quaternion RotationOffset(TownDecorationAxis facingAxis, TownDecorationAxis forwardAxis)
        {
            var facing = LocalDirection(facingAxis, Vector3.up);
            var forward = LocalDirection(forwardAxis, Vector3.forward);
            if (Vector3.Dot(facing, forward) != 0f)
                forward = facing.z == 0f ? Vector3.forward : Vector3.up;
            // Invert the prefab's authored frame before applying the mount frame.
            return Quaternion.Inverse(Quaternion.LookRotation(forward, facing));
        }

        private static Vector3 LocalDirection(TownDecorationAxis axis, Vector3 fallback) => axis switch
        {
            TownDecorationAxis.PositiveX => Vector3.right,
            TownDecorationAxis.NegativeX => Vector3.left,
            TownDecorationAxis.PositiveY => Vector3.up,
            TownDecorationAxis.NegativeY => Vector3.down,
            TownDecorationAxis.PositiveZ => Vector3.forward,
            TownDecorationAxis.NegativeZ => Vector3.back,
            _ => fallback,
        };
    }

    [Serializable]
    public sealed class TownDecorationBinding
    {
        [Tooltip("Exact decoration label from Building Studio.")]
        public string label;
        [Tooltip("By default, prefab +Y faces away from the mounting surface and +Z follows the anchor's forward arrow.")]
        public GameObject[] prefabs = Array.Empty<GameObject>();
        [Tooltip("The local axis facing away from the surface on prefabs with this label. Default is +Y.")]
        public TownDecorationAxis facingAxis;
        [Tooltip("The local axis aligned with the anchor's forward arrow. Choose a perpendicular axis. Default is +Z, or +Y when facing along Z.")]
        public TownDecorationAxis forwardAxis;
        [Tooltip("Set different axes for individual prefabs in the list above.")]
        public TownDecorationPrefabAxes[] axisOverrides = Array.Empty<TownDecorationPrefabAxes>();

        internal Quaternion RotationOffsetFor(GameObject prefab)
        {
            if (axisOverrides != null)
                foreach (var axes in axisOverrides)
                {
                    if (axes == null || axes.prefab != prefab) continue;
                    return TownDecorationPrefabAxes.RotationOffset(
                        axes.facingAxis == TownDecorationAxis.Default ? facingAxis : axes.facingAxis,
                        axes.forwardAxis == TownDecorationAxis.Default ? forwardAxis : axes.forwardAxis);
                }
            return TownDecorationPrefabAxes.RotationOffset(facingAxis, forwardAxis);
        }
    }

    [CreateAssetMenu(menuName = "Motu/Town Decoration Catalogue", fileName = "TownDecorationCatalogue")]
    public sealed class TownDecorationCatalogue : ScriptableObject
    {
        [SerializeField] private TownDecorationBinding[] entries = Array.Empty<TownDecorationBinding>();

        internal int CountPrefabs(string label)
        {
            var count = 0;
            foreach (var entry in entries ?? Array.Empty<TownDecorationBinding>())
            {
                if (entry == null || entry.label != label || entry.prefabs == null) continue;
                foreach (var prefab in entry.prefabs)
                    if (prefab != null) count++;
            }
            return count;
        }

        internal (GameObject prefab, Quaternion rotationOffset) SelectPrefab(string label, uint variant)
        {
            var count = CountPrefabs(label);
            if (count == 0) return default;

            var index = (int)(variant % (uint)count);
            foreach (var entry in entries)
            {
                if (entry == null || entry.label != label || entry.prefabs == null) continue;
                foreach (var prefab in entry.prefabs)
                    if (prefab != null && index-- == 0) return (prefab, entry.RotationOffsetFor(prefab));
            }
            return default;
        }
    }
}
