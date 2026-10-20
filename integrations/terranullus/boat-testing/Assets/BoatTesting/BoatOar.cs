using UnityEngine;

// The transform carrying this component is the fixed rowlock. The model keeps
// its existing prefab parent; only its temporary runtime pose is driven.
[AddComponentMenu("Boats/Oar IK Rig")]
[DisallowMultipleComponent]
public sealed class BoatOar : MonoBehaviour
{
    [SerializeField] private Transform model;
    [SerializeField] private Transform handGrip;
    [Tooltip("Contact point on the oar shaft, fixed to this rig's rowlock in every pose.")]
    [SerializeField] private Transform rowlockPin;
    [SerializeField, Range(0, 180)] private float maximumSwing = 110f;
    [Tooltip("Extra downward blade angle about the fixed rowlock. Raises the handle while keeping the shaft pinned; 0 retains the animation's original dip.")]
    [SerializeField, Range(0, 30)] private float bladeDipDegrees = 12f;
    [SerializeField, Min(.01f)] private float releaseSeconds = .3f;
    private Vector3 restPosition, pivotInModel, gripFromPivot;
    private Quaternion restRotation;
    private bool captured;
    private int drivenFrame = -1;
    private float weight;

    public Transform Model => model;
    public Transform HandGrip => handGrip;
    public Transform RowlockPin => rowlockPin;
    public float BladeDipDegrees { get => bladeDipDegrees; set => bladeDipDegrees = Mathf.Clamp(value, 0, 30); }
    public bool IsValid => model != null && handGrip != null;

    public void Configure(Transform oarModel, Transform grip) { model = oarModel; handGrip = grip; }
    public void ConfigurePin(Transform pin) => rowlockPin = pin;

    private void Capture()
    {
        if (captured || !IsValid) return;
        restPosition = model.localPosition;
        restRotation = model.localRotation;
        var pivot = rowlockPin != null ? rowlockPin.position : transform.position;
        pivotInModel = Quaternion.Inverse(model.rotation) * (pivot - model.position);
        gripFromPivot = Quaternion.Inverse(model.rotation) * (handGrip.position - pivot);
        captured = true;
    }

    public void Drive(Vector3 animatedHand, float blend)
    {
        Capture();
        if (!captured) return;
        drivenFrame = Time.frameCount;
        weight = Mathf.Clamp01(blend);
        var parent = model.parent;
        var restWorldRotation = parent != null ? parent.rotation * restRotation : restRotation;
        var direction = animatedHand - transform.position;
        if (direction.sqrMagnitude < .000001f || gripFromPivot.sqrMagnitude < .000001f) return;
        var aimed = Quaternion.FromToRotation(restWorldRotation * gripFromPivot, direction) * restWorldRotation;
        aimed = Quaternion.RotateTowards(restWorldRotation, aimed, maximumSwing);
        // The handle is inboard of the pin, so raising it dips the outboard
        // blade. Use boat up, retaining the correct sign on both sides and turns.
        var handleDirection = aimed * gripFromPivot;
        var up = transform.up;
        var pitchAxis = Vector3.Cross(handleDirection, up);
        if (pitchAxis.sqrMagnitude > .000001f)
        {
            float elevation = Mathf.Asin(Mathf.Clamp(Vector3.Dot(handleDirection.normalized, up), -1, 1)) * Mathf.Rad2Deg;
            aimed = Quaternion.AngleAxis(Mathf.Clamp(85f - elevation, 0, bladeDipDegrees), pitchAxis.normalized) * aimed;
        }
        var rotation = Quaternion.Slerp(restWorldRotation, aimed, weight);
        model.rotation = rotation;
        PinToRowlock();
    }

    private void LateUpdate()
    {
        Capture();
        if (!captured || model == null) return;
        if (drivenFrame == Time.frameCount) { PinToRowlock(); return; }
        weight = Mathf.MoveTowards(weight, 0, Time.deltaTime / releaseSeconds);
        model.localRotation = Quaternion.Slerp(model.localRotation, restRotation, weight > 0 ? 1f - Mathf.Exp(-Time.deltaTime * 12) : 1);
        PinToRowlock();
    }

    private void PinToRowlock()
    {
        if (rowlockPin != null) model.position += transform.position - rowlockPin.position;
        else model.position = transform.position - model.rotation * pivotInModel;
    }

    private void OnDisable()
    {
        if (captured && model != null) model.SetLocalPositionAndRotation(restPosition, restRotation);
        captured = false;
        weight = 0;
        drivenFrame = -1;
    }

    private void OnDrawGizmosSelected()
    {
        Gizmos.color = Color.magenta;
        Gizmos.DrawWireSphere(transform.position, .065f);
        if (rowlockPin != null) Gizmos.DrawLine(transform.position, rowlockPin.position);
        if (handGrip == null) return;
        Gizmos.color = Color.yellow;
        Gizmos.DrawWireSphere(handGrip.position, .05f);
        Gizmos.DrawLine(transform.position, handGrip.position);
    }
}
