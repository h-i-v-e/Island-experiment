using System.Collections.Generic;
using Motu.Gameplay;
using UnityEngine;

[AddComponentMenu("Boats/Pushable Rowboat")]
[DisallowMultipleComponent]
[RequireComponent(typeof(RowboatController))]
public sealed class BoatPushable : MonoBehaviour
{
    [Tooltip("The actual wooden hull surfaces used for palm contact. No standing or hand markers are needed.")]
    [SerializeField] private Collider[] hullSurfaces;
    [SerializeField, Min(0)] private float pushForceNewtons = 4200f;
    [SerializeField, Min(.1f)] private float maximumPushSpeed = 1.8f;
    private RowboatController boat;
    private BoatPushing pusher;
    private static readonly HashSet<BoatPushable> active = new HashSet<BoatPushable>();
    private readonly RaycastHit[] hits = new RaycastHit[32];
    public static IEnumerable<BoatPushable> Active => active;
    public Rigidbody Body => boat != null ? boat.Body : null;
    public bool IsGrounded => boat != null && boat.IsGrounded;
    public BoatSeat ControlSeat => boat != null ? boat.CommandSeat : null;
    public bool IsAfloat => boat != null && !IsGrounded && !boat.IsShoreHeld && Body != null && !Body.isKinematic
        && Body.TryGetComponent<OceanBuoyancy>(out var buoyancy) && buoyancy.SubmergedFraction > .05f;
    public bool IsAvailable => isActiveAndEnabled && pusher == null && IsGrounded && Body != null && !Body.isKinematic
        && boat.Input.sqrMagnitude < .0001f && Vector3.Dot(Body.rotation * Vector3.up, Vector3.up) > .5f
        && Vector3.ProjectOnPlane(Body.linearVelocity, Vector3.up).sqrMagnitude < 4f;
    public void Configure(Collider[] surfaces) => hullSurfaces = surfaces;

    public readonly struct HandContact
    {
        internal readonly BoatPushable Owner;
        private readonly Vector3 localPoint, localNormal;
        internal readonly Vector3 AnimatedPalm;
        internal readonly float SampleTime;
        public bool IsValid => Owner != null && Owner.Body != null && Time.time - SampleTime <= .1f;
        public Vector3 Position => Owner.Body.position + Owner.Body.rotation * localPoint;
        public Vector3 Normal => Owner.Body.rotation * localNormal;
        internal HandContact(BoatPushable owner, RaycastHit hit, Vector3 palm)
        {
            Owner = owner; AnimatedPalm = palm; SampleTime = Time.time;
            var inverse = Quaternion.Inverse(owner.Body.rotation);
            localPoint = inverse * (hit.point - owner.Body.position);
            localNormal = inverse * hit.normal;
        }
        private HandContact(BoatPushable owner, Vector3 point, Vector3 normal, Vector3 palm)
        { Owner = owner; localPoint = point; localNormal = normal; AnimatedPalm = palm; SampleTime = Time.time; }
        internal HandContact WithPalm(Vector3 palm) => new HandContact(Owner, localPoint, localNormal, palm);
    }

    private void Awake()
    {
        boat = GetComponent<RowboatController>();
        if (hullSurfaces == null || hullSurfaces.Length == 0)
        {
            var surfaces = new List<Collider>();
            foreach (var mesh in GetComponentsInChildren<MeshCollider>(true))
                if (!mesh.convex && !mesh.isTrigger && mesh.sharedMesh != null) surfaces.Add(mesh);
            hullSurfaces = surfaces.Count > 0 ? surfaces.ToArray() : boat.Body.GetComponentsInChildren<Collider>(true);
        }
    }
    private void OnEnable() => active.Add(this);
    private void OnDisable()
    {
        active.Remove(this);
        if (pusher != null) pusher.CancelPush();
    }
    private bool IsSurface(Collider shape)
    {
        foreach (var surface in hullSurfaces)
            if (surface == shape && surface.enabled && surface.gameObject.activeInHierarchy) return true;
        return false;
    }
    private bool FirstHullHit(Vector3 origin, float radius, Vector3 direction, float distance, out RaycastHit result)
    {
        result = default;
        int count = Physics.SphereCastNonAlloc(origin, radius, direction, hits, distance,
            Physics.DefaultRaycastLayers, QueryTriggerInteraction.Ignore);
        if (count == hits.Length) return false;
        int nearest = -1;
        for (int i = 0; i < count; i++)
        {
            var shape = hits[i].collider;
            if (shape is CharacterController) continue;
            // The detached coarse physics hull must not hide the detailed wooden surface.
            if (!IsSurface(shape) && (shape.attachedRigidbody == Body || shape.transform.IsChildOf(transform))) continue;
            if (nearest < 0 || hits[i].distance < hits[nearest].distance) nearest = i;
        }
        if (nearest < 0 || !IsSurface(hits[nearest].collider)) return false;
        result = hits[nearest]; return true;
    }
    public bool IsInFrontOf(Vector3 position, Vector3 forward, float distance, out float gap)
    {
        gap = 0;
        const float backoff = .3f;
        if (!FirstHullHit(position + Vector3.up * .45f - forward * backoff, .25f, forward, distance + backoff, out var hit)) return false;
        gap = Mathf.Max(0, hit.distance - backoff); return true;
    }
    public bool TryHandContact(Vector3 palm, Vector3 previousPalm, bool hasPrevious, Vector3 forward,
        float radius, float reach, out HandContact contact)
    {
        contact = default;
        // Start behind the animated palm to recover a shallow penetration from
        // the previous animation step; never pull a hand across an air gap.
        const float penetration = .18f;
        bool touched = FirstHullHit(palm - forward * penetration, radius, forward, penetration + reach, out var hit);
        if (!touched && hasPrevious)
        {
            var travel = palm - previousPalm;
            if (travel.sqrMagnitude > .000001f && travel.sqrMagnitude < .25f)
                touched = FirstHullHit(previousPalm, radius, travel.normalized, travel.magnitude, out hit);
        }
        if (!touched || Vector3.Distance(palm, hit.point) > penetration + radius + reach) return false;
        var inward = -Vector3.ProjectOnPlane(hit.normal, Vector3.up).normalized;
        if (inward.sqrMagnitude < .5f || Vector3.Dot(forward, inward) < .25f) return false;
        contact = new HandContact(this, hit, palm); return true;
    }
    public bool TryReserve(BoatPushing actor)
    {
        if (actor == null || !IsAvailable) return false;
        pusher = actor; return true;
    }
    public void Release(BoatPushing actor)
    {
        if (pusher != actor) return;
        boat.EndShorePush(); pusher = null;
    }
    public bool ApplyPush(BoatPushing actor, HandContact contact, float strength)
    {
        if (pusher != actor || contact.Owner != this || !contact.IsValid || Body.isKinematic || strength <= 0
            || Vector3.Distance(contact.AnimatedPalm, contact.Position) > .08f) return false;
        var direction = -Vector3.ProjectOnPlane(contact.Normal, Vector3.up).normalized;
        if (direction.sqrMagnitude < .5f) return false;
        float speed = Vector3.Dot(Body.GetPointVelocity(contact.Position), direction);
        float limit = Mathf.Clamp01(1f - speed / maximumPushSpeed);
        if (limit <= 0) return false;
        boat.BeginShorePush();
        // Each touching hand supplies half the available push. Off-centre hands
        // naturally turn the body through the force's application point.
        Body.AddForceAtPosition(direction * (pushForceNewtons * .5f * strength * limit), contact.Position, ForceMode.Force);
        return true;
    }
}
