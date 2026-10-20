using UnityEngine;

namespace Motu.Rendering
{
    public sealed partial class OceanDeckWaveClamp
    {
        private const int MaximumHulls = 4;
        [Header("Convex hull exclusion")]
        [Tooltip("Baked in this Transform's local space. Excludes interior water fragments and clamps only crests above the mean sea plane; replaces footprint suppression; bow waves and wake are retained. Clear to restore the footprint.")]
        [SerializeField] private OceanHullVolume convexHull;
        public OceanHullVolume ConvexHull => convexHull;
        internal bool HasConvexHull => convexHull != null && convexHull.IsValid;

        private static readonly Vector4[] HullPlanes = new Vector4[MaximumHulls * OceanHullVolume.MaximumPlanes];
        private static readonly Vector4[] HullMinimum = new Vector4[MaximumHulls];
        private static readonly Vector4[] HullMaximum = new Vector4[MaximumHulls];
        private static readonly int HullCountId = Shader.PropertyToID("_MotuHullCount");
        private static readonly int HullPlanesId = Shader.PropertyToID("_MotuHullPlanes");
        private static readonly int HullMinimumId = Shader.PropertyToID("_MotuHullMinimum");
        private static readonly int HullMaximumId = Shader.PropertyToID("_MotuHullMaximum");

        public void ConfigureConvexHull(OceanHullVolume volume)
        {
            convexHull = volume;
            BindGlobals();
        }

        private static void BindHullGlobals()
        {
            int count = 0;
            foreach (var ship in Active)
            {
                if (ship == null || !ship.isActiveAndEnabled || !ship.HasConvexHull) continue;
                if (count == MaximumHulls) break;
                var matrix = ship.transform.localToWorldMatrix;
                var planeMatrix = ship.transform.worldToLocalMatrix.transpose;
                var source = ship.convexHull.Planes;
                for (int i = 0; i < source.Length; i++)
                {
                    var p = planeMatrix * source[i];
                    float length = new Vector3(p.x, p.y, p.z).magnitude;
                    HullPlanes[count * OceanHullVolume.MaximumPlanes + i] = p / Mathf.Max(length, .000001f);
                }
                var bounds = ship.convexHull.Boundary.bounds;
                var centre = matrix.MultiplyPoint3x4(bounds.center);
                var x = matrix.MultiplyVector(Vector3.right * bounds.extents.x);
                var y = matrix.MultiplyVector(Vector3.up * bounds.extents.y);
                var z = matrix.MultiplyVector(Vector3.forward * bounds.extents.z);
                var extent = new Vector3(Mathf.Abs(x.x) + Mathf.Abs(y.x) + Mathf.Abs(z.x),
                    Mathf.Abs(x.y) + Mathf.Abs(y.y) + Mathf.Abs(z.y), Mathf.Abs(x.z) + Mathf.Abs(y.z) + Mathf.Abs(z.z));
                var minimum = centre - extent;
                var maximum = centre + extent;
                HullMinimum[count] = new Vector4(minimum.x, minimum.y, minimum.z, source.Length);
                HullMaximum[count] = new Vector4(maximum.x, maximum.y, maximum.z, 0);
                count++;
            }
            Shader.SetGlobalVectorArray(HullPlanesId, HullPlanes);
            Shader.SetGlobalVectorArray(HullMinimumId, HullMinimum);
            Shader.SetGlobalVectorArray(HullMaximumId, HullMaximum);
            Shader.SetGlobalInt(HullCountId, count);
        }
    }
}
