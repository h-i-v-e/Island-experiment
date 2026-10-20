using UnityEngine;
using UnityEngine.InputSystem;

// Inspection controls wrap the real character so boarding uses normal gameplay locomotion.
[DefaultExecutionOrder(-900)]
public sealed class BoatTestPlayer : MonoBehaviour
{
    [SerializeField] private bool startFlying = true;
    [SerializeField] private Transform resetTarget;
    [SerializeField] private Vector3 resetLocalPosition;
    [SerializeField] private float flySpeed = 4f;
    private PlayerController player;
    private CharacterController character;
    private Animator animator;
    private BoatBoarding boarding;
    private Camera view;
    private Transform cameraParent;
    private Vector3 cameraLocalPosition;
    private Quaternion cameraLocalRotation;
    private Vector3 initialPosition;
    private Quaternion initialRotation;
    private bool flying;
    private float yaw, pitch;

    private void Awake()
    {
        player = GetComponent<PlayerController>();
        character = GetComponent<CharacterController>();
        animator = GetComponent<Animator>();
        boarding = GetComponent<BoatBoarding>();
        view = GetComponentInChildren<Camera>();
        cameraParent = view.transform.parent;
        cameraLocalPosition = view.transform.localPosition;
        cameraLocalRotation = view.transform.localRotation;
        initialPosition = transform.position;
        initialRotation = transform.rotation;
    }

    private void Start() => SetFlying(startFlying);

    public void SetFlying(bool value)
    {
        if (flying == value && player.enabled == !value) return;
        flying = value;
        if (value)
        {
            player.enabled = false;
            character.enabled = false;
            animator.enabled = false;
            view.transform.SetParent(transform, true);
            yaw = view.transform.eulerAngles.y;
            pitch = Mathf.DeltaAngle(0f, view.transform.eulerAngles.x);
            Cursor.lockState = CursorLockMode.Locked;
            Cursor.visible = false;
        }
        else
        {
            view.transform.SetParent(cameraParent, false);
            view.transform.SetLocalPositionAndRotation(cameraLocalPosition, cameraLocalRotation);
            animator.enabled = true;
            character.enabled = true;
            player.enabled = true;
        }
    }

    public void ResetPlayer()
    {
        SetFlying(true);
        transform.SetPositionAndRotation(resetTarget != null ? resetTarget.TransformPoint(resetLocalPosition) : initialPosition,
            resetTarget != null ? resetTarget.rotation : initialRotation);
        yaw = transform.eulerAngles.y;
        pitch = 0f;
        view.transform.rotation = Quaternion.Euler(pitch, yaw, 0f);
        if (resetTarget != null) SetFlying(false);
    }

    private void Update()
    {
        var keys = Keyboard.current;
        if (keys == null) return;
        if (keys.fKey.wasPressedThisFrame) SetFlying(!flying);
        if (keys.rKey.wasPressedThisFrame) ResetPlayer();
        if (keys.escapeKey.wasPressedThisFrame) { Cursor.lockState = CursorLockMode.None; Cursor.visible = true; }
        var mouse = Mouse.current;
        if (mouse != null && mouse.leftButton.wasPressedThisFrame) { Cursor.lockState = CursorLockMode.Locked; Cursor.visible = false; }
        if (!flying || Cursor.lockState != CursorLockMode.Locked) return;
        if (mouse != null)
        {
            var delta = mouse.delta.ReadValue() * .12f;
            yaw += delta.x;
            pitch = Mathf.Clamp(pitch - delta.y, -85f, 85f);
            view.transform.rotation = Quaternion.Euler(pitch, yaw, 0f);
        }
        float x = (keys.dKey.isPressed ? 1f : 0f) - (keys.aKey.isPressed ? 1f : 0f);
        float z = (keys.wKey.isPressed ? 1f : 0f) - (keys.sKey.isPressed ? 1f : 0f);
        float y = (keys.eKey.isPressed ? 1f : 0f) - (keys.qKey.isPressed ? 1f : 0f);
        var movement = view.transform.right * x + view.transform.forward * z + Vector3.up * y;
        transform.position += Vector3.ClampMagnitude(movement, 1f) * (flySpeed * (keys.leftShiftKey.isPressed ? 3f : 1f) * Time.unscaledDeltaTime);
    }

    private void OnGUI()
    {
        string status = flying ? "inspection flight" : boarding != null && boarding.State != BoatBoarding.BoardingState.Idle
            ? boarding.State + " — " + boarding.CurrentSeat?.name : "walking";
        GUI.Box(new Rect(12, Screen.height - 86, 700, 74),
            $"Boat test — {status}\nF: fly / walk   R: return to boat / spawn   WASD: move   Q/E: fly down/up   Esc: cursor\nE: approach nearest free seat / leave seat   Move or jump: cancel approach");
    }

    private void OnDestroy()
    {
        // Camera is parented within this player in both modes; no scene object is left behind.
        Cursor.lockState = CursorLockMode.None;
        Cursor.visible = true;
    }
}
