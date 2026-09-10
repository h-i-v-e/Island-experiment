using UnityEngine;

namespace Motu.Rendering
{
    public sealed partial class OceanDeckWaveClamp
    {
        [Header("Texture displacement (optional)")]
        [Tooltip("Linear greyscale: black clamps crests to the sea plane, mid-grey does nothing, white adds Bow Height at full speed. Null uses the capsule instead.")]
        [SerializeField] private Texture2D hullTexture;
        [Tooltip("World width and length of the hull texture. Texture top (+V) points from Start towards End (the bow).")]
        [SerializeField] private Vector2 textureSizeMetres = new Vector2(24, 42);
        [Tooltip("Scale of the clamping footprint inside the authored hull texture. Below one leaves a softer waterline rim; bow waves keep their original size.")]
        [Range(.5f, 1f), SerializeField] private float hullClampFootprintScale = .9f;
        [Tooltip("World-space radius used to feather the hull clamp. Water becomes fully flat only inside the softened, shrunken footprint.")]
        [Range(0, 8), SerializeField] private float hullClampFeatherMetres = 3f;
        public float HullClampFootprintScale { get => hullClampFootprintScale; set => hullClampFootprintScale = Mathf.Clamp(value, .5f, 1f); }
        public float HullClampFeatherMetres { get => hullClampFeatherMetres; set => hullClampFeatherMetres = Mathf.Clamp(value, 0, 8); }
        [Min(0), SerializeField] private float bowHeight = 1.2f;
        [Min(.1f), SerializeField] private float fullWaveSpeed = 8;
        [Range(0, 1), SerializeField] private float waveFoam = .8f;

        [Header("Wake left behind the ship")]
        [Tooltip("Same greyscale convention. Normally grey background with white wake ridges; avoid black here unless deliberately flattening waves behind the ship.")]
        [SerializeField] private Texture2D wakeTexture;
        [SerializeField] private Vector2 wakeSizeMetres = new Vector2(18, 14);
        [Min(0), SerializeField] private float wakeHeight = .8f;
        [Min(.1f), SerializeField] private float wakeLifetime = 20;
        [Min(.5f), SerializeField] private float wakeSpacing = 3;
        [Min(0), SerializeField] private float wakeSpreadMetresPerSecond = .35f;

        private Rigidbody body;
        private const int MaximumWakeStamps = 96;
        private readonly WakeStamp[] trail = new WakeStamp[MaximumWakeStamps];
        private int trailHead;
        private int trailCount;
        private bool hasPreviousTrailPosition;
        private Vector3 previousTrailPosition;
        private Vector3 lastStampedPosition;
        private float previousTrailTime;

        internal struct WakeStamp
        {
            internal Vector3 Position;
            internal Vector3 Forward;
            internal float CreatedAt;
            internal float Strength;
        }

        internal int TrailCount => trailCount;
        public Texture2D HullTexture => hullTexture;
        public Texture2D WakeTexture => wakeTexture;
        internal bool HasHullTexture => hullTexture != null;
        internal WakeStamp TrailAt(int index) => trail[(trailHead + index) % MaximumWakeStamps];

        public void ConfigureTextures(Texture2D hull, Texture2D wake)
        {
            hullTexture = hull;
            wakeTexture = wake;
            ResetTrail();
            BindGlobals();
        }

        internal void FitGeneratedHullTexture(Texture2D hull, Vector3 worldCentre, Vector3 forward,
            Vector2 sectionSize, Vector2 textureSize)
        {
            var radius = Mathf.Max(.01f, sectionSize.x * .5f);
            var halfSegment = Mathf.Max(.01f, sectionSize.y * .5f-radius);
            Configure(transform.InverseTransformPoint(worldCentre-forward*halfSegment),
                transform.InverseTransformPoint(worldCentre+forward*halfSegment), radius, blendMetres);
            hullTexture = hull;
            textureSizeMetres = textureSize;
            ResetTrail();
            BindGlobals();
        }

        internal void GetFootprint(out Vector3 centre, out Vector3 forward)
        {
            var a = transform.TransformPoint(start);
            var b = transform.TransformPoint(end);
            centre = (a + b) * .5f;
            forward = Vector3.ProjectOnPlane(b - a, Vector3.up).normalized;
            if (forward.sqrMagnitude < .01f) forward = Vector3.forward;
        }

        private void LateUpdate()
        {
            if (!Application.isPlaying || hullTexture == null || wakeTexture == null) return;
            GetFootprint(out var centre, out var forward);
            var velocity = body != null ? Vector3.ProjectOnPlane(body.linearVelocity, Vector3.up) : Vector3.zero;
            var direction = velocity.sqrMagnitude > .01f ? velocity.normalized : forward;
            var stern = centre - direction * ((transform.TransformPoint(end) - transform.TransformPoint(start)).magnitude * .5f + radiusMetres);
            AdvanceTrail(stern, direction, velocity.magnitude, Time.time);
        }

        internal void AdvanceTrail(Vector3 position, Vector3 direction, float speed, float now)
        {
            position.y = 0; // Deposited in world space, independent of hull heave.
            while (trailCount > 0 && now - TrailAt(0).CreatedAt >= wakeLifetime)
            {
                trailHead = (trailHead + 1) % MaximumWakeStamps;
                trailCount--;
            }
            var elapsed = Mathf.Max(0, now - previousTrailTime);
            if (hasPreviousTrailPosition && (now < previousTrailTime ||
                Vector3.Distance(previousTrailPosition, position) > Mathf.Max(20, speed * elapsed * 3 + 5)))
                ResetTrail(); // Teleports must not draw a line across the ocean.
            if (!hasPreviousTrailPosition)
            {
                lastStampedPosition = position;
                hasPreviousTrailPosition = true;
            }
            previousTrailPosition = position;
            previousTrailTime = now;
            if (speed < .25f) { lastStampedPosition = position; return; }
            var distance = Vector3.Distance(lastStampedPosition, position);
            var steps = Mathf.Min(MaximumWakeStamps, Mathf.FloorToInt(distance / wakeSpacing));
            var heading = distance > .001f ? (position - lastStampedPosition) / distance : direction;
            for (var i = 0; i < steps; i++)
            {
                lastStampedPosition += heading * wakeSpacing;
                if (trailCount == MaximumWakeStamps) { trailHead = (trailHead + 1) % MaximumWakeStamps; trailCount--; }
                trail[(trailHead + trailCount++) % MaximumWakeStamps] = new WakeStamp
                {
                    Position = lastStampedPosition,
                    Forward = direction,
                    CreatedAt = now,
                    Strength = Mathf.Clamp01(speed / fullWaveSpeed),
                };
            }
        }

        private void ResetTrail()
        {
            trailHead = trailCount = 0;
            hasPreviousTrailPosition = false;
        }

        internal void DrawStamps(OceanShipWaveField.StampWriter writer, float now)
        {
            if (hullTexture == null) return;
            GetFootprint(out var centre, out var forward);
            var speed = Application.isPlaying && body != null ? Mathf.Max(0, Vector3.Dot(body.linearVelocity, forward)) : 0;
            writer.Draw(hullTexture, centre, forward, textureSizeMetres,
                1, bowHeight * Mathf.Clamp01(speed / fullWaveSpeed), waveFoam,
                hullClampFootprintScale, hullClampFeatherMetres);
            if (wakeTexture == null) return;
            for (var i = 0; i < trailCount; i++)
            {
                var stamp = TrailAt(i);
                var age = Mathf.Max(0, now - stamp.CreatedAt);
                var fade = Mathf.Clamp01(1 - age / wakeLifetime);
                fade *= fade;
                if (fade <= 0) continue;
                writer.Draw(wakeTexture, stamp.Position, stamp.Forward,
                    wakeSizeMetres + Vector2.right * (age * wakeSpreadMetresPerSecond * 2),
                    fade, wakeHeight * stamp.Strength * fade, waveFoam * fade);
            }
        }
    }
}
