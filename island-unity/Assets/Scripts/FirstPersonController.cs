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
    private TerrainTileStreamer terrainStreamer;
    private float yaw;
    private float pitch;
    private float verticalSpeed;

    public bool IsActive { get; private set; }

    public void Configure(OrbitCamera overviewCamera)
    {
        orbitCamera = overviewCamera;
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
        enabled = false;
    }

    public void Enter(Vector3 groundPosition)
    {
        terrainStreamer?.SetPlayerPosition(groundPosition);
        orbitCamera.enabled = false;
        characterController.enabled = false;
        transform.position = groundPosition + Vector3.up * EyeHeight;
        yaw = transform.eulerAngles.y;
        pitch = NormalizePitch(transform.eulerAngles.x);
        pitch = Mathf.Clamp(pitch, -85f, 85f);
        verticalSpeed = -2f;
        characterController.enabled = true;
        IsActive = true;
        enabled = true;
        Cursor.lockState = CursorLockMode.Locked;
        Cursor.visible = false;
    }

    public void SetTerrainStreamer(TerrainTileStreamer value)
    {
        terrainStreamer = value;
    }

    public void Exit()
    {
        if (!IsActive)
        {
            return;
        }

        IsActive = false;
        characterController.enabled = false;
        enabled = false;
        orbitCamera.enabled = true;
        Cursor.lockState = CursorLockMode.None;
        Cursor.visible = true;
        terrainStreamer?.ClearPlayerFocus();
    }

    private void Update()
    {
        if (Input.GetKeyDown(KeyCode.Escape))
        {
            Exit();
            return;
        }

        terrainStreamer?.SetPlayerPosition(transform.position);

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
        terrainStreamer?.SetPlayerPosition(transform.position);
    }

    private static float NormalizePitch(float angle)
    {
        return angle > 180f ? angle - 360f : angle;
    }
}
