using UnityEngine;

namespace Motu.Gameplay
{
    [AddComponentMenu("Motu/Ship Bridge Camera")]
    [RequireComponent(typeof(Camera))]
    [DisallowMultipleComponent]
    [DefaultExecutionOrder(1400)]
    public sealed class ShipBridgeCamera : MonoBehaviour
    {
        [SerializeField] private Transform eyePosition;
        [SerializeField] private Rigidbody ship;
        [Min(0f)] [SerializeField] private float lookSensitivity = 2.2f;
        [SerializeField] private bool captureMouseOnStart = true;
        private float yaw;
        private float pitch;
        public bool IsLooking => isActiveAndEnabled && Cursor.lockState == CursorLockMode.Locked;
        public Transform EyePosition => eyePosition;

        public void Configure(Transform eye, Rigidbody rigidbody)
        {
            eyePosition = eye;
            ship = rigidbody;
            SnapToShip();
        }

        private void Start()
        {
            SnapToShip();
            SetMouseLook(captureMouseOnStart);
        }

        private void Update()
        {
            if (Input.GetKeyDown(KeyCode.Escape)) SetMouseLook(false);
            else if (Input.GetKeyDown(KeyCode.Tab)) SetMouseLook(!IsLooking);
            if (!IsLooking || !Application.isFocused) return;
            yaw = Mathf.Repeat(yaw + Input.GetAxisRaw("Mouse X") * lookSensitivity + 180f, 360f) - 180f;
            pitch = Mathf.Clamp(pitch - Input.GetAxisRaw("Mouse Y") * lookSensitivity, -80f, 80f);
        }

        private void LateUpdate()
        {
            if (eyePosition == null) return;
            // Follow the ship's interpolated render pose, never move its Rigidbody.
            transform.SetPositionAndRotation(eyePosition.position,
                eyePosition.rotation * Quaternion.Euler(pitch, yaw, 0f));
        }

        public void SnapToShip()
        {
            if (eyePosition == null) return;
            if (ship == null) { LateUpdate(); return; }
            var local = ship.transform.InverseTransformPoint(eyePosition.position);
            var rotation = Quaternion.Inverse(ship.transform.rotation) * eyePosition.rotation;
            transform.SetPositionAndRotation(ship.position + ship.rotation * Vector3.Scale(local, ship.transform.lossyScale),
                ship.rotation * rotation * Quaternion.Euler(pitch, yaw, 0));
        }

        public void SetMouseLook(bool active)
        {
            Cursor.lockState = active ? CursorLockMode.Locked : CursorLockMode.None;
            Cursor.visible = !active;
        }

        private void OnApplicationFocus(bool focused) { if (!focused) SetMouseLook(false); }
        private void OnDisable() => SetMouseLook(false);
    }
}
