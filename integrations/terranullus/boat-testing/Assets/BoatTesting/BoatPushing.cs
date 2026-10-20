using UnityEngine;
using UnityEngine.InputSystem;

[AddComponentMenu("Boats/Character Boat Pushing")]
[DisallowMultipleComponent]
[RequireComponent(typeof(PlayerController), typeof(BoatBoarding))]
[DefaultExecutionOrder(-65)]
public sealed class BoatPushing : MonoBehaviour, IKConstraint
{
    public enum PushState { Idle, Pushing }
    [SerializeField, Min(.1f)] private float interactionRange = 1.35f;
    [SerializeField, Range(0, 1)] private float contactStart = .18f;
    [SerializeField, Range(0, 1)] private float contactEnd = .78f;
    [SerializeField, Min(.1f)] private float maximumFollowSpeed = 2f;
    [Tooltip("Palm collision radius. Contact probes follow the animated hands, independently of IK.")]
    [SerializeField, Range(.01f, .1f)] private float palmRadius = .045f;
    [Tooltip("Small contact tolerance in front of each animated palm.")]
    [SerializeField, Range(0, .15f)] private float contactTolerance = .06f;
    private PlayerController player;
    private BoatBoarding boarding;
    private CharacterController character;
    private Animator animator;
    private BoatPushable boat;
    private BoatPushable pendingLaunch;
    private float launchExpires, afloatSince = -1;
    private PushState state;
    private float started, verticalSpeed;
    private bool animationSeen, hasPreviousPalms;
    private Vector3 previousLeft, previousRight;
    private readonly RaycastHit[] supportHits = new RaycastHit[16];
    private static readonly int PushTrigger = Animator.StringToHash("PushOutBoat");
    public PushState State => state;
    public bool IsActive => state != PushState.Idle;
    public bool IsAnimating => state == PushState.Pushing;
    public bool IsAwaitingLaunch => pendingLaunch != null;
    public float ContactWeight { get; private set; }
    public int ForceSteps { get; private set; }
    public int LeftForceSteps { get; private set; }
    public int RightForceSteps { get; private set; }
    public BoatPushable.HandContact LeftContact { get; private set; }
    public BoatPushable.HandContact RightContact { get; private set; }

    private void Awake()
    {
        player = GetComponent<PlayerController>(); boarding = GetComponent<BoatBoarding>();
        character = GetComponent<CharacterController>(); animator = GetComponent<Animator>();
    }
    private bool CanStart => isActiveAndEnabled && player.isActiveAndEnabled && player.UsesPlayerInput
        && character.enabled && HasFootSupport() && animator.isActiveAndEnabled && !IsActive
        && boarding.State == BoatBoarding.BoardingState.Idle && player.IKConstraint == null;
    private bool HasFootSupport()
    {
        if (character.isGrounded) return true;
        var feet = transform.TransformPoint(character.center - Vector3.up * (character.height * .5f));
        int count = Physics.RaycastNonAlloc(feet + Vector3.up * .15f, Vector3.down, supportHits, .3f,
            Physics.DefaultRaycastLayers, QueryTriggerInteraction.Ignore);
        if (count == supportHits.Length) return false;
        for (int i = 0; i < count; i++)
        {
            var hit = supportHits[i];
            if (hit.normal.y >= .5f && !(hit.collider is CharacterController)
                && hit.collider.GetComponentInParent<FloatingShip>() == null) return true;
        }
        return false;
    }
    private bool FindNearest(out BoatPushable nearest)
    {
        nearest = null;
        if (!CanStart) return false;
        float best = interactionRange;
        foreach (var candidate in BoatPushable.Active)
        {
            if (!candidate.IsAvailable || player.IsSupportedBy(candidate.transform)
                || !candidate.IsInFrontOf(transform.position, transform.forward, interactionRange, out float gap)) continue;
            if (gap < best) { nearest = candidate; best = gap; }
        }
        return nearest != null;
    }
    public bool TryPushNearest()
    {
        if (!FindNearest(out var nearest) || !nearest.TryReserve(this)) return false;
        ClearPendingLaunch();
        boat = nearest; state = PushState.Pushing; started = Time.time; verticalSpeed = -2;
        animationSeen = hasPreviousPalms = false;
        ForceSteps = LeftForceSteps = RightForceSteps = 0; LeftContact = RightContact = default;
        player.IKConstraint = this;
        animator.ResetTrigger("Jump"); animator.ResetTrigger("Land"); animator.SetTrigger(PushTrigger);
        return true;
    }
    private void OnGUI()
    {
        if (!player.UsesPlayerInput) return;
        string key = player.Interact != null ? player.Interact.GetBindingDisplayString() : "Use";
        if (IsActive) GUI.Box(new Rect((Screen.width - 360) * .5f, Screen.height - 108, 360, 32), key + " — Cancel boat push");
        else if (FindNearest(out _)) GUI.Box(new Rect((Screen.width - 360) * .5f, Screen.height - 108, 360, 32), key + " — Push grounded boat");
    }
    private void Update()
    {
        if (player.UsePressed)
        {
            if (IsActive) { player.TryConsumeUse(); CancelPush(); return; }
            ClearPendingLaunch();
            if (TryPushNearest()) player.TryConsumeUse();
        }
        if (!IsActive) { TryBoardLaunchedBoat(); return; }
        if (!player.isActiveAndEnabled || !character.enabled || !animator.isActiveAndEnabled || boat == null
            || !boat.isActiveAndEnabled || boat.Body == null || boat.Body.isKinematic
            || boarding.State != BoatBoarding.BoardingState.Idle || player.MovementInput.sqrMagnitude > .1f)
        { CancelPush(); return; }
        if (TryBoardLaunchedBoat()) return;
        if (!HasFootSupport()) { CancelPush(); return; }
        bool inPush = TryAnimationTime(out float time); animationSeen |= inPush;
        if (Time.time - started > 8f || !animationSeen && Time.time - started > 2f) { CancelPush(); return; }
        if (animationSeen && (!inPush || time >= .98f))
        {
            var launched = ForceSteps > 0 ? boat : null;
            float since = afloatSince;
            CancelPush();
            // The last shove may carry the hull off the beach after the clip ends.
            pendingLaunch = launched; launchExpires = Time.time + 10f; afloatSince = since;
            return;
        }
        ContactWeight = inPush ? Mathf.SmoothStep(0, 1, Mathf.InverseLerp(.05f, contactStart, time))
            * (1f - Mathf.SmoothStep(0, 1, Mathf.InverseLerp(contactEnd, .95f, time))) : 0;
    }
    private void ClearPendingLaunch() { pendingLaunch = null; afloatSince = -1; }
    private bool TryBoardLaunchedBoat()
    {
        var launched = IsActive && ForceSteps > 0 ? boat : pendingLaunch;
        if (launched == null) return false;
        if (!launched.isActiveAndEnabled || !player.isActiveAndEnabled || !player.UsesPlayerInput
            || !animator.isActiveAndEnabled || !character.enabled || player.MovementInput.sqrMagnitude > .1f
            || boarding.State != BoatBoarding.BoardingState.Idle || !IsActive && Time.time > launchExpires
            || player.IKConstraint != null && !ReferenceEquals(player.IKConstraint, this))
        { ClearPendingLaunch(); return false; }
        if (!launched.IsAfloat) { afloatSince = -1; return false; }
        if (afloatSince < 0) afloatSince = Time.time;
        // Ignore a brief lift from a wave; the boat must actually clear shore.
        if (Time.time - afloatSince < .15f) return false;
        var seat = launched.ControlSeat;
        if (seat == null || !seat.IsAvailable) { ClearPendingLaunch(); return false; }
        bool fromPort = launched.transform.InverseTransformPoint(transform.position).x < 0;
        CancelPush();
        // Reuse normal seat reservation, seated animation and moving-boat support.
        return boarding.TrySeatImmediately(seat, fromPort);
    }
    private bool TryAnimationTime(out float time)
    {
        var current = animator.GetCurrentAnimatorStateInfo(0);
        var next = animator.IsInTransition(0) ? animator.GetNextAnimatorStateInfo(0) : default;
        bool currentPush = current.shortNameHash == PushTrigger, nextPush = next.shortNameHash == PushTrigger;
        time = currentPush ? current.normalizedTime : nextPush ? next.normalizedTime : 0;
        return currentPush || nextPush;
    }
    private void FixedUpdate()
    {
        if (!IsAnimating || boat == null || !HasFootSupport() || !TryAnimationTime(out float time)
            || time < contactStart || time > contactEnd) return;
        float strength = Mathf.Sin(Mathf.InverseLerp(contactStart, contactEnd, time) * Mathf.PI);
        bool left = boat.ApplyPush(this, LeftContact, strength), right = boat.ApplyPush(this, RightContact, strength);
        if (left) LeftForceSteps++;
        if (right) RightForceSteps++;
        if (left || right) ForceSteps++;
    }
    public void Move(CharacterController controller)
    {
        if (!IsAnimating) return;
        // The clip drives the player's own motion through normal capsule
        // collisions. There is no boat attachment or generated standing target.
        var delta = Vector3.ClampMagnitude(Vector3.ProjectOnPlane(animator.deltaPosition, Vector3.up), maximumFollowSpeed * Time.deltaTime);
        verticalSpeed = controller.isGrounded && verticalSpeed < 0 ? -2 : verticalSpeed + Physics.gravity.y * Time.deltaTime;
        delta.y = verticalSpeed * Time.deltaTime; controller.Move(delta);
    }
    public void IK(Animator target, float weight)
    {
        var lastLeft = LeftContact; var lastRight = RightContact;
        LeftContact = RightContact = default;
        if (!IsAnimating || boat == null || !TryAnimationTime(out float time)) return;
        BoatPushable.HandContact Hand(AvatarIKGoal goal, HumanBodyBones wristBone, HumanBodyBones fingerBone,
            HumanBodyBones upperBone, HumanBodyBones lowerBone, BoatPushable.HandContact last, ref Vector3 previous)
        {
            var wrist = target.GetBoneTransform(wristBone); var finger = target.GetBoneTransform(fingerBone);
            var palm = finger != null ? Vector3.Lerp(wrist.position, finger.position, .65f) : target.GetIKPosition(goal);
            var goalPosition = target.GetIKPosition(goal); var goalRotation = target.GetIKRotation(goal);
            bool touching = boat.TryHandContact(palm, previous, hasPreviousPalms, transform.forward, palmRadius, contactTolerance, out var contact);
            // Once a palm touches, let IK maintain that physical contact while
            // the animation presses. Release if the arm cannot reach or pulls away.
            if (!touching && last.IsValid && time < contactEnd)
            {
                var upper = target.GetBoneTransform(upperBone); var lower = target.GetBoneTransform(lowerBone);
                float armReach = Vector3.Distance(upper.position, lower.position) + Vector3.Distance(lower.position, wrist.position)
                    + Vector3.Distance(wrist.position, palm);
                bool withdrawing = Vector3.Dot(palm - previous, last.Normal) > .025f;
                if (!withdrawing && Vector3.Distance(upper.position, last.Position) <= armReach + .02f
                    && Vector3.Distance(palm, last.Position) < .45f)
                { touching = true; contact = last; }
            }
            previous = palm;
            if (!touching || ContactWeight <= 0) return default;
            var palmOffset = Quaternion.Inverse(goalRotation) * (palm - goalPosition);
            var alignment = Quaternion.FromToRotation(transform.forward, -Vector3.ProjectOnPlane(contact.Normal, Vector3.up).normalized);
            var rotation = alignment * goalRotation;
            target.SetIKPositionWeight(goal, weight * ContactWeight);
            target.SetIKRotationWeight(goal, weight * ContactWeight);
            target.SetIKPosition(goal, contact.Position + contact.Normal * .015f - rotation * palmOffset);
            target.SetIKRotation(goal, rotation);
            return contact;
        }
        LeftContact = Hand(AvatarIKGoal.LeftHand, HumanBodyBones.LeftHand, HumanBodyBones.LeftMiddleProximal,
            HumanBodyBones.LeftUpperArm, HumanBodyBones.LeftLowerArm, lastLeft, ref previousLeft);
        RightContact = Hand(AvatarIKGoal.RightHand, HumanBodyBones.RightHand, HumanBodyBones.RightMiddleProximal,
            HumanBodyBones.RightUpperArm, HumanBodyBones.RightLowerArm, lastRight, ref previousRight);
        hasPreviousPalms = true;
    }
    private void LateUpdate()
    {
        // Verify the solved palms as well as the animation probes. An unreachable
        // IK target or a hand that has left the hull cannot keep supplying force.
        BoatPushable.HandContact Verify(BoatPushable.HandContact contact, HumanBodyBones wrist, HumanBodyBones finger)
        {
            if (!contact.IsValid) return default;
            var hand = animator.GetBoneTransform(wrist); var digits = animator.GetBoneTransform(finger);
            var palm = digits != null ? Vector3.Lerp(hand.position, digits.position, .65f) : hand.position;
            return Vector3.Distance(palm, contact.Position) <= .08f ? contact.WithPalm(palm) : default;
        }
        LeftContact = Verify(LeftContact, HumanBodyBones.LeftHand, HumanBodyBones.LeftMiddleProximal);
        RightContact = Verify(RightContact, HumanBodyBones.RightHand, HumanBodyBones.RightMiddleProximal);
    }
    public void CancelPush()
    {
        ClearPendingLaunch();
        if (!IsActive) return;
        if (boat != null) boat.Release(this);
        if (ReferenceEquals(player.IKConstraint, this)) player.IKConstraint = null;
        animator.ResetTrigger(PushTrigger);
        if (animator.isActiveAndEnabled) animator.CrossFade("Base Layer.Blend Tree", .15f, 0);
        state = PushState.Idle; ContactWeight = 0; boat = null; LeftContact = RightContact = default;
        player.ResetBoatSupport();
    }
    private void OnDisable() => CancelPush();
}
