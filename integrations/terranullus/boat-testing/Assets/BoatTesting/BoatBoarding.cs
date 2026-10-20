using UnityEngine;

[AddComponentMenu("Boats/Character Boarding")]
[DisallowMultipleComponent]
[RequireComponent(typeof(Animator), typeof(CharacterController), typeof(PlayerController))]
[DefaultExecutionOrder(-50)]
public sealed class BoatBoarding : MonoBehaviour
{
    public enum BoardingState { Idle, Approaching, Aligning, Entering, Seated, Exiting }
    [SerializeField, Min(.1f)] private float interactionRange = 4f;
    [SerializeField, Min(.02f)] private float arrivalDistance = .09f;
    [SerializeField, Min(.1f)] private float alignmentSeconds = .25f;
    [SerializeField, Min(.1f)] private float maximumHeightDifference = .75f;
    [SerializeField, Min(1f)] private float approachTimeout = 15f;
    [Tooltip("Extra space beyond the walking hull for the full character capsule at the end of an exit.")]
    [SerializeField, Min(0f)] private float exitHullClearance = .05f;
    private PlayerController player;
    private Animator animator;
    private CharacterController character;
    private BoatSeat seat;
    private bool port, savedCharacterEnabled;
    private BoardingState state;
    private float stateStarted, bestDistance, lastProgress;
    private Vector3 alignmentLocalPosition;
    private Quaternion alignmentLocalRotation;
    private float seatIKBlend;
    private Quaternion leftWristToGoal, rightWristToGoal;
    private bool leftWristAxesCaptured, rightWristAxesCaptured;
    private Transform exitBoat;
    private Collider[] exitHullColliders = System.Array.Empty<Collider>();

    public BoardingState State => state;
    public BoatSeat CurrentSeat => seat;
    public bool PortSide => port;
    public bool ControlsRootMotion => (int)state >= (int)BoardingState.Aligning;
    public bool CanRow => state == BoardingState.Seated && seat != null && seat.Role == BoatSeat.SeatRole.Rower;
    public float SeatIKBlend => seatIKBlend;

    public void ApplySeatIK()
    {
        if (seat == null) { seatIKBlend = 0; return; }
        seatIKBlend = Mathf.MoveTowards(seatIKBlend, state == BoardingState.Seated ? 1 : 0,
            Time.deltaTime / seat.IKBlendSeconds);
        if (seatIKBlend <= 0) return;
        var hips = animator.GetBoneTransform(HumanBodyBones.Hips);
        if (hips == null) return;
        var correction = (seat.transform.position - hips.position) * (seat.PelvisIKWeight * seatIKBlend);
        animator.bodyPosition += correction;
        if (seat.Role != BoatSeat.SeatRole.Rower) return;
        void Hand(BoatOar oar, AvatarIKGoal goal, HumanBodyBones bone)
        {
            if (oar == null || !oar.IsValid) return;
            var animated = animator.GetBoneTransform(bone);
            if (animated == null) return;
            // Grip markers describe the wrist bone's pose. Humanoid IK goals
            // use avatar-dependent palm axes rather than the imported bone axes.
            ref var captured = ref (goal == AvatarIKGoal.LeftHand ? ref leftWristAxesCaptured : ref rightWristAxesCaptured);
            ref var wristToGoal = ref (goal == AvatarIKGoal.LeftHand ? ref leftWristToGoal : ref rightWristToGoal);
            if (!captured)
            {
                wristToGoal = Quaternion.Inverse(animated.rotation) * animator.GetIKRotation(goal);
                captured = true;
            }
            oar.Drive(animated.position + correction, seatIKBlend);
            animator.SetIKPositionWeight(goal, seat.HandIKWeight * seatIKBlend);
            animator.SetIKRotationWeight(goal, seat.HandRotationWeight * seatIKBlend);
            animator.SetIKPosition(goal, oar.HandGrip.position);
            animator.SetIKRotation(goal, oar.HandGrip.rotation * wristToGoal);
        }
        Hand(seat.LeftHandOar, AvatarIKGoal.LeftHand, HumanBodyBones.LeftHand);
        Hand(seat.RightHandOar, AvatarIKGoal.RightHand, HumanBodyBones.RightHand);
    }

    private void Awake()
    {
        player = GetComponent<PlayerController>();
        animator = GetComponent<Animator>();
        character = GetComponent<CharacterController>();
    }

    private void Update()
    {
        if (player == null || !player.enabled || !animator.enabled) { CancelBoarding(); return; }
        if (state != BoardingState.Idle && (seat == null || !seat.isActiveAndEnabled)) { CancelBoarding(); return; }
        if ((state == BoardingState.Approaching || state == BoardingState.Aligning || state == BoardingState.Entering)
            && IsBoatGrounded(seat)) { CancelBoarding(); return; }
        if (player.UsePressed && player.IKConstraint == null)
        {
            if (state == BoardingState.Idle)
            {
                if (ShipRowboatLaunch.TryUseNearest(player)) return;
                if (TryBoardNearest()) player.TryConsumeUse();
            }
            else if (state == BoardingState.Seated)
            { if (RequestExit(port)) player.TryConsumeUse(); }
            else if (state == BoardingState.Approaching)
            { CancelBoarding(); player.TryConsumeUse(); }
        }
        if (state == BoardingState.Approaching)
        {
            var target = seat.Approach(port, animator.humanScale);
            var difference = target.position - transform.position;
            float distance = Vector3.ProjectOnPlane(difference, Vector3.up).magnitude;
            if (distance < bestDistance - .025f) { bestDistance = distance; lastProgress = Time.time; }
            if (Time.time - stateStarted > approachTimeout || Time.time - lastProgress > 3f
                || player.MovementInput.sqrMagnitude > .1f) { CancelBoarding(); return; }
            if (distance <= arrivalDistance && Mathf.Abs(difference.y) <= maximumHeightDifference)
            {
                alignmentLocalPosition = seat.transform.InverseTransformPoint(transform.position);
                alignmentLocalRotation = Quaternion.Inverse(seat.transform.rotation) * transform.rotation;
                savedCharacterEnabled = character.enabled;
                character.enabled = false;
                state = BoardingState.Aligning;
                stateStarted = Time.time;
            }
        }
        else if (ControlsRootMotion && Time.time - stateStarted > 12f && state != BoardingState.Seated)
            CancelBoarding();
    }

    public bool TryBoardNearest()
    {
        BoatSeat nearest = null;
        bool nearestPort = true;
        float distance = interactionRange * interactionRange;
        foreach (var candidate in BoatSeat.ActiveSeats)
        {
            if (!candidate.IsAvailable || IsBoatGrounded(candidate)) continue;
            foreach (bool side in new[] { true, false })
            {
                float squared = (candidate.Approach(side, animator.humanScale).position - transform.position).sqrMagnitude;
                if (squared < distance) { nearest = candidate; nearestPort = side; distance = squared; }
            }
        }
        return nearest != null && TryBoard(nearest, nearestPort);
    }

    public bool TryBoard(BoatSeat target, bool fromPort)
    {
        if (!isActiveAndEnabled || state != BoardingState.Idle || target == null || player == null
            || !player.enabled || player.IKConstraint != null || IsBoatGrounded(target) || !target.TryReserve(this)) return false;
        seat = target;
        port = fromPort;
        state = BoardingState.Approaching;
        stateStarted = lastProgress = Time.time;
        bestDistance = float.MaxValue;
        return true;
    }

    // Launching from a ship skips the shore climb but retains reservations,
    // moving-boat tracking, collision suspension, IK and normal exit behaviour.
    public bool TrySeatImmediately(BoatSeat target, bool fromPort)
    {
        if (!isActiveAndEnabled || state != BoardingState.Idle || target == null || player == null
            || !player.enabled || player.IKConstraint != null || IsBoatGrounded(target) || !target.TryReserve(this)) return false;
        seat = target;
        port = fromPort;
        savedCharacterEnabled = character.enabled;
        character.enabled = false;
        player.ResetBoatSupport();
        state = BoardingState.Seated;
        stateStarted = Time.time;
        seatIKBlend = 1;
        animator.SetBool("PortSide", port);
        animator.SetBool("ClimbFromWater", false);
        animator.SetBool("Passenger", seat.Role == BoatSeat.SeatRole.Passenger);
        animator.SetInteger("Locamotion", 1);
        animator.SetFloat("XAxis", 0); animator.SetFloat("YAxis", 0); animator.SetFloat("SideStep", 0);
        int overlay = animator.GetLayerIndex("Side Step Layer");
        if (overlay >= 0) animator.SetLayerWeight(overlay, 0);
        animator.ResetTrigger("Jump"); animator.ResetTrigger("Land");
        SetPose(seat.SeatedRoot(animator.humanScale));
        animator.Play(seat.Role == BoatSeat.SeatRole.Rower ? "Base Layer.Rowing.rowing" : "Base Layer.Rowing.passenger", 0, 0);
        animator.Update(0);
        // The explicit zero-time update initializes the seated pose. Capture
        // the avatar's fixed wrist axes on the next normal animation pass,
        // once that pose has reached the skeleton transforms.
        leftWristAxesCaptured = rightWristAxesCaptured = false;
        return true;
    }

    private static bool IsBoatGrounded(BoatSeat target)
    {
        var rowboat = target != null ? target.GetComponentInParent<RowboatController>() : null;
        // A beach hold survives terrain collider unloading, so losing the
        // streamed collider must not turn a beached boat into a boarding target.
        return rowboat != null && (rowboat.IsGrounded || rowboat.IsShoreHeld);
    }

    // Normal character locomotion and collisions handle the approach. Targets
    // are evaluated every frame, so a moving boat cannot leave a stale waypoint.
    public bool TryGetApproachInput(out Vector2 movement, out float heading)
    {
        movement = Vector2.zero;
        heading = transform.eulerAngles.y;
        if (state != BoardingState.Approaching || seat == null) return false;
        var delta = seat.Approach(port, animator.humanScale).position - transform.position;
        delta.y = 0;
        float distance = delta.magnitude;
        var local = Quaternion.Inverse(Quaternion.Euler(0, heading, 0)) * delta;
        movement = Vector2.ClampMagnitude(new Vector2(local.x, local.z) * 2f, 1f);
        // Strafe for the last part while adopting the animation's starting facing.
        heading = distance > .5f ? Quaternion.LookRotation(delta).eulerAngles.y
            : seat.Approach(port, animator.humanScale).rotation.eulerAngles.y;
        return true;
    }

    public bool ApplyAnimatorMotion()
    {
        if (!ControlsRootMotion || seat == null) return false;
        var current = animator.GetCurrentAnimatorStateInfo(0);
        var next = animator.IsInTransition(0) ? animator.GetNextAnimatorStateInfo(0) : default;
        Pose pose;
        if (state == BoardingState.Aligning)
        {
            float blend = Mathf.SmoothStep(0, 1, (Time.time - stateStarted) / alignmentSeconds);
            var target = seat.Approach(port, animator.humanScale);
            pose = new Pose(Vector3.Lerp(seat.transform.TransformPoint(alignmentLocalPosition), target.position, blend),
                Quaternion.Slerp(seat.transform.rotation * alignmentLocalRotation, target.rotation, blend));
            if (blend >= 1)
            {
                animator.SetBool("PortSide", port);
                animator.SetBool("ClimbFromWater", seat.WaterClimb);
                animator.SetBool("Passenger", seat.Role == BoatSeat.SeatRole.Passenger);
                animator.SetInteger("Locamotion", 1);
                state = BoardingState.Entering;
                stateStarted = Time.time;
            }
        }
        else if (state == BoardingState.Entering)
        {
            var motion = seat.Profile.Entry(port, seat.WaterClimb);
            float time = current.IsName(motion.state) ? current.normalizedTime : next.IsName(motion.state) ? next.normalizedTime : 0f;
            if (current.IsName("Base Layer.Rowing.rowing") || current.IsName("Base Layer.Rowing.passenger"))
            { state = BoardingState.Seated; pose = seat.SeatedRoot(animator.humanScale); }
            else
            {
                var sample = motion.Evaluate(time, animator.humanScale);
                var adjusted = BoatBoardingProfile.Multiply(seat.Approach(port, animator.humanScale), sample);
                var exact = BoatBoardingProfile.Multiply(seat.AnimationStart(port, animator.humanScale), sample);
                float correction = Mathf.SmoothStep(0, 1, Mathf.Clamp01(time));
                pose = new Pose(Vector3.Lerp(adjusted.position, exact.position, correction), Quaternion.Slerp(adjusted.rotation, exact.rotation, correction));
            }
        }
        else if (state == BoardingState.Exiting)
        {
            var motion = seat.Profile.Exit(port);
            float time = current.IsName(motion.state) ? current.normalizedTime : next.IsName(motion.state) ? next.normalizedTime : 0f;
            pose = ExitPose(time);
            if (current.IsName("Base Layer.Blend Tree") || current.IsName(motion.state) && time >= 1f)
            {
                pose = ExitPose(1);
                SetPose(pose);
                Release(false);
                return true;
            }
        }
        else pose = seat.SeatedRoot(animator.humanScale);
        SetPose(pose);
        return true;
    }

    private void SetPose(Pose pose)
    {
        float turn = Mathf.DeltaAngle(transform.eulerAngles.y, pose.rotation.eulerAngles.y);
        transform.SetPositionAndRotation(pose.position, pose.rotation);
        player.CarryBoatFacing(turn);
    }

    private Pose ExitPose(float time)
    {
        var root = seat.SeatedRoot(animator.humanScale);
        var motion = seat.Profile.Exit(port);
        var sample = BoatBoardingProfile.Multiply(root, motion.Evaluate(time, animator.humanScale));
        var end = BoatBoardingProfile.Multiply(root, motion.Evaluate(1, animator.humanScale));
        var target = ExitTarget();
        target.rotation *= Quaternion.Euler(0, 180, 0);
        float blend = Mathf.SmoothStep(0, 1, Mathf.Clamp01(time));
        sample.position += (target.position - end.position) * blend;
        sample.rotation = Quaternion.Slerp(sample.rotation, target.rotation * Quaternion.Inverse(end.rotation) * sample.rotation, blend);
        return sample;
    }

    private Pose ExitTarget()
    {
        var target = seat.Approach(port, animator.humanScale);
        if (exitBoat == null) return target;
        var outward = Vector3.ProjectOnPlane(exitBoat.right, Vector3.up);
        if (outward.sqrMagnitude < .000001f) return target;
        outward.Normalize();
        if (Vector3.Dot(target.position - exitBoat.position, outward) < 0) outward = -outward;
        float hullPastTarget = float.NegativeInfinity;
        foreach (var shape in exitHullColliders)
        {
            if (shape == null || !shape.enabled || !shape.gameObject.activeInHierarchy || shape.isTrigger
                || shape is CharacterController || Physics.GetIgnoreCollision(character, shape)) continue;
            Bounds bounds;
            if (shape is MeshCollider mesh && mesh.sharedMesh != null) bounds = mesh.sharedMesh.bounds;
            else if (shape is BoxCollider box) bounds = new Bounds(box.center, box.size);
            else
            {
                bounds = shape.bounds;
                float radius = Vector3.Dot(bounds.extents, new Vector3(Mathf.Abs(outward.x), Mathf.Abs(outward.y), Mathf.Abs(outward.z)));
                hullPastTarget = Mathf.Max(hullPastTarget, Vector3.Dot(bounds.center - target.position, outward) + radius);
                continue;
            }
            var model = shape.transform;
            var extent = bounds.extents;
            float projectedRadius = Mathf.Abs(Vector3.Dot(model.TransformVector(Vector3.right * extent.x), outward))
                + Mathf.Abs(Vector3.Dot(model.TransformVector(Vector3.up * extent.y), outward))
                + Mathf.Abs(Vector3.Dot(model.TransformVector(Vector3.forward * extent.z), outward));
            hullPastTarget = Mathf.Max(hullPastTarget, Vector3.Dot(model.TransformPoint(bounds.center) - target.position, outward) + projectedRadius);
        }
        if (float.IsNegativeInfinity(hullPastTarget)) return target;
        var scale = character.transform.lossyScale;
        float capsuleRadius = character.radius * Mathf.Max(Mathf.Abs(scale.x), Mathf.Abs(scale.z));
        float clearance = capsuleRadius + character.skinWidth + exitHullClearance;
        // A clear centre point is insufficient: the capsule's rounded bottom
        // can balance on the gunwale and look as though it is standing on air.
        // Re-evaluate as the boat rolls, blending this offset into the climb.
        target.position += outward * Mathf.Max(0, hullPastTarget + clearance);
        return target;
    }

    public bool RequestExit(bool toPort)
    {
        if (state != BoardingState.Seated || seat == null || !seat.Profile.Exit(toPort).IsValid) return false;
        port = toPort;
        var floating = seat.GetComponentInParent<FloatingShip>();
        exitBoat = floating != null ? floating.transform : null;
        exitHullColliders = floating != null ? floating.GetComponentsInChildren<Collider>() : System.Array.Empty<Collider>();
        animator.SetBool("PortSide", port);
        animator.SetInteger("Locamotion", 0);
        state = BoardingState.Exiting;
        stateStarted = Time.time;
        return true;
    }

    public void CancelBoarding() { if (state != BoardingState.Idle) Release(true); }

    private void Release(bool cancelAnimation)
    {
        bool restore = ControlsRootMotion;
        var hull = seat != null ? seat.GetComponentInParent<FloatingShip>()?.Hull : null;
        var velocity = hull != null ? hull.GetPointVelocity(transform.position) : Vector3.zero;
        if (seat != null) seat.Release(this);
        seat = null;
        seatIKBlend = 0;
        state = BoardingState.Idle;
        exitBoat = null;
        exitHullColliders = System.Array.Empty<Collider>();
        if (restore)
        {
            // A completed exit returns control to walking physics even when
            // the capsule was temporarily disabled before immediate seating.
            character.enabled = !cancelAnimation || savedCharacterEnabled;
            player.ResumeFromBoat(velocity);
            animator.SetInteger("Locamotion", 0);
            animator.SetBool("Passenger", false);
            if (animator.isActiveAndEnabled)
            {
                if (cancelAnimation) animator.Play("Base Layer.Blend Tree", 0, 0);
                else if (!animator.GetCurrentAnimatorStateInfo(0).IsName("Base Layer.Blend Tree"))
                    animator.CrossFadeInFixedTime("Base Layer.Blend Tree", .1f, 0);
            }
        }
    }

    private void OnDisable() => CancelBoarding();
}
