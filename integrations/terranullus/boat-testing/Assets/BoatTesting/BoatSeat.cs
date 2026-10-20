using System.Collections.Generic;
using UnityEngine;

[AddComponentMenu("Boats/Seat")]
[DisallowMultipleComponent]
public sealed class BoatSeat : MonoBehaviour
{
    public enum SeatRole { Rower, Passenger }
    public enum EntryKind { Shore, WaterClimb }

    [Tooltip("Place this marker at the seated hips. Its blue arrow is the character's facing direction.")]
    [SerializeField] private SeatRole role;
    [SerializeField] private EntryKind entry;
    [SerializeField] private BoatBoardingProfile profile;
    [Tooltip("Extra space outside the animated start for the standing character capsule. Blends out during entry.")]
    [SerializeField, Min(0f)] private float approachClearance = .35f;
    [SerializeField] private Transform leftApproach, rightApproach;
    [Header("Seated IK")]
    [SerializeField, Range(0, 1)] private float pelvisIKWeight = 1f;
    [SerializeField, Range(0, 1)] private float handIKWeight = 1f;
    [SerializeField, Range(0, 1)] private float handRotationWeight = 1f;
    [SerializeField, Min(.01f)] private float ikBlendSeconds = .25f;
    [SerializeField] private BoatOar leftHandOar, rightHandOar;
    private BoatBoarding occupant;
    private static readonly HashSet<BoatSeat> seats = new HashSet<BoatSeat>();

    public static IEnumerable<BoatSeat> ActiveSeats => seats;
    public SeatRole Role => role;
    public bool WaterClimb => entry == EntryKind.WaterClimb;
    public BoatBoardingProfile Profile => profile;
    public BoatBoarding Occupant => occupant;
    public bool IsAvailable => isActiveAndEnabled && occupant == null && profile != null
        && profile.Entry(true, WaterClimb).IsValid && profile.Entry(false, WaterClimb).IsValid;
    public Transform LeftApproach => leftApproach;
    public Transform RightApproach => rightApproach;
    public float PelvisIKWeight => pelvisIKWeight;
    public float HandIKWeight => handIKWeight;
    public float HandRotationWeight => handRotationWeight;
    public float IKBlendSeconds => ikBlendSeconds;
    public BoatOar LeftHandOar => leftHandOar;
    public BoatOar RightHandOar => rightHandOar;

    public void ConfigureOars(BoatOar left, BoatOar right) { leftHandOar = left; rightHandOar = right; }

    private void OnEnable() => seats.Add(this);
    private void OnDisable()
    {
        seats.Remove(this);
        if (occupant != null) occupant.CancelBoarding();
    }

    public void Configure(SeatRole seatRole, BoatBoardingProfile animationProfile)
    { role = seatRole; profile = animationProfile; }

    public bool TryReserve(BoatBoarding character)
    {
        if (character == null || !IsAvailable) return false;
        occupant = character;
        return true;
    }

    public void Release(BoatBoarding character) { if (occupant == character) occupant = null; }

    public Pose SeatedRoot(float humanScale) => new Pose(transform.position - transform.rotation * (profile.seatedHips * humanScale), transform.rotation);

    public Pose AnimationStart(bool port, float humanScale)
        => BoatBoardingProfile.Multiply(SeatedRoot(humanScale), BoatBoardingProfile.Inverse(profile.Entry(port, WaterClimb).Evaluate(1f, humanScale)));

    public Pose GeneratedApproach(bool port, float humanScale)
    {
        var pose = AnimationStart(port, humanScale);
        float side = Mathf.Sign(Vector3.Dot(pose.position - transform.position, transform.right));
        pose.position += transform.right * (side * approachClearance);
        return pose;
    }

    public Pose Approach(bool port, float humanScale)
    {
        var marker = port ? leftApproach : rightApproach;
        var generated = GeneratedApproach(port, humanScale);
        if (marker == null) return generated;
        // A designer can move a generated marker for clearance. Preserve that
        // adjustment while correcting the animation travel for another avatar size.
        var reference = GeneratedApproach(port, profile.referenceHumanScale);
        generated.position += marker.position - reference.position;
        generated.rotation = marker.rotation * Quaternion.Inverse(reference.rotation) * generated.rotation;
        return generated;
    }

    [ContextMenu("Generate Left And Right Approach Points")]
    public void GenerateApproachPoints()
    {
        if (profile == null || !profile.Entry(true, WaterClimb).IsValid || !profile.Entry(false, WaterClimb).IsValid) return;
        void Place(ref Transform marker, string label, bool port)
        {
            if (marker == null)
            {
                marker = transform.Find(label);
                if (marker == null) { marker = new GameObject(label).transform; marker.SetParent(transform, false); }
            }
            var pose = GeneratedApproach(port, profile.referenceHumanScale);
            marker.SetPositionAndRotation(pose.position, pose.rotation);
        }
        Place(ref leftApproach, "Left approach", true);
        Place(ref rightApproach, "Right approach", false);
    }

    private void OnDrawGizmos()
    {
        Gizmos.color = role == SeatRole.Rower ? Color.cyan : new Color(1, .75f, .2f);
        Gizmos.DrawWireCube(transform.position, Vector3.one * .18f);
        Gizmos.DrawRay(transform.position, transform.forward * .55f);
        if (profile == null) return;
        foreach (bool port in new[] { true, false })
        {
            if (!profile.Entry(port, WaterClimb).IsValid) continue;
            var approach = Approach(port, profile.referenceHumanScale);
            Gizmos.color = port ? new Color(.3f, 1, .4f) : new Color(1, .4f, .3f);
            Gizmos.DrawWireSphere(approach.position, .12f);
            Gizmos.DrawRay(approach.position, approach.rotation * Vector3.forward * .4f);
            Gizmos.DrawLine(approach.position, transform.position);
        }
    }
}
