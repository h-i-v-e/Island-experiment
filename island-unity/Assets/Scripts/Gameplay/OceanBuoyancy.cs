using System;
using Motu.World;
using UnityEngine;

namespace Motu.Gameplay
{
    [AddComponentMenu("Motu/Ocean Buoyancy")]
    [RequireComponent(typeof(Rigidbody))]
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(1200)] // Sample after ocean animation and coastal mask composition.
    public sealed class OceanBuoyancy : MonoBehaviour
    {
        [Tooltip("Optional. When empty, finds the active world ocean at runtime.")]
        [SerializeField] private OceanSurfaceController ocean;
        [Tooltip("Probe positions in this object's local space. Place them around the submerged hull, below the centre of mass.")]
        [SerializeField] private Vector3[] probes =
        {
            new Vector3(-0.8f, -0.5f, -1.6f), new Vector3(0.8f, -0.5f, -1.6f),
            new Vector3(-0.8f, -0.5f, 0f), new Vector3(0.8f, -0.5f, 0f),
            new Vector3(-0.8f, -0.5f, 1.6f), new Vector3(0.8f, -0.5f, 1.6f),
        };
        [Tooltip("At rest, probes sit this many world metres below the water. Smaller values give stiffer buoyancy.")]
        [Min(0.01f)] [SerializeField] private float draft = 0.5f;
        [Tooltip("Total lift when fully submerged, as a multiple of the body's weight. Must exceed one to float.")]
        [Min(1.01f)] [SerializeField] private float maximumLift = 2f;
        [Tooltip("Vertical damping ratio. One is critical damping for vertical motion in still water.")]
        [Range(0f, 2f)] [SerializeField] private float damping = 0.7f;
        [Tooltip("Horizontal velocity damping per second, applied at each submerged probe.")]
        [Min(0f)] [SerializeField] private float waterDrag = 0.5f;
        [Tooltip("Smooth incoming water heights over this many seconds. Removes readback steps without delaying the Rigidbody position used by the springs. Zero disables smoothing.")]
        [Min(0f)] [SerializeField] private float surfaceSmoothingSeconds = 0.15f;

        private Rigidbody body;
        private OceanSurfaceSampler sampler;
        private Vector3[] worldPositions;
        private WaterSampleFilter[] waterSamples;
        private float nextOceanSearch;

        public OceanSurfaceController Ocean { get => ocean; set { ocean = value; ReleaseSampler(); } }
        public float Draft { get => draft; set => draft = Mathf.Max(0.01f, value); }
        public float SubmergedFraction { get; private set; }

        private void Awake() => body = GetComponent<Rigidbody>();

        private void LateUpdate()
        {
            if (body == null || body.isKinematic || probes == null || probes.Length == 0) return;
            if (ocean == null && Time.unscaledTime >= nextOceanSearch)
            {
                ocean = FindFirstObjectByType<OceanSurfaceController>();
                nextOceanSearch = Time.unscaledTime + 1f;
            }
            if (ocean == null || !ocean.isActiveAndEnabled || ocean.SurfaceMaterial == null) return;
            if (sampler == null || sampler.Count != probes.Length)
            {
                ReleaseSampler();
                try { sampler = new OceanSurfaceSampler(probes.Length); }
                catch (NotSupportedException error)
                {
                    Debug.LogError(error.Message, this);
                    enabled = false;
                    return;
                }
                worldPositions = new Vector3[probes.Length];
                waterSamples = new WaterSampleFilter[probes.Length];
            }
            for (var i = 0; i < probes.Length; i++) worldPositions[i] = PhysicsProbePosition(probes[i]);
            sampler.Submit(ocean, worldPositions);
        }

        private void FixedUpdate()
        {
            SubmergedFraction = 0f;
            if (body == null || body.isKinematic || sampler == null || probes == null
                || sampler.Count != probes.Length || ocean == null || !ocean.isActiveAndEnabled) return;
            var gravity = Mathf.Max(-Physics.gravity.y, 0f);
            for (var i = 0; i < probes.Length; i++)
            {
                var point = PhysicsProbePosition(probes[i]);
                if (!sampler.TryGetHeight(i, point, Mathf.Max(draft, 0.5f), out var height, out var confidence))
                {
                    // Teleports must not carry a smoothed height from the old location.
                    waterSamples[i] = default;
                    continue;
                }
                waterSamples[i].Update(height, confidence, Time.fixedDeltaTime, surfaceSmoothingSeconds);
                SubmergedFraction += Mathf.Clamp01((waterSamples[i].Height - point.y) / draft)
                    * waterSamples[i].Confidence / probes.Length;
                var acceleration = ProbeAcceleration(waterSamples[i].Height - point.y, body.GetPointVelocity(point),
                    gravity, draft, maximumLift, damping, waterDrag, Time.fixedDeltaTime);
                ApplyProbeAcceleration(body, acceleration * (waterSamples[i].Confidence / probes.Length), point);
            }
        }

        internal static void ApplyProbeAcceleration(Rigidbody rigidbody, Vector3 acceleration, Vector3 point)
        {
            // Convert the desired linear acceleration to a physical force. Using
            // Acceleration mode at an offset also bypasses rotational inertia:
            // long probe lever arms then turn damping into an unstable feedback.
            rigidbody.AddForceAtPosition(acceleration * rigidbody.mass, point, ForceMode.Force);
        }

        internal Vector3 PhysicsProbePosition(Vector3 localPoint)
        {
            // With interpolation, Transform contains the delayed render pose.
            // Using it in a spring introduces position feedback one step late.
            return body.position + body.rotation * Vector3.Scale(localPoint, transform.lossyScale);
        }

        internal struct WaterSampleFilter
        {
            private bool initialized;
            internal float Height { get; private set; }
            internal float Confidence { get; private set; }

            internal void Update(float height, float confidence, float deltaTime, float smoothingSeconds)
            {
                if (!initialized)
                {
                    Height = height;
                    initialized = true;
                }
                var blend = smoothingSeconds <= 0f ? 1f : 1f - Mathf.Exp(-deltaTime / smoothingSeconds);
                Height = Mathf.Lerp(Height, height, blend);
                // Falling confidence already fades continuously with sample age.
                // Smooth its recovery so a late readback cannot snap lift back on.
                Confidence = confidence < Confidence ? confidence : Mathf.Lerp(Confidence, confidence, blend);
            }
        }

        internal static Vector3 ProbeAcceleration(float depth, Vector3 velocity, float gravity,
            float draft, float maximumLift, float damping, float drag, float deltaTime)
        {
            if (depth <= 0f || gravity <= 0f) return Vector3.zero;
            var immersion = depth / Mathf.Max(draft, 0.01f);
            var wetness = Mathf.Clamp01(immersion);
            var spring = gravity / Mathf.Max(draft, 0.01f);
            var lift = gravity * Mathf.Min(immersion, maximumLift)
                - 2f * damping * Mathf.Sqrt(spring) * velocity.y * wetness;
            // Buoyancy cannot pull the hull down into the water or create an
            // unbounded impulse after a fast fall. Weight remains Rigidbody gravity.
            lift = Mathf.Clamp(lift, 0f, gravity * maximumLift);
            var dragRate = (1f - Mathf.Exp(-drag * wetness * deltaTime)) / Mathf.Max(deltaTime, 0.0001f);
            return new Vector3(-velocity.x * dragRate, lift, -velocity.z * dragRate);
        }

        [ContextMenu("Fit Probes To Hull Bounds")]
        public void FitProbesToHullBounds()
        {
            var renderers = GetComponentsInChildren<Renderer>();
            if (renderers.Length == 0) return;
            var bounds = renderers[0].bounds;
            for (var i = 1; i < renderers.Length; i++) bounds.Encapsulate(renderers[i].bounds);
            // Fit in world XZ so FBX models with a -90 degree root rotation work
            // too. This is an initial layout; exclude masts/spars when tuning it.
            probes = new Vector3[6];
            var longX = bounds.size.x > bounds.size.z;
            for (var row = 0; row < 3; row++)
            for (var side = 0; side < 2; side++)
            {
                var along = (row - 1) * 0.65f;
                var across = (side == 0 ? -1f : 1f) * 0.65f;
                var position = bounds.center + new Vector3(
                    bounds.extents.x * (longX ? along : across),
                    -bounds.extents.y * 0.85f,
                    bounds.extents.z * (longX ? across : along));
                probes[row * 2 + side] = transform.InverseTransformPoint(position);
            }
            draft = Mathf.Max(0.01f, bounds.size.y * 0.12f);
            ReleaseSampler();
        }

        private void Reset()
        {
            GetComponent<Rigidbody>().interpolation = RigidbodyInterpolation.Interpolate;
            FitProbesToHullBounds();
        }
        private void OnDisable() => ReleaseSampler();
        private void ReleaseSampler() { sampler?.Dispose(); sampler = null; }

        private void OnValidate()
        {
            draft = Mathf.Max(0.01f, draft);
            maximumLift = Mathf.Max(1.01f, maximumLift);
            if (probes != null && probes.Length > 64) Array.Resize(ref probes, 64);
        }

        private void OnDrawGizmosSelected()
        {
            if (probes == null) return;
            Gizmos.color = Color.cyan;
            foreach (var probe in probes)
            {
                var point = transform.TransformPoint(probe);
                Gizmos.DrawWireSphere(point, Mathf.Max(draft * 0.12f, 0.015f));
                Gizmos.DrawLine(point, point + Vector3.up * draft);
            }
        }
    }
}
