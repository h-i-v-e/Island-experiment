using Motu.World;
using UnityEngine;

namespace Motu.Gameplay
{
    [AddComponentMenu("Motu/Ship Controller")]
    [RequireComponent(typeof(Rigidbody), typeof(OceanBuoyancy))]
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(1300)]
    public sealed class ShipController : MonoBehaviour
    {
        [Tooltip("Reference whose forward points towards the bow and up points above the deck.")]
        [SerializeField] private Transform helm;
        [SerializeField] private ShipBridgeCamera bridgeCamera;
        [SerializeField] private IslandWorldManager worldManager;
        [SerializeField] private bool readPlayerInput = true;
        [Min(0f)] [SerializeField] private float forwardSpeed = 8f;
        [Min(0f)] [SerializeField] private float reverseSpeed = 3f;
        [Min(.01f)] [SerializeField] private float speedResponseSeconds = 3f;
        [Min(0f)] [SerializeField] private float acceleration = 1.5f;
        [Min(0f)] [SerializeField] private float brakingAcceleration = 2f;
        [Min(0f)] [SerializeField] private float turnRateDegrees = 8f;
        [Min(.01f)] [SerializeField] private float rudderResponseSeconds = 2f;
        [Min(.01f)] [SerializeField] private float fullRudderSpeed = 3f;

        private Rigidbody body;
        private OceanBuoyancy buoyancy;
        private Quaternion helmLocalRotation;
        private float throttle;
        private float steering;
        private bool brake;

        public ShipBridgeCamera BridgeCamera => bridgeCamera;
        public float SpeedMetresPerSecond => body != null ? Vector3.Dot(body.linearVelocity, Forward) : 0f;
        public bool ReadPlayerInput { get => readPlayerInput; set { readPlayerInput = value; SetInput(0, 0); } }
        private Vector3 Forward => Vector3.ProjectOnPlane(body.rotation * helmLocalRotation * Vector3.forward,
            Vector3.up).normalized;

        public void Configure(Transform helmReference, ShipBridgeCamera camera, IslandWorldManager manager)
        {
            helm = helmReference;
            bridgeCamera = camera;
            worldManager = manager;
            CacheBody();
        }

        private void Awake() => CacheBody();
        private void CacheBody()
        {
            body = GetComponent<Rigidbody>();
            buoyancy = GetComponent<OceanBuoyancy>();
            helmLocalRotation = helm != null ? Quaternion.Inverse(transform.rotation) * helm.rotation : Quaternion.identity;
        }

        private void Start()
        {
            if (worldManager == null || bridgeCamera == null) return;
            worldManager.SetStreamingTarget(bridgeCamera.transform);
            worldManager.SetFirstPersonViewActive(true);
        }

        private void Update()
        {
            if (!readPlayerInput) return;
            if (bridgeCamera == null || !bridgeCamera.IsLooking || !Application.isFocused)
            {
                SetInput(0f, 0f);
                return;
            }
            SetInput(Input.GetAxisRaw("Vertical"), Input.GetAxisRaw("Horizontal"), Input.GetKey(KeyCode.Space));
        }

        public void SetInput(float forward, float rudder, bool braking = false)
        {
            throttle = Mathf.Clamp(forward, -1f, 1f);
            steering = Mathf.Clamp(rudder, -1f, 1f);
            brake = braking;
        }

        private void FixedUpdate()
        {
            if (!buoyancy.isActiveAndEnabled) return;
            ApplyControls(buoyancy.SubmergedFraction, Time.fixedDeltaTime);
        }

        internal void ApplyControls(float immersion, float deltaTime)
        {
            if (body.isKinematic || immersion <= 0f) return;
            var forward = Forward;
            if (forward.sqrMagnitude < .5f) return;
            var speed = Vector3.Dot(body.linearVelocity, forward);
            var drive = DriveAcceleration(throttle, speed, forwardSpeed, reverseSpeed, acceleration,
                speedResponseSeconds);
            var force = forward * drive;
            if (brake)
            {
                var velocity = Vector3.ProjectOnPlane(body.linearVelocity, Vector3.up);
                force = -velocity.normalized * Mathf.Min(brakingAcceleration, velocity.magnitude / deltaTime);
            }
            body.AddForce(force * (body.mass * immersion), ForceMode.Force);
            var targetYaw = steering * turnRateDegrees * Mathf.Deg2Rad
                * Mathf.Clamp(speed / fullRudderSpeed, -1f, 1f);
            var yawAcceleration = (targetYaw - body.angularVelocity.y) / rudderResponseSeconds;
            body.AddTorque(Vector3.up * (yawAcceleration * InertiaAboutWorldUp(body) * immersion),
                ForceMode.Force);
        }

        internal static float DriveAcceleration(float throttle, float speed, float forwardLimit,
            float reverseLimit, float maximumAcceleration, float responseSeconds)
        {
            if (Mathf.Abs(throttle) < .001f) return 0f; // Released throttle coasts.
            var target = throttle * (throttle > 0f ? forwardLimit : reverseLimit);
            return Mathf.Clamp((target - speed) / Mathf.Max(responseSeconds, .01f),
                -maximumAcceleration, maximumAcceleration);
        }

        internal static float InertiaAboutWorldUp(Rigidbody rigidbody)
        {
            var axis = Quaternion.Inverse(rigidbody.rotation * rigidbody.inertiaTensorRotation) * Vector3.up;
            return Vector3.Dot(Vector3.Scale(axis, axis), rigidbody.inertiaTensor);
        }

        public void Teleport(Vector3 destination)
        {
            if (body == null) CacheBody();
            destination.y = body.position.y;
            body.position = destination;
            body.linearVelocity = Vector3.zero;
            body.angularVelocity = Vector3.zero;
            bridgeCamera?.SnapToShip();
            if (worldManager != null && bridgeCamera != null)
                worldManager.SetStreamingTarget(bridgeCamera.transform);
        }

        private void OnDisable() => SetInput(0f, 0f);
    }
}
