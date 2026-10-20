using System;
using System.Collections.Generic;
using System.Linq;
using Motu.Gameplay;
using Motu.World;
using UnityEngine;
using UnityEngine.InputSystem;

[AddComponentMenu("Boats/Ship Rowboat Launch")]
[DisallowMultipleComponent]
public sealed class ShipRowboatLaunch : MonoBehaviour
{
    [SerializeField] private ShipController ship;
    [SerializeField] private Transform shipFrame;
    [SerializeField] private Renderer stowedHull;
    [Tooltip("Ship-relative water position. Height is taken from the sea, not the deck.")]
    [SerializeField] private Transform launchPoint;
    [SerializeField] private GameObject rowboatPrefab;
    [SerializeField] private GameObject rowerPrefab;
    [SerializeField] private string passengerSeatName = "Passenger aft";
    [SerializeField, Min(.1f)] private float useDistance = 2.25f;
    [Tooltip("Extra outboard search when the rolling physical ship hull occupies the authored launch point. Other obstacles still reject launch.")]
    [SerializeField, Min(0)] private float maximumHullClearanceAdjustment = 8f;
    [Tooltip("Hull-only bounds in the floating rowboat's root frame; used to reject blocked launches.")]
    [SerializeField] private Bounds clearanceBounds;
    private static readonly HashSet<ShipRowboatLaunch> launches = new HashSet<ShipRowboatLaunch>();
    private readonly Collider[] overlaps = new Collider[64];
    private PlayerController nearbyPlayer;
    private float nextPlayerSearch, feedbackUntil;

    public GameObject LaunchedBoat { get; private set; }
    public string LastFailure { get; private set; }
    public Collider LaunchObstruction { get; private set; }
    public Pose LastLaunchPose { get; private set; }
    public Transform LaunchPoint => launchPoint;
    public Transform ShipFrame => shipFrame;
    public ShipController Ship => ship;
    public bool PortSide => shipFrame != null && shipFrame.InverseTransformPoint(transform.position).x < 0;
    public bool AnchorDeployed => ship != null && ship.GetComponent<ShipAnchor>() is ShipAnchor anchor && anchor.IsDeployed;

    private void OnEnable() => launches.Add(this);
    private void OnDisable() => launches.Remove(this);

    public float DistanceFrom(PlayerController player) => stowedHull != null
        ? Mathf.Sqrt(stowedHull.bounds.SqrDistance(player.transform.position + Vector3.up * .9f))
        : Vector3.Distance(transform.position, player.transform.position + Vector3.up * .9f);

    public static ShipRowboatLaunch Nearest(PlayerController player)
    {
        if (player == null || !player.enabled || !player.UsesPlayerInput || player.IKConstraint != null) return null;
        ShipRowboatLaunch nearest = null;
        float best = float.PositiveInfinity;
        foreach (var launch in launches)
        {
            if (launch == null || !launch.isActiveAndEnabled || launch.LaunchedBoat != null) continue;
            float distance = launch.DistanceFrom(player);
            if (distance <= launch.useDistance && distance < best) { nearest = launch; best = distance; }
        }
        return nearest;
    }

    // Called from the existing use action before ordinary shore-seat boarding.
    // A denied launch also consumes this use, so it cannot activate another seat.
    public static bool TryUseNearest(PlayerController player)
    {
        var nearest = Nearest(player);
        if (nearest == null || !player.TryConsumeUse()) return false;
        nearest.TryLaunch(player);
        return true;
    }

    public Pose WaterPose(OceanSurfaceController ocean)
    {
        var forward = Vector3.ProjectOnPlane(shipFrame.forward, Vector3.up).normalized;
        if (forward.sqrMagnitude < .5f) forward = Vector3.forward;
        var heading = Quaternion.LookRotation(forward);
        var local = shipFrame.InverseTransformPoint(launchPoint.position);
        // Offset in the ship's horizontal heading frame. Projecting the rolled
        // marker straight down can pull a lee-side boat back into the hull.
        var position = shipFrame.position + heading * Vector3.Scale(new Vector3(local.x, 0, local.z), shipFrame.lossyScale);
        position.y = (ocean != null && ocean.SurfaceTransform != null ? ocean.SurfaceTransform.position.y : 0)
            + rowboatPrefab.transform.position.y;
        return new Pose(position, heading * rowboatPrefab.transform.rotation);
    }

    public bool TryLaunch(PlayerController player)
    {
        LastFailure = null;
        LaunchObstruction = null;
        var passenger = player != null ? player.GetComponent<BoatBoarding>() : null;
        if (!isActiveAndEnabled || LaunchedBoat != null || passenger == null || !passenger.enabled
            || !player.enabled || !player.UsesPlayerInput || player.IKConstraint != null
            || passenger.State != BoatBoarding.BoardingState.Idle || DistanceFrom(player) > useDistance)
            return Fail("Move closer to an unused deck rowboat.");
        if (!AnchorDeployed) return Fail("Lower the ship's anchor before launching a rowboat.");
        if (launchPoint == null || shipFrame == null || rowboatPrefab == null || rowerPrefab == null)
            return Fail("Rowboat launch is not configured.");
        var buoyancy = ship.GetComponent<OceanBuoyancy>();
        var ocean = buoyancy != null ? buoyancy.Ocean : null;
        if (ocean == null || !ocean.isActiveAndEnabled || ocean.SurfaceTransform == null)
            return Fail("The sea is not ready for launching.");
        var pose = WaterPose(ocean);
        var shipBody = ship.GetComponent<Rigidbody>();
        if (!FindClearWater(ref pose, shipBody))
            return Fail("There is no clear water beside this side of the ship.");
        LastLaunchPose = pose;

        GameObject boat = null, staging = new GameObject("Preparing launched rowboat");
        staging.SetActive(false);
        var crew = new NpcController[2];
        try
        {
            boat = Instantiate(rowboatPrefab, pose.position, pose.rotation, staging.transform);
            boat.name = PortSide ? "Explorer port rowboat" : "Explorer starboard rowboat";
            var seats = boat.GetComponentsInChildren<BoatSeat>(true);
            var aft = seats.Single(s => s.name == passengerSeatName && s.Role == BoatSeat.SeatRole.Passenger);
            var rowSeats = seats.Where(s => s.Role == BoatSeat.SeatRole.Rower).OrderBy(s => s.transform.localPosition.z).ToArray();
            if (rowSeats.Length != 2 || seats.Any(s => s.Profile == null) || rowerPrefab.GetComponent<NpcController>() == null)
                throw new InvalidOperationException("Launch needs two rower seats, a free aft passenger seat and the NPC prefab.");
            var floating = boat.GetComponent<FloatingShip>();
            var body = boat.GetComponentInChildren<Rigidbody>(true);
            var owner = boat.AddComponent<RowboatCrew>();
            owner.Configure(body, crew);
            floating.ConfigureLaunchHeight(pose.position.y);
            boat.transform.SetParent(null, true); // Activates the prepared boat at its water pose.
            IgnoreDuplicateWalkingHulls(boat, body, shipBody);
            if (!body.isKinematic && shipBody != null)
                body.linearVelocity = Vector3.ProjectOnPlane(shipBody.GetPointVelocity(pose.position), Vector3.up);
            for (int i = 0; i < crew.Length; i++)
            {
                crew[i] = Instantiate(rowerPrefab, rowSeats[i].transform.position, pose.rotation).GetComponent<NpcController>();
                crew[i].name = "Explorer rower " + (i + 1);
                if (!crew[i].Boarding.TrySeatImmediately(rowSeats[i], !PortSide))
                    throw new InvalidOperationException("Could not seat a rower.");
                floating.IgnoreCharacter(crew[i].GetComponent<CharacterController>());
            }
            if (!passenger.TrySeatImmediately(aft, !PortSide)) throw new InvalidOperationException("Could not seat the player.");
            floating.IgnoreCharacter(player.GetComponent<CharacterController>());
            PlayerView.Activate(player);
            LaunchedBoat = boat;
            gameObject.SetActive(false);
            return true;
        }
        catch (Exception error)
        {
            if (boat != null) Destroy(boat);
            Debug.LogException(error, this);
            return Fail("The rowboat could not be launched.");
        }
        finally { Destroy(staging); }
    }

    private bool Fail(string reason)
    {
        LastFailure = reason;
        feedbackUntil = Time.unscaledTime + 3;
        return false;
    }

    private bool FindClearWater(ref Pose pose, Rigidbody shipBody)
    {
        var scale = rowboatPrefab.transform.localScale;
        var forward = Vector3.ProjectOnPlane(shipFrame.forward, Vector3.up).normalized;
        var outward = Vector3.Cross(Vector3.up, forward) * (PortSide ? -1 : 1);
        for (float shift = 0; shift <= maximumHullClearanceAdjustment + .001f; shift += .5f)
        {
            int count = Physics.OverlapBoxNonAlloc(pose.position + pose.rotation * Vector3.Scale(clearanceBounds.center, scale),
                Vector3.Scale(clearanceBounds.extents, scale) + Vector3.one * .1f, overlaps, pose.rotation,
                Physics.DefaultRaycastLayers, QueryTriggerInteraction.Ignore);
            if (count == overlaps.Length) return false;
            LaunchObstruction = null;
            for (int i = 0; i < count; i++)
            {
                var shape = overlaps[i];
                if (shape == null || shape is CharacterController
                    || shape.transform.IsChildOf(shipFrame) && shape.attachedRigidbody != shipBody) continue;
                LaunchObstruction = shape;
                // Only the launching ship may expand the needed broadside gap.
                // Land, other vessels and props are genuine obstructions.
                if (shape.attachedRigidbody != shipBody) return false;
            }
            if (LaunchObstruction == null) return true;
            pose.position += outward * .5f;
        }
        return false;
    }

    private void IgnoreDuplicateWalkingHulls(GameObject boat, Rigidbody boatBody, Rigidbody shipBody)
    {
        // The render roots carry detailed moving walking surfaces. Their hull
        // rigidbodies already own physical collisions; avoid double contacts.
        var boatPhysics = boatBody.GetComponentsInChildren<Collider>().Where(c => c.attachedRigidbody == boatBody).ToArray();
        var shipPhysics = shipBody.GetComponentsInChildren<Collider>().Where(c => c.attachedRigidbody == shipBody).ToArray();
        foreach (var walking in shipFrame.GetComponentsInChildren<Collider>())
            if (walking.attachedRigidbody != shipBody && !(walking is CharacterController))
                foreach (var physical in boatPhysics) Physics.IgnoreCollision(physical, walking);
        foreach (var walking in boat.GetComponentsInChildren<Collider>())
            if (walking.attachedRigidbody != boatBody && !(walking is CharacterController))
                foreach (var physical in shipPhysics) Physics.IgnoreCollision(physical, walking);
    }

    private void Update()
    {
        if (Time.unscaledTime < nextPlayerSearch) return;
        nextPlayerSearch = Time.unscaledTime + .5f;
        nearbyPlayer = FindObjectsByType<PlayerController>(FindObjectsSortMode.None).FirstOrDefault(p => p.UsesPlayerInput && p.enabled);
    }

    private void OnGUI()
    {
        if (nearbyPlayer == null || Nearest(nearbyPlayer) != this
            || nearbyPlayer.GetComponent<BoatBoarding>().State != BoatBoarding.BoardingState.Idle) return;
        string key = nearbyPlayer.Interact != null ? nearbyPlayer.Interact.GetBindingDisplayString() : "Use";
        string text = Time.unscaledTime < feedbackUntil && LastFailure != null ? LastFailure
            : AnchorDeployed ? key + " — Launch rowboat with two rowers" : "Lower the anchor to launch this rowboat";
        GUI.Box(new Rect((Screen.width - 440) * .5f, Screen.height - 68, 440, 34), text);
    }

    private void OnDrawGizmosSelected()
    {
        if (shipFrame == null || launchPoint == null || rowboatPrefab == null) return;
        var ocean = ship != null ? ship.GetComponent<OceanBuoyancy>()?.Ocean : null;
        var pose = WaterPose(ocean);
        Gizmos.color = Color.green;
        Gizmos.DrawLine(transform.position, pose.position);
        Gizmos.matrix = Matrix4x4.TRS(pose.position, pose.rotation, rowboatPrefab.transform.localScale);
        Gizmos.DrawWireCube(clearanceBounds.center, clearanceBounds.size);
        Gizmos.DrawRay(clearanceBounds.center, Vector3.forward * 2);
        Gizmos.matrix = Matrix4x4.identity;
    }
}
