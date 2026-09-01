using UnityEngine;

public sealed class FirstPersonController : MonoBehaviour
{
    private const float EyeHeight = 1.7f;
    private const float WalkSpeed = 6f;
    private const float RunSpeed = 12f;
    private const float LookSensitivity = 2.2f;
    private const float Gravity = -24f;
    private const float JumpSpeed = 7f;

    private OrbitCamera orbitCamera;
    private CharacterController characterController;
    private IslandGenerator island;
    private float yaw;
    private float pitch;
    private float verticalSpeed;

    public bool IsActive { get; private set; }
    public bool IsCursorReleased { get; private set; }

    public void Configure(OrbitCamera overviewCamera, IslandGenerator islandGenerator)
    {
        orbitCamera = overviewCamera;
        island = islandGenerator;
        characterController = GetComponent<CharacterController>();
        if (characterController == null)
        {
            characterController = gameObject.AddComponent<CharacterController>();
        }

        characterController.height = 1.6f;
        characterController.radius = 0.3f;
        characterController.center = new Vector3(0f, -0.8f, 0f);
        characterController.stepOffset = 0.35f;
        characterController.slopeLimit = 55f;
        characterController.skinWidth = 0.05f;
        characterController.enabled = false;
        island.SetFirstPersonViewActive(false);
        enabled = false;
    }

    public void Enter(Vector3 groundPosition)
    {
        if (island == null)
        {
            return;
        }
        island.PrepareStreamingAt(groundPosition);
        if (!island.TrySnapToTerrain(groundPosition, out groundPosition))
        {
            Debug.LogWarning(
                "First-person entry was cancelled because terrain collision is not ready.");
            return;
        }
        orbitCamera.enabled = false;
        characterController.enabled = false;
        transform.position = groundPosition + Vector3.up * EyeHeight;
        yaw = transform.eulerAngles.y;
        pitch = NormalizePitch(transform.eulerAngles.x);
        pitch = Mathf.Clamp(pitch, -85f, 85f);
        verticalSpeed = -2f;
        characterController.enabled = true;
        island.SetStreamingTarget(transform);
        IsActive = true;
        island.SetFirstPersonViewActive(true);
        IsCursorReleased = false;
        enabled = true;
        ApplyCursorState();
    }

    public void SetIsland(IslandGenerator value)
    {
        island = value;
    }

    public void Exit()
    {
        if (!IsActive)
        {
            return;
        }

        IsActive = false;
        IsCursorReleased = false;
        characterController.enabled = false;
        enabled = false;
        orbitCamera.enabled = true;
        ApplyCursorState();
        island?.SetFirstPersonViewActive(false);
        island?.SetStreamingTarget(null);
    }

    private void Update()
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

        island?.PrepareStreamingAt(transform.position);
        if (IsCursorReleased)
        {
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
        island?.PrepareStreamingAt(transform.position);
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
