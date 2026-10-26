using UnityEngine;

namespace Motu.Rendering
{
    public sealed partial class OceanDeckWaveClamp
    {
        [Header("Texture displacement (optional)")]
        [Tooltip("Optional motion source when the visual hull follows a separately rooted physics body.")]
        [SerializeField] private Rigidbody waveBody;
        public Rigidbody WaveBody { get => waveBody; set { waveBody = value; body = value; } }
        [Tooltip("Linear greyscale: black clamps crests to the sea plane, mid-grey does nothing, white adds Bow Height at full speed. Null uses the capsule instead.")]
        [SerializeField] private Texture2D hullTexture;
        [Tooltip("Generated waterline maps store an independent bow rise in alpha. Leave off for legacy greyscale stamps.")]
        [SerializeField] private bool bowWaveInAlpha;
        public bool BowWaveInAlpha { get => bowWaveInAlpha; set => bowWaveInAlpha = value; }
        [Tooltip("World width and length of the hull texture. Texture top (+V) points from Start towards End (the bow).")]
        [SerializeField] private Vector2 textureSizeMetres = new Vector2(24, 42);
        [Tooltip("Scale of the clamping footprint inside the authored hull texture. Below one leaves a softer waterline rim; bow waves keep their original size.")]
        [Range(.5f, 1f), SerializeField] private float hullClampFootprintScale = 1f;
        [Tooltip("World-space radius used to feather the hull clamp. Water becomes fully flat only inside the softened footprint.")]
        [Range(0, 8), SerializeField] private float hullClampFeatherMetres = 3f;
        public float HullClampFootprintScale { get => hullClampFootprintScale; set => hullClampFootprintScale = Mathf.Clamp(value, .5f, 1f); }
        public float HullClampFeatherMetres { get => hullClampFeatherMetres; set => hullClampFeatherMetres = Mathf.Clamp(value, 0, 8); }
        [Tooltip("Move the raised bow-wave texture towards the stern, closing the gap between its rise and the bow. Does not move the hull clamp.")]
        [Min(0), SerializeField] private float bowWavePullbackMetres = 3.5f;
        [Tooltip("Fade the raised wave from zero at the waterline bow tip to full strength this many metres away.")]
        [Min(0), SerializeField] private float bowWaveTipBlendMetres = 2f;
        public float BowWavePullbackMetres { get => bowWavePullbackMetres; set => bowWavePullbackMetres = Mathf.Max(0, value); }
        public float BowWaveTipBlendMetres { get => bowWaveTipBlendMetres; set => bowWaveTipBlendMetres = Mathf.Max(0, value); }
        [Min(0), SerializeField] private float bowHeight = 1.2f;
        [Min(.1f), SerializeField] private float fullWaveSpeed = 8;
        [Range(0, 1), SerializeField] private float waveFoam = .8f;

        private Rigidbody body;
        public Texture2D HullTexture => hullTexture;
        internal bool HasHullTexture => hullTexture != null;

        public void ConfigureHullTexture(Texture2D hull)
        {
            hullTexture = hull;
            bowWaveInAlpha = false;
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

        internal void DrawHullStamp(OceanShipWaveField.StampWriter writer)
        {
            if (hullTexture == null) return;
            GetFootprint(out var centre, out var forward);
            var speed = Application.isPlaying && body != null ? Mathf.Max(0, Vector3.Dot(body.linearVelocity, forward)) : 0;
            writer.Draw(hullTexture, centre, forward, textureSizeMetres,
                HasConvexHull ? 0 : 1, bowHeight * Mathf.Clamp01(speed / fullWaveSpeed), waveFoam,
                hullClampFootprintScale, hullClampFeatherMetres, bowWavePullbackMetres,
                bowWaveTipBlendMetres, (transform.TransformPoint(end) - transform.TransformPoint(start)).magnitude * .5f + radiusMetres, bowWaveInAlpha);
        }
    }
}
