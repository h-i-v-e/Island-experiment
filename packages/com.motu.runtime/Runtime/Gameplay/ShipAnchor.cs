using System;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace Motu.Gameplay
{
    /// <summary>A bow rode that drags in deep water and catches terrain seabed in shallow water.</summary>
    [AddComponentMenu("Motu/Ship Anchor")]
    [RequireComponent(typeof(Rigidbody))]
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(1350)]
    public sealed class ShipAnchor : MonoBehaviour
    {
        [Tooltip("Rode attachment in physics-body local coordinates, at the bow. +Z points forward on the Explorer ship.")]
        [SerializeField] private Vector3 bowLocalPosition = new Vector3(0f, 6f, 14f);
        [Tooltip("Maximum catch depth below the mean sea level, rather than below the elevated bow.")]
        [Min(.1f)] [SerializeField] private float maximumCatchDepthMetres = 10f;
        [Tooltip("Rode angle from vertical when fully extended. 45 degrees leaves horizontal swing room equal to the bow-to-bottom drop.")]
        [Range(5f, 80f)] [SerializeField] private float rodeAngleDegrees = 45f;
        [Tooltip("Gentle downwind acceleration at 9 m/s wind, with sails furled. Existing momentum is retained.")]
        [Min(0f)] [SerializeField] private float windDriftAcceleration = .04f;
        [Tooltip("Maximum windage acceleration even in a storm, in m/s squared.")]
        [Min(0f)] [SerializeField] private float maximumWindAcceleration = .2f;
        [Tooltip("Maximum assisted yaw rate towards the wind's source while furled, including while the rode is slack or dragging. Zero disables assistance.")]
        [Min(0f)] [SerializeField] private float windwardTurnRateDegrees = 3f;
        [Min(.1f)] [SerializeField] private float windwardTurnResponseSeconds = 4f;
        [Tooltip("Fallback mean sea level when no ocean is assigned to the ship's buoyancy.")]
        [SerializeField] private float seaLevelMetres;

        private readonly RaycastHit[] hits = new RaycastHit[32];
        private Rigidbody body;
        private OceanBuoyancy buoyancy;
        private ConfigurableJoint rode;
        private bool tensionReported;

        public event Action Dropped;
        public event Action BottomContact;
        public event Action Tensioned;

        public bool IsDeployed { get; private set; }
        public bool IsHolding => rode != null;
        public bool IsTaut { get; private set; }
        public Vector3 DeploymentPosition { get; private set; }
        public Vector3 AnchorPosition { get; private set; }
        public float RodeLengthMetres { get; private set; }
        public Vector3 BowWorldPosition
        {
            get
            {
                if (body == null) CacheBody();
                return body.position + body.rotation * Vector3.Scale(bowLocalPosition, transform.lossyScale);
            }
        }

        private void Awake() => CacheBody();

        private void CacheBody()
        {
            body = GetComponent<Rigidbody>();
            buoyancy = GetComponent<OceanBuoyancy>();
        }

        /// <summary>Repeated furl commands retain the original catch and never reset momentum.</summary>
        public void SetDeployed(bool deployed)
        {
            if (IsDeployed == deployed) return;
            if (body == null) CacheBody();
            IsDeployed = deployed;
            ReleaseRode();
            if (!deployed) return;
            DeploymentPosition = BowWorldPosition;
            Dropped?.Invoke();
            if (isActiveAndEnabled && !body.isKinematic) TryCatch();
        }

        /// <summary>An explicit teleport starts a new drop instead of pulling towards the previous world anchor.</summary>
        public void ResetAfterTeleport()
        {
            if (!IsDeployed) return;
            SetDeployed(false);
            SetDeployed(true);
        }

        private void FixedUpdate()
        {
            var wind = Shader.GetGlobalVector("_MotuWeatherWind");
            Simulate(new Vector3(wind.x, 0f, wind.y).normalized * Mathf.Max(0f, wind.z));
        }

        internal void Simulate(Vector3 windVelocity)
        {
            if (!IsDeployed || !isActiveAndEnabled || body == null || body.isKinematic) return;
            if (!IsHolding) TryCatch();
            UpdateTension();
            // Windage acts behind the bow attachment. Rode tension then turns
            // the bow upwind through normal rigidbody torque, leaving yaw free.
            windVelocity.y = 0f;
            if (windVelocity.sqrMagnitude <= .0001f) return;
            var flow = windVelocity - Vector3.ProjectOnPlane(body.linearVelocity, Vector3.up);
            float acceleration = Mathf.Min(Mathf.Max(0f, maximumWindAcceleration),
                Mathf.Max(0f, windDriftAcceleration) * flow.sqrMagnitude / 81f);
            body.AddForce(flow.normalized * (body.mass * acceleration), ForceMode.Force);
            TurnBowIntoWind(windVelocity);
        }

        private void TurnBowIntoWind(Vector3 windVelocity)
        {
            if (windwardTurnRateDegrees <= 0f) return;
            var bowDirection = Vector3.ProjectOnPlane(BowWorldPosition - body.worldCenterOfMass, Vector3.up).normalized;
            if (bowDirection.sqrMagnitude < .5f) return;
            // Weather direction is where air travels. Aim at its source, using
            // the physical bow rather than the imported model's local axes.
            float angle = Vector3.SignedAngle(bowDirection, -windVelocity, Vector3.up);
            float response = Mathf.Max(.1f, windwardTurnResponseSeconds);
            float maximumRate = windwardTurnRateDegrees * Mathf.Deg2Rad;
            float targetRate = Mathf.Clamp(angle * Mathf.Deg2Rad / response, -maximumRate, maximumRate);
            float rateResponse = 4f / response;
            float acceleration = Mathf.Clamp((targetRate - body.angularVelocity.y) * rateResponse,
                -maximumRate * rateResponse, maximumRate * rateResponse);
            var axis = Quaternion.Inverse(body.rotation * body.inertiaTensorRotation) * Vector3.up;
            float inertia = Vector3.Dot(Vector3.Scale(axis, axis), body.inertiaTensor);
            body.AddTorque(Vector3.up * (acceleration * inertia), ForceMode.Force);
        }

        private void UpdateTension()
        {
            IsTaut = IsHolding
                && Vector3.Distance(BowWorldPosition, AnchorPosition) >= RodeLengthMetres - .05f
                && rode.currentForce.magnitude >= Mathf.Max(1f, body.mass * .001f);
            if (!IsTaut || tensionReported) return;
            tensionReported = true;
            Tensioned?.Invoke();
        }

        private void TryCatch()
        {
            var bow = BowWorldPosition;
            float seaLevel = buoyancy != null && buoyancy.Ocean != null && buoyancy.Ocean.SurfaceTransform != null
                ? buoyancy.Ocean.SurfaceTransform.position.y : seaLevelMetres;
            const float margin = .02f;
            var origin = new Vector3(bow.x, seaLevel + margin, bow.z);
            int count = gameObject.scene.GetPhysicsScene().Raycast(origin, Vector3.down, hits,
                Mathf.Max(.1f, maximumCatchDepthMetres) + margin, Physics.AllLayers, QueryTriggerInteraction.Ignore);
            int nearest = -1;
            for (int i = 0; i < count; i++)
                if (hits[i].collider is TerrainCollider && hits[i].point.y <= seaLevel
                    && (nearest < 0 || hits[i].distance < hits[nearest].distance)) nearest = i;
            if (nearest < 0) return;

            AnchorPosition = hits[nearest].point;
            float drop = Mathf.Max(.1f, bow.y - AnchorPosition.y);
            RodeLengthMetres = drop / Mathf.Cos(Mathf.Clamp(rodeAngleDegrees, 5f, 80f) * Mathf.Deg2Rad);
            rode = gameObject.AddComponent<ConfigurableJoint>();
            rode.autoConfigureConnectedAnchor = false;
            rode.anchor = bowLocalPosition;
            rode.connectedBody = null;
            rode.connectedAnchor = AnchorPosition;
            // All three limited linear axes form a slack spherical rope limit.
            // No drives, angular locks, spring braking or positional projection.
            rode.xMotion = rode.yMotion = rode.zMotion = ConfigurableJointMotion.Limited;
            rode.angularXMotion = rode.angularYMotion = rode.angularZMotion = ConfigurableJointMotion.Free;
            rode.linearLimit = new SoftJointLimit { limit = RodeLengthMetres, bounciness = 0f, contactDistance = .02f };
            rode.linearLimitSpring = new SoftJointLimitSpring();
            rode.projectionMode = JointProjectionMode.None;
            rode.enableCollision = true;
            BottomContact?.Invoke();
        }

        private void ReleaseRode()
        {
            if (rode != null)
            {
                // Destroy is deferred in Play mode; free the limit immediately.
                rode.xMotion = rode.yMotion = rode.zMotion = ConfigurableJointMotion.Free;
                UnityObjectLifetime.DestroyUnityObject(rode);
                rode = null;
            }
            RodeLengthMetres = 0f;
            IsTaut = tensionReported = false;
        }

        private void OnDisable()
        {
            IsDeployed = false;
            ReleaseRode();
        }
    }
}
