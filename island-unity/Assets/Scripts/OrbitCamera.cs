using UnityEngine;

public sealed class OrbitCamera : MonoBehaviour
{
    private const float MinimumPitch = 8f;
    private const float MaximumPitch = 85f;

    private Vector3 target;
    private float distance;
    private float minimumDistance;
    private float maximumDistance;
    private float yaw = 35f;
    private float pitch = 42f;

    public void Configure(Vector3 initialTarget, float initialDistance)
    {
        target = initialTarget;
        distance = initialDistance;
        minimumDistance = initialDistance * 0.04f;
        maximumDistance = initialDistance * 3f;
        ApplyTransform();
    }

    public void OrbitByDegrees(float yawDegrees, float pitchDegrees = 0f)
    {
        yaw += yawDegrees;
        pitch = Mathf.Clamp(pitch + pitchDegrees, MinimumPitch, MaximumPitch);
        ApplyTransform();
    }

    public void ResetOrientation()
    {
        yaw = 35f;
        pitch = 42f;
        ApplyTransform();
    }

    private void LateUpdate()
    {
        if (Input.GetMouseButton(0))
        {
            yaw += Input.GetAxis("Mouse X") * 4f;
            pitch = Mathf.Clamp(
                pitch - Input.GetAxis("Mouse Y") * 4f,
                MinimumPitch,
                MaximumPitch);
        }

        if (Input.GetMouseButton(1))
        {
            var panScale = distance * 0.0025f;
            target -= transform.right * (Input.GetAxis("Mouse X") * panScale);
            target -= transform.up * (Input.GetAxis("Mouse Y") * panScale);
        }

        distance = Mathf.Clamp(
            distance * Mathf.Exp(-Input.mouseScrollDelta.y * 0.12f),
            minimumDistance,
            maximumDistance);
        ApplyTransform();
    }

    private void ApplyTransform()
    {
        var rotation = Quaternion.Euler(pitch, yaw, 0f);
        transform.SetPositionAndRotation(target - rotation * Vector3.forward * distance, rotation);
    }
}
