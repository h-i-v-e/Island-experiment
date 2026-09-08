using System.Collections.Generic;
using UnityEngine;

namespace Motu.Rendering
{
    /// <summary>Caps positive rendered wave displacement inside a moving deck footprint.</summary>
    [ExecuteAlways]
    [AddComponentMenu("Motu/Ocean Deck Wave Clamp")]
    public sealed partial class OceanDeckWaveClamp : MonoBehaviour
    {
        internal const int MaximumCapsules = 8;
        private static readonly List<OceanDeckWaveClamp> Active = new List<OceanDeckWaveClamp>();
        private static readonly Vector4[] Endpoints = new Vector4[MaximumCapsules];
        private static readonly Vector4[] Parameters = new Vector4[MaximumCapsules];
        private static readonly int CountId = Shader.PropertyToID("_MotuDeckCapsuleCount");
        private static readonly int EndpointsId = Shader.PropertyToID("_MotuDeckCapsuleEndpoints");
        private static readonly int ParametersId = Shader.PropertyToID("_MotuDeckCapsuleParameters");

        [Tooltip("Centres of the capsule's round ends, in this Transform's local space. Projected onto the horizontal sea plane.")]
        [SerializeField] private Vector3 start = new Vector3(0, 0, -8);
        [SerializeField] private Vector3 end = new Vector3(0, 0, 8);
        [Tooltip("Fully clamped radius in world metres, independent of imported model scale. Include a small margin around the deck for ocean mesh triangles.")]
        [Min(.01f), SerializeField] private float radiusMetres = 4;
        [Tooltip("Additional distance outside the capsule over which normal waves return, in world metres.")]
        [Min(.01f), SerializeField] private float blendMetres = 3;

        public void Configure(Vector3 localStart, Vector3 localEnd, float radius, float blend)
        {
            start = localStart;
            end = localEnd;
            radiusMetres = Mathf.Max(.01f, radius);
            blendMetres = Mathf.Max(.01f, blend);
            BindGlobals();
        }

        private void OnEnable()
        {
            body = GetComponentInParent<Rigidbody>();
            ResetTrail();
            if (Active.Count == 0) Camera.onPreCull += PrepareCamera;
            if (!Active.Contains(this)) Active.Add(this);
            if (Active.Count > MaximumCapsules)
                Debug.LogWarning($"Only {MaximumCapsules} active ocean deck capsules can be rendered at once.", this);
            BindGlobals();
        }

        private void OnDisable()
        {
            ResetTrail();
            Active.Remove(this);
            if (Active.Count == 0)
            {
                Camera.onPreCull -= PrepareCamera;
                OceanShipWaveField.Release();
            }
            BindGlobals();
        }

        private void OnValidate()
        {
            radiusMetres = Mathf.Max(.01f, radiusMetres);
            blendMetres = Mathf.Max(.01f, blendMetres);
            textureSizeMetres = new Vector2(Mathf.Max(1, textureSizeMetres.x), Mathf.Max(1, textureSizeMetres.y));
            fullWaveSpeed = Mathf.Max(.1f, fullWaveSpeed);
            wakeLifetime = Mathf.Max(.1f, wakeLifetime);
            wakeSpacing = Mathf.Max(.5f, wakeSpacing);
        }

        // Read the interpolated render pose immediately before each camera,
        // including Scene view and reflection cameras. Never affect GPU buoyancy queries.
        private static void PrepareCamera(Camera camera)
        {
            BindGlobals();
            OceanShipWaveField.Render(Active, camera.transform.position, Time.time);
        }

        internal static void BindGlobals()
        {
            var count = 0;
            foreach (var capsule in Active)
            {
                if (capsule == null || !capsule.isActiveAndEnabled) continue;
                if (capsule.hullTexture != null) continue;
                var a = capsule.transform.TransformPoint(capsule.start);
                var b = capsule.transform.TransformPoint(capsule.end);
                Endpoints[count] = new Vector4(a.x, a.z, b.x, b.z);
                Parameters[count] = new Vector4(capsule.radiusMetres, capsule.blendMetres, 0, 0);
                if (++count == MaximumCapsules) break;
            }
            Shader.SetGlobalVectorArray(EndpointsId, Endpoints);
            Shader.SetGlobalVectorArray(ParametersId, Parameters);
            Shader.SetGlobalInt(CountId, count);
        }

        private void OnDrawGizmosSelected()
        {
            if (hullTexture != null)
            {
                GetFootprint(out var centre, out var forward);
                Gizmos.color = Color.cyan;
                var previousMatrix = Gizmos.matrix;
                Gizmos.matrix = Matrix4x4.TRS(centre, Quaternion.LookRotation(forward), Vector3.one);
                Gizmos.DrawWireCube(Vector3.zero, new Vector3(textureSizeMetres.x, .05f, textureSizeMetres.y));
                Gizmos.matrix = previousMatrix;
                return;
            }
            var a = transform.TransformPoint(start);
            var b = transform.TransformPoint(end);
            b.y = a.y;
            DrawCapsule(a, b, radiusMetres, new Color(0, 1, 1, .9f));
            DrawCapsule(a, b, radiusMetres + blendMetres, new Color(0, 1, 1, .3f));
        }

        private static void DrawCapsule(Vector3 a, Vector3 b, float radius, Color colour)
        {
            Gizmos.color = colour;
            var direction = b - a;
            var angle = direction.sqrMagnitude > .0001f ? Mathf.Atan2(direction.z, direction.x) : 0;
            var previous = Vector3.zero;
            for (var i = 0; i <= 64; i++)
            {
                var theta = angle + Mathf.PI * 2 * i / 64;
                var offset = new Vector3(Mathf.Cos(theta), 0, Mathf.Sin(theta));
                var point = (Vector3.Dot(offset, direction) > 0 ? b : a) + offset * radius;
                if (i > 0) Gizmos.DrawLine(previous, point);
                previous = point;
            }
        }
    }
}
