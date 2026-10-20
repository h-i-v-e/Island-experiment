using Motu.Gameplay;
using UnityEngine;

[AddComponentMenu("Boats/Rowboat Controller")]
[DisallowMultipleComponent]
[RequireComponent(typeof(FloatingShip))]
[DefaultExecutionOrder(-20)]
public sealed class RowboatController : MonoBehaviour
{
    [SerializeField] private BoatSeat commandSeat;
    [SerializeField] private BoatSeat[] rowerSeats;
    [Header("Rowing")]
    [SerializeField, Min(0)] private float forcePerRowerNewtons = 600;
    [SerializeField, Min(.1f)] private float maximumForwardSpeed = 3;
    [SerializeField, Min(.01f)] private float inputResponseSeconds = .2f;
    [Tooltip("Variation of thrust across each rowing animation cycle. Zero gives steady thrust.")]
    [SerializeField, Range(0, 1)] private float strokePulsing = .5f;
    [Header("Steering")]
    [SerializeField, Min(0)] private float turnRateDegrees = 40;
    [SerializeField, Min(0)] private float turnAccelerationDegrees = 90;
    [SerializeField, Min(.01f)] private float turnResponseSeconds = .6f;
    [Header("Beach holding")]
    [Tooltip("Keep a slow boat at its shore or dock position after disembarking, including while terrain collision is streamed out. Rowing releases it; open-water boats remain free to drift.")]
    [SerializeField] private bool holdOnShore = true;
    [SerializeField, Min(.1f)] private float shoreHoldResponseSeconds = .35f;
    [Header("Wind drift")]
    [Tooltip("Downwind acceleration at 9 m/s wind when the crew is idle, in m/s squared.")]
    [SerializeField, Min(0)] private float windDriftAcceleration = .04f;
    [Tooltip("Maximum idle yaw rate towards the wind's source. Rowing takes priority.")]
    [SerializeField, Min(0)] private float windwardTurnRateDegrees = 8f;
    [SerializeField, Min(.1f)] private float windwardTurnResponseSeconds = 3f;
    private Rigidbody body;
    private OceanBuoyancy buoyancy;
    private Vector2 input;
    private int rowers;
    private Animator strokeAnimator;
    private bool wasCommanded;
    private ShoreContactRelay shoreContacts;
    private Collider shoreSupport;
    private bool shoreHeld;
    private bool shorePushActive;
    private float lastShoreContact = float.NegativeInfinity;
    private Vector3 shorePosition;
    private float shoreHeading;

    public Rigidbody Body => body;
    public BoatSeat CommandSeat => commandSeat;
    public Vector2 Input => input;
    public int ActiveRowers => rowers;
    public bool IsShoreHeld => shoreHeld;
    public bool IsGrounded => shoreSupport != null && shoreSupport.enabled && shoreSupport.gameObject.activeInHierarchy
        && Time.fixedTime - lastShoreContact <= .2f;

    public void BeginShorePush() { shorePushActive = true; shoreHeld = false; }
    public void EndShorePush() => shorePushActive = false;

    public void Configure(BoatSeat passenger, BoatSeat[] crewSeats)
    { commandSeat = passenger; rowerSeats = crewSeats; }

    private void Awake()
    {
        body = GetComponent<FloatingShip>().Hull;
        buoyancy = body != null ? body.GetComponent<OceanBuoyancy>() : null;
        if (body != null)
        {
            // FloatingShip detaches the simulated hull from the visual/seat root.
            // Physics callbacks therefore have to be forwarded from that body.
            shoreContacts = body.gameObject.AddComponent<ShoreContactRelay>();
            shoreContacts.Owner = this;
        }
    }

    private void Update()
    {
        var captain = commandSeat != null ? commandSeat.Occupant : null;
        var controls = captain != null ? captain.GetComponent<PlayerController>() : null;
        bool commanded = captain != null && captain.State == BoatBoarding.BoardingState.Seated
            && controls != null && controls.isActiveAndEnabled && controls.UsesPlayerInput;
        var target = commanded ? controls.MovementInput : Vector2.zero;
        target.y = Mathf.Max(0, target.y);
        input = commanded ? Vector2.MoveTowards(input, target, Time.deltaTime / inputResponseSeconds) : Vector2.zero;
        bool controlCrew = commanded || wasCommanded;
        wasCommanded = commanded;
        rowers = 0; strokeAnimator = null;
        if (rowerSeats == null) return;
        foreach (var seat in rowerSeats)
        {
            var occupant = seat != null ? seat.Occupant : null;
            var npc = occupant != null ? occupant.GetComponent<NpcController>() : null;
            if (npc == null || !npc.isActiveAndEnabled || !occupant.CanRow) continue;
            if (controlCrew) npc.SetRowing(input);
            rowers++;
            if (strokeAnimator == null) strokeAnimator = npc.GetComponent<Animator>();
        }
    }

    private void FixedUpdate()
    {
        UpdateShoreHold();
        UpdateWindDrift();
        if (body == null || body.isKinematic || buoyancy == null || buoyancy.SubmergedFraction < .05f
            || rowers == 0 || input.sqrMagnitude < .0001f || Vector3.Dot(body.rotation * Vector3.up, Vector3.up) < .25f) return;
        var forward = Vector3.ProjectOnPlane(body.rotation * Vector3.forward, Vector3.up).normalized;
        float speed = Vector3.Dot(body.linearVelocity - buoyancy.WaterVelocityAt(body.worldCenterOfMass), forward);
        float speedFactor = Mathf.Clamp01(1 - speed / maximumForwardSpeed);
        float phase = strokeAnimator != null ? strokeAnimator.GetCurrentAnimatorStateInfo(0).normalizedTime : 0;
        float pulse = Mathf.Lerp(1, .35f + .65f * Mathf.Max(0, Mathf.Sin(phase * Mathf.PI * 2)), strokePulsing);
        float stroke = Mathf.Max(input.y, Mathf.Abs(input.x) * .2f);
        body.AddForce(forward * (forcePerRowerNewtons * rowers * stroke * pulse * speedFactor), ForceMode.Force);
        float desiredRate = input.x * turnRateDegrees * Mathf.Deg2Rad;
        float acceleration = Mathf.Clamp((desiredRate - body.angularVelocity.y) / turnResponseSeconds,
            -turnAccelerationDegrees * Mathf.Deg2Rad, turnAccelerationDegrees * Mathf.Deg2Rad);
        body.AddTorque(Vector3.up * acceleration * Mathf.Clamp01(rowers / 2f), ForceMode.Acceleration);
    }

    private void UpdateWindDrift()
    {
        if (body == null || body.isKinematic || IsShoreHeld || shorePushActive || input.sqrMagnitude > .0001f
            || buoyancy == null || buoyancy.SubmergedFraction < .05f
            || Vector3.Dot(body.rotation * Vector3.up, Vector3.up) < .25f) return;
        // Weather direction points downwind; an idle boat's bow faces its source.
        var wind = Shader.GetGlobalVector("_MotuWeatherWind");
        var direction = new Vector3(wind.x, 0, wind.y);
        if (wind.z <= .01f || direction.sqrMagnitude < .0001f) return;
        direction.Normalize();
        var flow = direction * wind.z - Vector3.ProjectOnPlane(body.linearVelocity, Vector3.up);
        if (flow.sqrMagnitude > .0001f)
            body.AddForce(flow.normalized * Mathf.Min(.2f, windDriftAcceleration * flow.sqrMagnitude / 81f), ForceMode.Acceleration);
        var forward = Vector3.ProjectOnPlane(body.rotation * Vector3.forward, Vector3.up).normalized;
        float angle = Vector3.SignedAngle(forward, -direction, Vector3.up) * Mathf.Deg2Rad;
        float response = Mathf.Max(.1f, windwardTurnResponseSeconds);
        float rate = windwardTurnRateDegrees * Mathf.Deg2Rad;
        float targetRate = Mathf.Clamp(angle / response, -rate, rate);
        body.AddTorque(Vector3.up * Mathf.Clamp((targetRate - body.angularVelocity.y) * 4f / response,
            -rate * 4f / response, rate * 4f / response), ForceMode.Acceleration);
    }

    private void UpdateShoreHold()
    {
        if (body == null || !holdOnShore || shorePushActive || input.sqrMagnitude > .0001f)
        { shoreHeld = false; return; }
        // Beaching belongs to the boat, not the streamed collider's lifetime.
        // Keep the saved hold across terrain suspension and collider replacement.
        if (body.isKinematic) return;
        var captain = commandSeat != null ? commandSeat.Occupant : null;
        if (!shoreHeld && shoreSupport != null && shoreSupport.enabled
            && shoreSupport.gameObject.activeInHierarchy && Time.fixedTime - lastShoreContact <= .15f
            && (captain == null || captain.State != BoatBoarding.BoardingState.Seated)
            && Vector3.ProjectOnPlane(body.linearVelocity, Vector3.up).sqrMagnitude <= .75f * .75f)
        {
            shoreHeld = true;
            shorePosition = body.position;
            shoreHeading = body.rotation.eulerAngles.y;
        }
        if (!shoreHeld) return;
        // A soft horizontal hold keeps waves from walking a grounded bow back
        // down the beach. Heave, pitch and roll still use normal buoyancy. Keep
        // the hold through brief wave lift-off until rowing resumes.
        float response = Mathf.Max(.1f, shoreHoldResponseSeconds);
        Vector3 acceleration = Vector3.ProjectOnPlane((shorePosition - body.position) / (response * response)
            - 2f * body.linearVelocity / response, Vector3.up);
        body.AddForce(Vector3.ClampMagnitude(acceleration, 4f), ForceMode.Acceleration);
        float turn = Mathf.DeltaAngle(body.rotation.eulerAngles.y, shoreHeading) * Mathf.Deg2Rad;
        body.AddTorque(Vector3.up * Mathf.Clamp(turn / (response * response)
            - 2f * body.angularVelocity.y / response, -2f, 2f), ForceMode.Acceleration);
    }

    private void RecordShoreContact(Collision collision)
    {
        var support = collision.collider;
        // Other boats, characters and vertical bumps cannot beach the hull.
        if (!isActiveAndEnabled || support == null || support.attachedRigidbody != null
            || support is CharacterController || support.GetComponentInParent<FloatingShip>() != null) return;
        for (int i = 0; i < collision.contactCount; i++)
        {
            var contact = collision.GetContact(i);
            if (contact.normal.y < .5f) continue;
            shoreSupport = support;
            lastShoreContact = Time.fixedTime;
            return;
        }
    }

    private sealed class ShoreContactRelay : MonoBehaviour
    {
        public RowboatController Owner;
        private void OnCollisionEnter(Collision collision) { if (Owner != null) Owner.RecordShoreContact(collision); }
        private void OnCollisionStay(Collision collision) { if (Owner != null) Owner.RecordShoreContact(collision); }
    }

    private void OnDestroy() { if (shoreContacts != null) Destroy(shoreContacts); }

    private void OnDisable()
    {
        shoreHeld = false;
        shorePushActive = false;
        shoreSupport = null;
        lastShoreContact = float.NegativeInfinity;
        input = Vector2.zero; rowers = 0;
        bool releaseCrew = wasCommanded;
        wasCommanded = false;
        if (!releaseCrew) return;
        if (rowerSeats == null) return;
        foreach (var seat in rowerSeats)
            if (seat != null && seat.Occupant != null) seat.Occupant.GetComponent<NpcController>()?.SetRowing(Vector2.zero);
    }
}
