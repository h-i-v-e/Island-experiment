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
        [Header("Water sampling and draft")]
        [Tooltip("Water sampling positions in body-local space. With a hull profile these measure waves; without one they also act as buoyancy springs.")]
        [SerializeField] private Vector3[] probes =
        {
            new Vector3(-0.8f, -0.5f, -1.6f), new Vector3(0.8f, -0.5f, -1.6f),
            new Vector3(-0.8f, -0.5f, 0f), new Vector3(0.8f, -0.5f, 0f),
            new Vector3(-0.8f, -0.5f, 1.6f), new Vector3(0.8f, -0.5f, 1.6f),
        };
        [Tooltip("Without a hull profile: probe spring rest depth. With Hull Draft Loading attached: desired upright draft in world metres, measured up from the hull's lowest point; changing this adjusts loading mass. Otherwise hull flotation is determined by Rigidbody mass and this value only sets sampling tolerance.")]
        [Min(0.01f)] [SerializeField] private float draft = 0.5f;
        [Tooltip("Total lift when fully submerged, as a multiple of the body's weight. Must exceed one to float.")]
        [Min(1.01f)] [SerializeField] private float maximumLift = 2f;
        [Tooltip("Vertical damping ratio. One is critical damping for vertical motion in still water.")]
        [Range(0f, 2f)] [SerializeField] private float damping = 0.7f;
        [Tooltip("Simple probe flotation only: horizontal damping per second. Hull profiles use the geometry-based resistance settings below.")]
        [Min(0f)] [SerializeField] private float waterDrag = 0.5f;
        [Tooltip("Smooth incoming water heights over this many seconds. Removes readback steps without delaying the Rigidbody position used by the springs. Zero disables smoothing.")]
        [Min(0f)] [SerializeField] private float surfaceSmoothingSeconds = 0.15f;

        [Header("Mesh hull hydrodynamics")]
        [Tooltip("Baked hull sections. When assigned, displaced volume replaces the probe springs and this component owns all hull water resistance.")]
        [SerializeField] private ShipHullGeometry hullGeometry;
        [Min(1f)] [SerializeField] private float waterDensity = 1025f;
        [Min(.0000001f)] [SerializeField] private float waterViscosity = .00000119f;
        [Tooltip("Hull pressure resistance referenced to its largest submerged transverse section.")]
        [Min(0f)] [SerializeField] private float hullPressureDrag = .08f;
        [Tooltip("Cross-flow pressure coefficient for sideways motion through the submerged sides and vertical motion through the waterplane.")]
        [Min(0f)] [SerializeField] private float lateralDrag = 1.1f;
        [Tooltip("Effective hull/keel lift slope per radian. Forward water flow increases lateral resistance at submerged depths, damping roll and leeway. Empirical: tune to the hull; zero disables the forward-speed contribution.")]
        [Min(0f)] [SerializeField] private float keelLiftSlope = 2f;
        [Tooltip("Empirical wave-making resistance at Froude number 0.4, referenced to the submerged cross section. Requires hull-specific calibration.")]
        [Min(0f)] [SerializeField] private float waveResistance = .04f;
        [Tooltip("Water current in world metres per second, pointing towards its destination.")]
        [SerializeField] private Vector3 waterCurrent;

        private Vector2[] clippedHull;
        private ShipHullGeometry.ImmersedSection[] immersedSections;
        private float[] sectionConfidence;
        private Rigidbody body;
        private OceanSurfaceSampler sampler;
        private Vector3[] worldPositions;
        private WaterSampleFilter[] waterSamples;
        private float nextOceanSearch;
        // A cold shader or a large mesh upload can delay GPU readback across
        // several physics steps. Keep the last local water height long enough
        // for rendering to recover instead of removing all lift mid-stall.
        private const float SampleHoldSeconds = 2f;
        private const float SampleFadeSeconds = 4f;

        public OceanSurfaceController Ocean { get => ocean; set { ocean = value; ReleaseSampler(); } }
        public float Draft { get => draft; set => draft = Mathf.Max(0.01f, value); }
        public float SubmergedFraction { get; private set; }

        public ShipHullGeometry HullGeometry => hullGeometry;
        public float WaterDensity => waterDensity;
        public Vector3 WaterCurrent { get => waterCurrent; set => waterCurrent = value; }
        public float DisplacedVolume { get; private set; }
        public float WettedSurfaceArea { get; private set; }
        public float SubmergedCrossSection { get; private set; }
        public float WaterlineLength { get; private set; }
        public Vector3 CentreOfBuoyancy { get; private set; }

        // Set by the ship controller; standalone floating bodies retain physical speed.
        private float forwardSpeedMultiplier = 1f;
        public float ForwardSpeedMultiplier
        {
            get => forwardSpeedMultiplier;
            set => forwardSpeedMultiplier = Mathf.Max(1f, value);
        }

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
                try { sampler = new OceanSurfaceSampler(probes.Length, includeVelocity: hullGeometry != null); }
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
                if (!sampler.TryGetHeight(i, point, Mathf.Max(draft, 0.5f), out var height,
                        out var confidence, SampleHoldSeconds, SampleFadeSeconds))
                {
                    // Teleports must not carry a smoothed height from the old location.
                    waterSamples[i] = default;
                    continue;
                }
                waterSamples[i].Update(height, confidence, Time.fixedDeltaTime, surfaceSmoothingSeconds);
                if (hullGeometry != null)
                {
                    Vector3 velocity = sampler.TryGetSurface(i, point, Mathf.Max(draft, .5f), out _, out var sampledVelocity)
                        ? sampledVelocity : Vector3.zero;
                    float blend = surfaceSmoothingSeconds > 0f ? 1f - Mathf.Exp(-Time.fixedDeltaTime / surfaceSmoothingSeconds) : 1f;
                    waterSamples[i].Velocity = Vector3.Lerp(waterSamples[i].Velocity, velocity, blend);
                }
                SubmergedFraction += Mathf.Clamp01((waterSamples[i].Height - point.y) / draft)
                    * waterSamples[i].Confidence / probes.Length;
                if (hullGeometry == null)
                {
                    var acceleration = ProbeAcceleration(waterSamples[i].Height - point.y, body.GetPointVelocity(point),
                        gravity, draft, maximumLift, damping, waterDrag, Time.fixedDeltaTime);
                    ApplyProbeAcceleration(body, acceleration * (waterSamples[i].Confidence / probes.Length), point);
                }
            }
            if (hullGeometry != null) ApplyHullForces(gravity);
        }

        // Fit a local water plane to the existing probes. Keeping its cross-beam
        // slope lets asymmetric immersion excite roll, including when stopped.
        public bool TryWaterHeight(Vector3 position, out float height, out float confidence)
            => TryWaterPlane(position, out height, out _, out confidence);

        internal bool TryWaterPlane(Vector3 position, out float height, out Vector3 normal, out float confidence)
        {
            height = confidence = 0f;
            normal = Vector3.up;
            if (waterSamples == null || probes == null) return false;
            double totalWeight = 0, validWeight = 0, meanX = 0, meanZ = 0, meanHeight = 0;
            for (int i = 0; i < probes.Length; i++)
            {
                Vector3 delta = PhysicsProbePosition(probes[i]) - position;
                delta.y = 0f;
                float weight = 1f / Mathf.Max(delta.sqrMagnitude, 1f);
                totalWeight += weight;
                float valid = weight * waterSamples[i].Confidence;
                meanX += valid * delta.x;
                meanZ += valid * delta.z;
                meanHeight += valid * waterSamples[i].Height;
                validWeight += valid;
            }
            if (validWeight <= .00001f) return false;
            meanX /= validWeight; meanZ /= validWeight; meanHeight /= validWeight;
            double xx = 0, xz = 0, zz = 0, xh = 0, zh = 0;
            for (int i = 0; i < probes.Length; i++)
            {
                Vector3 delta = PhysicsProbePosition(probes[i]) - position;
                delta.y = 0f;
                double weight = waterSamples[i].Confidence / Mathf.Max(delta.sqrMagnitude, 1f);
                double x = delta.x - meanX, z = delta.z - meanZ;
                double h = waterSamples[i].Height - meanHeight;
                xx += weight * x * x; xz += weight * x * z; zz += weight * z * z;
                xh += weight * x * h; zh += weight * z * h;
            }
            double determinant = xx * zz - xz * xz;
            double slopeX = 0, slopeZ = 0;
            if (determinant > 1e-6 * xx * zz && determinant > 1e-12)
            {
                slopeX = (xh * zz - zh * xz) / determinant;
                slopeZ = (zh * xx - xh * xz) / determinant;
            }
            else if (xx + zz > 1e-12)
            {
                // Collinear surviving probes constrain only one slope direction.
                slopeX = xh / (xx + zz); slopeZ = zh / (xx + zz);
            }
            height = (float)(meanHeight - slopeX * meanX - slopeZ * meanZ);
            normal = new Vector3(-(float)slopeX, 1f, -(float)slopeZ).normalized;
            confidence = (float)(validWeight / totalWeight);
            return true;
        }

        public Vector3 WaterVelocityAt(Vector3 position)
        {
            if (waterSamples == null || probes == null) return waterCurrent;
            Vector3 velocity = Vector3.zero;
            float weights = 0f;
            for (int i = 0; i < probes.Length; i++)
            {
                Vector3 delta = PhysicsProbePosition(probes[i]) - position;
                delta.y = 0f;
                float weight = waterSamples[i].Confidence / Mathf.Max(delta.sqrMagnitude, 1f);
                velocity += waterSamples[i].Velocity * weight;
                weights += weight;
            }
            return waterCurrent + (weights > .00001f ? velocity / weights : Vector3.zero);
        }

        private void ApplyHullForces(float gravity)
        {
            var sections = hullGeometry.Sections;
            if (immersedSections == null || immersedSections.Length != sections.Length)
            {
                immersedSections = new ShipHullGeometry.ImmersedSection[sections.Length];
                sectionConfidence = new float[sections.Length];
                int largest = 0;
                foreach (var section in sections) largest = Mathf.Max(largest, section.outline.Length);
                clippedHull = new Vector2[largest + 2];
            }
            Vector3 scale = transform.lossyScale;
            Quaternion worldToBody = Quaternion.Inverse(body.rotation);
            DisplacedVolume = WettedSurfaceArea = SubmergedCrossSection = WaterlineLength = 0f;
            Vector3 weightedCentre = Vector3.zero;
            float firstStation = float.MaxValue, lastStation = float.MinValue;
            for (int i = 0; i < sections.Length; i++)
            {
                var section = sections[i];
                Vector3 station = body.position + body.rotation * Vector3.Scale(new Vector3(0, 0, section.station), scale);
                if (!TryWaterPlane(station, out float height, out var waterNormal, out float confidence))
                { immersedSections[i] = default; sectionConfidence[i] = 0f; continue; }
                Vector3 waterPoint = new Vector3(station.x, height, station.z);
                var wet = ShipHullGeometry.Immerse(section, scale, worldToBody * waterNormal,
                    Vector3.Dot(waterNormal, waterPoint - body.position), clippedHull);
                immersedSections[i] = wet;
                sectionConfidence[i] = confidence;
                if (wet.area <= .00001f) continue;
                float length = section.length * Mathf.Abs(scale.z);
                float volume = wet.area * length * confidence;
                DisplacedVolume += volume;
                WettedSurfaceArea += wet.wettedPerimeter * length * confidence;
                SubmergedCrossSection = Mathf.Max(SubmergedCrossSection, wet.area * confidence);
                Vector3 centre = body.position + body.rotation * wet.centre;
                weightedCentre += centre * volume;
                firstStation = Mathf.Min(firstStation, section.station * scale.z - length * .5f);
                lastStation = Mathf.Max(lastStation, section.station * scale.z + length * .5f);
            }
            SubmergedFraction = Mathf.Clamp01(DisplacedVolume * waterDensity / body.mass);
            if (DisplacedVolume <= .00001f) return;
            CentreOfBuoyancy = weightedCentre / DisplacedVolume;
            WaterlineLength = Mathf.Max(lastStation - firstStation, .01f);
            Vector3 forward = Vector3.ProjectOnPlane(body.rotation * Vector3.forward, Vector3.up).normalized;
            Vector3 right = Vector3.Cross(Vector3.up, forward);
            for (int i = 0; i < sections.Length; i++)
            {
                var wet = immersedSections[i];
                if (wet.area <= .00001f) continue;
                float length = sections[i].length * Mathf.Abs(scale.z);
                float confidence = sectionConfidence[i];
                float volume = wet.area * length * confidence;
                Vector3 centre = body.position + body.rotation * wet.centre;
                Vector3 velocity = body.GetPointVelocity(centre) - WaterVelocityAt(centre);
                float sectionMass = body.mass * volume / DisplacedVolume;
                float stiffness = waterDensity * gravity * wet.waterlineWidth * length;
                float dampingForce = -2f * damping * Mathf.Sqrt(stiffness * sectionMass) * velocity.y;
                // Vertical cross-flow acts over each section's waterplane area.
                // Bow/stern motion then produces pitch resistance through the hull's own lever arms.
                dampingForce -= .5f * waterDensity * lateralDrag * wet.waterlineWidth * length * confidence
                    * velocity.y * Mathf.Abs(velocity.y);
                // A dissipative step may stop motion but must not reverse it in a single tick.
                dampingForce = Mathf.Clamp(dampingForce, -Mathf.Abs(velocity.y) * sectionMass / Time.fixedDeltaTime,
                    Mathf.Abs(velocity.y) * sectionMass / Time.fixedDeltaTime);
                body.AddForceAtPosition(Vector3.up * (waterDensity * gravity * volume + dampingForce), centre, ForceMode.Force);
                // Two Gauss points integrate the depth-squared roll lever instead
                // of collapsing side pressure to one point near the roll axis.
                float halfDepth = wet.depth * .5f;
                float middle = (wet.minimumY + wet.maximumY) * .5f;
                for (int strip = -1; strip <= 1; strip += 2)
                {
                    var local = wet.centre;
                    local.y = middle + strip * halfDepth * .577350269f;
                    Vector3 point = body.position + body.rotation * local;
                    Vector3 flow = body.GetPointVelocity(point) - WaterVelocityAt(point);
                    float sideSpeed = Vector3.Dot(flow, right);
                    float forwardSpeed = Mathf.Abs(Vector3.Dot(flow, forward));
                    float area = halfDepth * length * confidence;
                    float sideForce = -.5f * waterDensity * area * sideSpeed
                        * (lateralDrag * Mathf.Abs(sideSpeed) + keelLiftSlope * forwardSpeed);
                    float maximumForce = .5f * sectionMass * Mathf.Abs(sideSpeed) / Time.fixedDeltaTime;
                    body.AddForceAtPosition(right * Mathf.Clamp(sideForce, -maximumForce, maximumForce), point, ForceMode.Force);
                }
            }
            // Evaluate resistance at the unboosted sailing speed, keeping the hull's
            // nonlinear friction and wave-drag curve intact for an artificial speed boost.
            float speed = Vector3.Dot(body.GetPointVelocity(CentreOfBuoyancy) - WaterVelocityAt(CentreOfBuoyancy), forward)
                / forwardSpeedMultiplier;
            float friction = ShipHullGeometry.SkinFrictionCoefficient(Mathf.Abs(speed), WaterlineLength, waterViscosity);
            float froude = Mathf.Abs(speed) / Mathf.Sqrt(Mathf.Max(gravity * WaterlineLength, .01f));
            float waveCoefficient = waveResistance * Mathf.Pow(froude / .4f, 4f);
            float dragArea = friction * WettedSurfaceArea + (hullPressureDrag + waveCoefficient) * SubmergedCrossSection;
            float resistance = -.5f * waterDensity * dragArea * speed * Mathf.Abs(speed);
            float maximumResistance = body.mass * Mathf.Abs(speed) / Time.fixedDeltaTime;
            resistance = Mathf.Clamp(resistance, -maximumResistance, maximumResistance);
            body.AddForceAtPosition(forward * resistance, CentreOfBuoyancy, ForceMode.Force);
            // The boost's extra drag acts at the centre of mass, matching the extra sail drive.
            body.AddForce(forward * (resistance * (forwardSpeedMultiplier - 1f)), ForceMode.Force);
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
            internal Vector3 Velocity;
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
