using UnityEngine;
using Motu.Islands;
using Motu.World;
using Motu.Streaming;

namespace Motu.Gameplay
{
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "FirstPersonController")]
    public sealed class FirstPersonController : MonoBehaviour
    {
        private const float EyeHeight = 1.7f;
        private const float WalkSpeed = 6f;
        private const float RunSpeed = 12f;
        private const float DefaultFlySpeed = RunSpeed * 2f;
        private const float LookSensitivity = 2.2f;
        private const float Gravity = -24f;
        private const float JumpSpeed = 7f;

        [Header("Fly Mode")]
        [SerializeField] private KeyCode toggleFlyModeKey = KeyCode.V;
        [SerializeField, Min(0.1f)] private float flyClearanceMetres = 4f;
        [SerializeField, Min(0.1f)] private float flySpeedMetresPerSecond = DefaultFlySpeed;
        [SerializeField, Min(0.01f)] private float flyDescentSmoothTime = 0.18f;

        [Header("Torch")]
        [SerializeField] private KeyCode toggleTorchKey = KeyCode.T;
        [SerializeField, Min(1f)] private float torchRangeMetres = 35f;
        [SerializeField, Min(0f)] private float torchIntensity = 4f;
        [SerializeField, Range(1f, 179f)] private float torchSpotAngle = 75f;
        private Light torchLight;
        private bool torchRequested;

        private OrbitCamera orbitCamera;
        private CharacterController characterController;
        private CaveCollisionScope caveCollision;
        private IWorldSurfaceQuery worldSurface;
        private float yaw;
        private float pitch;
        private float verticalSpeed;
        private float flyVerticalVelocity;
        public bool FollowsTerrainInFlyMode { get; private set; } = true;

        public bool IsActive { get; private set; }
        public bool IsCursorReleased { get; private set; }
        public bool IsFlyMode { get; private set; }
        public KeyCode ToggleFlyModeKey => toggleFlyModeKey;
        public KeyCode ToggleTorchKey => toggleTorchKey;
        public bool IsTorchOn => torchLight != null && torchLight.isActiveAndEnabled;
        public float FlyClearanceMetres => flyClearanceMetres;
        public float FlySpeedMetresPerSecond => flySpeedMetresPerSecond;

        public void Configure(OrbitCamera overviewCamera, IslandGenerator islandGenerator)
        {
            Configure(overviewCamera, (IWorldSurfaceQuery)islandGenerator);
        }

        public void Configure(
            OrbitCamera overviewCamera,
            IWorldSurfaceQuery surfaceQuery)
        {
            orbitCamera = overviewCamera;
            worldSurface = surfaceQuery;
            characterController = GetComponent<CharacterController>();
            if (characterController == null)
            {
                characterController = gameObject.AddComponent<CharacterController>();
            }

            caveCollision?.Dispose();
            caveCollision = new CaveCollisionScope(characterController);
            characterController.height = 1.6f;
            characterController.radius = 0.3f;
            characterController.center = new Vector3(0f, -0.8f, 0f);
            characterController.stepOffset = 0.35f;
            characterController.slopeLimit = 55f;
            characterController.skinWidth = 0.05f;
            caveCollision?.Dispose();
            characterController.enabled = false;
            worldSurface.SetFirstPersonViewActive(false);
            enabled = false;
        }

        public void Enter(Vector3 groundPosition)
        {
            if (worldSurface == null)
            {
                return;
            }
            worldSurface.PrepareStreamingAt(groundPosition);
            if (!worldSurface.TrySnapToTerrain(groundPosition, out groundPosition))
            {
                Debug.LogWarning(
                    "First-person entry was cancelled because terrain collision is not ready.");
                return;
            }
            EnterPreparedGround(groundPosition);
        }

        internal void EnterPreparedGround(Vector3 groundPosition)
        {
            orbitCamera.enabled = false;
            caveCollision?.Dispose();
            characterController.enabled = false;
            transform.position = groundPosition + Vector3.up * EyeHeight;
            yaw = transform.eulerAngles.y;
            pitch = NormalizePitch(transform.eulerAngles.x);
            pitch = Mathf.Clamp(pitch, -85f, 85f);
            verticalSpeed = -2f;
            flyVerticalVelocity = 0f;
            IsFlyMode = false;
            characterController.enabled = true;
            UpdateCaveCollision();
            worldSurface.SetStreamingTarget(transform);
            IsActive = true;
            worldSurface.SetFirstPersonViewActive(true);
            IsCursorReleased = false;
            enabled = true;
            RefreshTorch();
            ApplyCursorState();
        }

        public void BeginFlying(
            Vector3 worldPosition,
            float yawDegrees = 0f,
            float pitchDegrees = 0f,
            bool followTerrain = true)
        {
            if (worldSurface == null)
            {
                return;
            }

            worldSurface.PrepareStreamingAt(worldPosition);
            orbitCamera.enabled = false;
            caveCollision?.Dispose();
            characterController.enabled = false;
            transform.SetPositionAndRotation(
                worldPosition,
                Quaternion.Euler(pitchDegrees, yawDegrees, 0f));
            yaw = yawDegrees;
            pitch = Mathf.Clamp(pitchDegrees, -85f, 85f);
            verticalSpeed = 0f;
            flyVerticalVelocity = 0f;
            IsFlyMode = true;
            FollowsTerrainInFlyMode = followTerrain;
            worldSurface.SetStreamingTarget(transform);
            IsActive = true;
            worldSurface.SetFirstPersonViewActive(true);
            IsCursorReleased = false;
            enabled = true;
            RefreshTorch();
            FollowFlySurface(0f);
            ApplyCursorState();
        }

        public void SetIsland(IslandGenerator value)
        {
            worldSurface = value;
        }

        public void Teleport(Vector3 worldPosition) => Teleport(worldPosition, !IsActive || IsCursorReleased);

        public void Teleport(Vector3 worldPosition, bool releaseCursor)
        {
            if (worldSurface == null)
                return;
            var heading = transform.eulerAngles;
            // Terrain-following flight waits safely above sea level; free flight
            // retains its chosen altitude across a minimap teleport.
            var followTerrain = !IsActive || !IsFlyMode || FollowsTerrainInFlyMode;
            if (!followTerrain) worldPosition.y = transform.position.y;
            BeginFlying(worldPosition, heading.y, NormalizePitch(heading.x), followTerrain);
            orbitCamera.SetTarget(worldPosition);
            IsCursorReleased = releaseCursor;
            ApplyCursorState();
        }

        public void SetWorldSurface(IWorldSurfaceQuery value)
        {
            worldSurface = value;
        }

        public void Exit()
        {
            if (!IsActive)
            {
                return;
            }

            IsActive = false;
            RefreshTorch();
            IsCursorReleased = false;
            IsFlyMode = false;
            flyVerticalVelocity = 0f;
            caveCollision?.Dispose();
            characterController.enabled = false;
            enabled = false;
            orbitCamera.enabled = true;
            ApplyCursorState();
            worldSurface?.SetFirstPersonViewActive(false);
            worldSurface?.SetStreamingTarget(null);
        }

        private void Update() => UpdateMovement();
        private void OnEnable() => RefreshTorch();
        private void OnDisable()
        {
            caveCollision?.Dispose();
            if (torchLight != null) torchLight.enabled = false;
        }
        private void OnDestroy() => caveCollision?.Dispose();

        public void ToggleTorch() => SetTorchEnabled(!torchRequested);

        public void SetTorchEnabled(bool active)
        {
            torchRequested = active;
            if (active && torchLight == null)
            {
                var lamp = new GameObject("Player Torch");
                lamp.transform.SetParent(transform, false);
                // Keep the light near the eye so it cannot poke through a wall
                // before the camera reaches it.
                lamp.transform.localPosition = new Vector3(0.12f, -0.1f, 0f);
                torchLight = lamp.AddComponent<Light>();
                torchLight.type = LightType.Spot;
                torchLight.renderMode = LightRenderMode.ForcePixel;
                torchLight.color = new Color(1f, 0.94f, 0.82f);
                torchLight.shadows = LightShadows.Soft;
                torchLight.shadowNearPlane = 0.05f;
                torchLight.shadowBias = 0.02f;
            }
            RefreshTorch();
        }

        private void RefreshTorch()
        {
            if (torchLight == null) return;
            torchLight.range = Mathf.Max(1f, torchRangeMetres);
            torchLight.intensity = Mathf.Max(0f, torchIntensity);
            torchLight.spotAngle = Mathf.Clamp(torchSpotAngle, 1f, 179f);
            torchLight.innerSpotAngle = torchLight.spotAngle * 0.5f;
            torchLight.enabled = torchRequested && IsActive && isActiveAndEnabled;
        }
        private void UpdateCaveCollision()
        {
            var bounds = characterController.bounds;
            var feet = new Vector3(bounds.center.x, bounds.min.y, bounds.center.z);
            caveCollision?.Update(feet, !IsFlyMode && characterController.enabled);
        }


        internal void UpdateMovement()
        {
            if (Input.GetKeyDown(KeyCode.Escape))
            {
                Exit();
                return;
            }
            if (Input.GetKeyDown(KeyCode.Tab))
            {
                IsCursorReleased = !IsCursorReleased;
                ApplyCursorState();
            }
            if (toggleTorchKey != KeyCode.None && Input.GetKeyDown(toggleTorchKey))
                ToggleTorch();
            if (toggleFlyModeKey != KeyCode.None
                && Input.GetKeyDown(toggleFlyModeKey))
            {
                SetFlyMode(!IsFlyMode);
            }

            worldSurface?.PrepareStreamingAt(transform.position);
            UpdateCaveCollision();
            if (IsCursorReleased)
            {
                if (IsFlyMode)
                    FollowFlySurface(Time.deltaTime);
                return;
            }

            yaw += Input.GetAxisRaw("Mouse X") * LookSensitivity;
            pitch = Mathf.Clamp(
                pitch - Input.GetAxisRaw("Mouse Y") * LookSensitivity,
                -85f,
                85f);
            transform.rotation = Quaternion.Euler(pitch, yaw, 0f);

            var input = new Vector2(Input.GetAxisRaw("Horizontal"), Input.GetAxisRaw("Vertical"));
            input = Vector2.ClampMagnitude(input, 1f);
            var heading = Quaternion.Euler(0f, yaw, 0f);
            var movement = heading * new Vector3(input.x, 0f, input.y);
            if (IsFlyMode)
            {
                UpdateFlyMovement(movement);
                return;
            }
            var speed = Input.GetKey(KeyCode.LeftShift) ? RunSpeed : WalkSpeed;

            if (characterController.isGrounded)
            {
                verticalSpeed = -2f;
                if (Input.GetKeyDown(KeyCode.Space))
                {
                    verticalSpeed = JumpSpeed;
                }
            }
            else
            {
                verticalSpeed += Gravity * Time.deltaTime;
            }

            movement = movement * speed + Vector3.up * verticalSpeed;
            characterController.Move(movement * Time.deltaTime);
            worldSurface?.PrepareStreamingAt(transform.position);
        }

        private void SetFlyMode(bool active)
        {
            IsFlyMode = active;
            FollowsTerrainInFlyMode = true;
            verticalSpeed = 0f;
            flyVerticalVelocity = 0f;
            caveCollision?.Dispose();
            characterController.enabled = !active;
            if (!active) UpdateCaveCollision();
            if (active)
            {
                FollowFlySurface(0f);
            }
        }

        private void UpdateFlyMovement(Vector3 direction)
        {
            var speedMultiplier = Input.GetKey(KeyCode.LeftShift) ? 2f : 1f;
            if (!FollowsTerrainInFlyMode)
            {
                var vertical = (Input.GetKey(KeyCode.E) ? 1f : 0f) - (Input.GetKey(KeyCode.Q) ? 1f : 0f);
                direction = Vector3.ClampMagnitude(direction + Vector3.up * vertical, 1f);
            }
            transform.position += direction
                * (flySpeedMetresPerSecond * speedMultiplier * Time.deltaTime);
            worldSurface?.PrepareStreamingAt(transform.position);
            FollowFlySurface(Time.deltaTime);
            worldSurface?.PrepareStreamingAt(transform.position);
        }

        private void FollowFlySurface(float deltaTime)
        {
            if (worldSurface == null || !FollowsTerrainInFlyMode)
            {
                return;
            }
            var position = transform.position;
            var targetHeight = worldSurface.GetTerrainOrSeaHeight(position)
                + flyClearanceMetres;
            if (position.y <= targetHeight || deltaTime <= 0f)
            {
                position.y = targetHeight;
                flyVerticalVelocity = 0f;
            }
            else
            {
                position.y = Mathf.SmoothDamp(
                    position.y,
                    targetHeight,
                    ref flyVerticalVelocity,
                    flyDescentSmoothTime,
                    Mathf.Infinity,
                    deltaTime);
            }
            transform.position = position;
        }

        private void ApplyCursorState()
        {
            var captureCursor = IsActive && !IsCursorReleased;
            Cursor.lockState = captureCursor ? CursorLockMode.Locked : CursorLockMode.None;
            Cursor.visible = !captureCursor;
        }

        private static float NormalizePitch(float angle)
        {
            return angle > 180f ? angle - 360f : angle;
        }
    }
}
