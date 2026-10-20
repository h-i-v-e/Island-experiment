using UnityEngine;

// Supplies intent to the same animation/root-motion motor used by the player.
// Destination steering is direct; a pathfinding system can supply waypoints.
[DefaultExecutionOrder(-10)]
[DisallowMultipleComponent]
[RequireComponent(typeof(PlayerController), typeof(BoatBoarding))]
public sealed class NpcController : MonoBehaviour
{
    [SerializeField, Min(.05f)] private float stoppingDistance = .2f;
    [SerializeField] private Transform moveTarget;
    [SerializeField] private bool run;
    private PlayerController motor;
    private BoatBoarding boarding;
    private Vector3 destination;
    private bool hasDestination, following;
    private Vector2 rowingInput;

    public bool HasDestination => hasDestination;
    public BoatBoarding Boarding => boarding;

    private void Awake()
    {
        motor = GetComponent<PlayerController>();
        boarding = GetComponent<BoatBoarding>();
        if (motor.UsesPlayerInput)
        {
            Debug.LogError("NPC requires Character control > Control Source = External.", this);
            enabled = false;
        }
    }

    private void Start() { if (moveTarget != null) MoveTo(moveTarget, run); }

    public void MoveTo(Vector3 position, bool running = false)
    {
        if (boarding != null && boarding.State != BoatBoarding.BoardingState.Idle) return;
        moveTarget = null;
        following = false;
        destination = position;
        hasDestination = true;
        run = running;
    }

    public void MoveTo(Transform target, bool running = false)
    {
        if (target == null) { Stop(); return; }
        MoveTo(target.position, running);
        if (!hasDestination) return;
        moveTarget = target;
        following = true;
    }

    public void Stop()
    {
        hasDestination = following = false;
        moveTarget = null;
        rowingInput = Vector2.zero;
        if (motor != null) motor.SetExternalMotion(Vector2.zero, transform.eulerAngles.y);
    }

    public bool BoardSeat(BoatSeat seat, bool fromPort)
    {
        Stop();
        return boarding != null && boarding.TryBoard(seat, fromPort);
    }

    public bool ExitBoat(bool toPort)
    {
        rowingInput = Vector2.zero;
        return boarding != null && boarding.RequestExit(toPort);
    }

    public void SetRowing(Vector2 input) => rowingInput = Vector2.ClampMagnitude(input, 1f);

    private void Update()
    {
        if (boarding.State != BoatBoarding.BoardingState.Idle)
        {
            motor.SetExternalMotion(boarding.CanRow ? rowingInput : Vector2.zero, transform.eulerAngles.y);
            return;
        }
        if (following)
        {
            if (moveTarget == null) { Stop(); return; }
            destination = moveTarget.position;
        }
        if (!hasDestination) { motor.SetExternalMotion(Vector2.zero, transform.eulerAngles.y); return; }
        var delta = Vector3.ProjectOnPlane(destination - transform.position, Vector3.up);
        if (delta.magnitude <= stoppingDistance)
        {
            if (!following) hasDestination = false;
            motor.SetExternalMotion(Vector2.zero, transform.eulerAngles.y);
            return;
        }
        float heading = Quaternion.LookRotation(delta).eulerAngles.y;
        // Keep the existing directional clips moving toward the destination
        // while the shared motor turns the body to face it.
        var local = transform.InverseTransformDirection(delta.normalized);
        float strength = Mathf.Clamp01((delta.magnitude - stoppingDistance) * 2f + .2f);
        motor.SetExternalMotion(new Vector2(local.x, local.z) * strength, heading, run);
    }

    private void OnDisable() => Stop();
}
