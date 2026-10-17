using System;
using UnityEngine;

namespace Motu.Rendering
{
    /// <summary>Closed convex render-only hull, in the clamp Transform's local space.</summary>
    [CreateAssetMenu(menuName = "Motu/Ocean Hull Volume")]
    public sealed class OceanHullVolume : ScriptableObject
    {
        public const int MaximumPlanes = 128;
        [SerializeField] private Vector4[] planes = Array.Empty<Vector4>();
        [SerializeField] private Mesh boundary;
        public Vector4[] Planes => planes;
        public Mesh Boundary => boundary;
        public bool IsValid => boundary != null && planes != null && planes.Length >= 4 && planes.Length <= MaximumPlanes;

        public void SetGeometry(Vector4[] facePlanes, Mesh mesh)
        {
            if (facePlanes == null || facePlanes.Length < 4 || facePlanes.Length > MaximumPlanes || mesh == null)
                throw new ArgumentException("A closed hull needs 4–128 outward planes and a boundary mesh.");
            planes = facePlanes;
            boundary = mesh;
        }
    }
}
