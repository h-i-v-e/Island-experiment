using UnityEngine;

namespace Motu.Gameplay
{
    // Opt-in loading for boats tuned by draft. Ordinary hulls retain their authored mass.
    [AddComponentMenu("Motu/Hull Draft Loading")]
    [RequireComponent(typeof(Rigidbody), typeof(OceanBuoyancy))]
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(-250)]
    public sealed class HullDraftLoading : MonoBehaviour
    {
        [Tooltip("The body's intended resting rotation. Draft is measured vertically at this pose, so model trim need not follow its keel axis.")]
        [SerializeField] private Vector3 referenceEulerAngles;
        [Tooltip("Align the centre of mass below the displaced-water centre at the reference pose whenever draft changes. Does not lock rotation or apply an artificial righting torque.")]
        [SerializeField] private bool balanceAtReferencePose;
        [Tooltip("Centre of mass height above the hull's lowest point at the reference pose, in world metres.")]
        [Min(0f), SerializeField] private float centreOfMassHeight = .15f;

        private Rigidbody body;
        private OceanBuoyancy buoyancy;
        private ShipHullGeometry lastGeometry;
        private Vector3 lastScale, lastReferenceEulerAngles;
        private float lastDraft = float.NaN, lastDensity, lastCentreHeight, loadingMass;
        private bool lastBalance, dirty = true;

        public Quaternion ReferenceRotation => Quaternion.Euler(referenceEulerAngles);
        public Vector3 ReferenceUp => Quaternion.Inverse(ReferenceRotation) * Vector3.up;

        public readonly struct Loading
        {
            public readonly float Mass, MinimumHeight, Waterline;
            // Scaled body-local coordinates, matching ShipHullGeometry.Immerse.
            public readonly Vector3 CentreOfBuoyancy;
            public Loading(float mass, float minimumHeight, float waterline, Vector3 centre)
            { Mass = mass; MinimumHeight = minimumHeight; Waterline = waterline; CentreOfBuoyancy = centre; }
        }

        private void Awake() => UpdateLoading();
        private void FixedUpdate() => UpdateLoading();
        private void OnValidate() => dirty = true;
        private void OnEnable() => dirty = true;

        public void ConfigureReferencePose(Quaternion rotation, bool balance, float centreHeight)
        {
            var angles = rotation.eulerAngles;
            referenceEulerAngles = new Vector3(Mathf.DeltaAngle(0, angles.x), Mathf.DeltaAngle(0, angles.y), Mathf.DeltaAngle(0, angles.z));
            balanceAtReferencePose = balance;
            centreOfMassHeight = Mathf.Max(0f, centreHeight);
            ApplyDraftLoading();
        }

        [ContextMenu("Apply Draft Loading")]
        public void ApplyDraftLoading()
        {
            dirty = true;
            UpdateLoading();
        }

        private void UpdateLoading()
        {
            if (body == null) body = GetComponent<Rigidbody>();
            if (buoyancy == null) buoyancy = GetComponent<OceanBuoyancy>();
            var scale = transform.lossyScale;
            if (dirty || lastGeometry != buoyancy.HullGeometry || lastScale != scale
                || lastDraft != buoyancy.Draft || lastDensity != buoyancy.WaterDensity
                || lastReferenceEulerAngles != referenceEulerAngles || lastBalance != balanceAtReferencePose
                || lastCentreHeight != centreOfMassHeight)
            {
                dirty = false;
                lastGeometry = buoyancy.HullGeometry;
                lastScale = scale;
                lastDraft = buoyancy.Draft;
                lastDensity = buoyancy.WaterDensity;
                lastReferenceEulerAngles = referenceEulerAngles;
                lastBalance = balanceAtReferencePose;
                lastCentreHeight = centreOfMassHeight;
                if (!TryCalculateLoading(lastGeometry, scale, lastDraft, lastDensity, ReferenceUp, out var loading))
                {
                    loadingMass = 0f;
                    Debug.LogWarning("Draft loading needs a valid hull, reference pose and a draft strictly between zero and the hull's reference height. Keeping the current mass.", this);
                }
                else
                {
                    loadingMass = loading.Mass;
                    if (balanceAtReferencePose)
                    {
                        var up = ReferenceUp;
                        var scaledCentre = loading.CentreOfBuoyancy + up * (loading.MinimumHeight + centreOfMassHeight
                            - Vector3.Dot(up, loading.CentreOfBuoyancy));
                        // Rigidbody mass-local coordinates do not apply Transform scale.
                        body.centerOfMass = scaledCentre;
                    }
                }
            }
            if (loadingMass > 0f && !Mathf.Approximately(body.mass, loadingMass))
            {
                body.mass = loadingMass;
                if (Application.isPlaying && !body.isKinematic) body.WakeUp();
            }
        }

        // Archimedes' equilibrium: the mass of water displaced at the desired waterline.
        // The reference pose only calibrates loading; live buoyancy still integrates
        // the real waves, heel and pitch without teleporting or constraining the boat.
        public static bool TryCalculateMass(ShipHullGeometry geometry, Vector3 scale, float draft,
            float density, out float mass)
        {
            bool valid = TryCalculateLoading(geometry, scale, draft, density, Vector3.up, out var loading);
            mass = loading.Mass;
            return valid;
        }

        public static bool TryCalculateLoading(ShipHullGeometry geometry, Vector3 scale, float draft,
            float density, Vector3 referenceUp, out Loading loading)
        {
            loading = default;
            if (geometry == null || !float.IsFinite(draft) || draft <= 0f
                || !float.IsFinite(density) || density <= 0f
                || !float.IsFinite(scale.x) || !float.IsFinite(scale.y) || !float.IsFinite(scale.z)
                || Mathf.Abs(scale.x * scale.y * scale.z) < .000001f
                || !float.IsFinite(referenceUp.x) || !float.IsFinite(referenceUp.y) || !float.IsFinite(referenceUp.z)
                || referenceUp.sqrMagnitude < .000001f) return false;
            referenceUp.Normalize();
            float minimum = float.MaxValue, maximum = float.MinValue;
            int largest = 0;
            foreach (var section in geometry.Sections)
            {
                if (section.outline == null || section.outline.Length < 3 || section.length <= 0f) return false;
                largest = Mathf.Max(largest, section.outline.Length);
                foreach (var point in section.outline)
                {
                    float height = Vector3.Dot(referenceUp, Vector3.Scale(new Vector3(point.x, point.y, section.station), scale));
                    minimum = Mathf.Min(minimum, height);
                    maximum = Mathf.Max(maximum, height);
                }
            }
            if (largest == 0 || draft >= maximum - minimum) return false;
            var clipped = new Vector2[largest + 2];
            float volume = 0f;
            Vector3 centre = Vector3.zero;
            float waterline = minimum + draft;
            foreach (var section in geometry.Sections)
            {
                var wet = ShipHullGeometry.Immerse(section, scale, referenceUp, waterline, clipped);
                float part = wet.area * section.length * Mathf.Abs(scale.z);
                volume += part;
                centre += wet.centre * part;
            }
            float result = volume * density;
            if (!float.IsFinite(result) || result <= 0f) return false;
            loading = new Loading(result, minimum, waterline, centre / volume);
            return true;
        }
    }
}
