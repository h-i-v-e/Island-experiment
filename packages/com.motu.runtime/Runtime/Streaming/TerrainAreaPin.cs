using System.Collections.Generic;
using UnityEngine;

namespace Motu.Streaming
{
    /// <summary>Keeps collision under an active actor on already resident islands.
    /// Does not request islands, move the viewer, or retain visual detail.</summary>
    [DisallowMultipleComponent, DefaultExecutionOrder(-500)]
    public sealed class TerrainAreaPin : MonoBehaviour
    {
        [SerializeField] private Collider[] collisionShapes;
        [SerializeField] private Behaviour activeOwner;
        [SerializeField] private bool requireActiveOwner;
        [SerializeField] private bool requireNavigation;
        [SerializeField, Min(0f)] private float paddingMetres = 2f;
        private static readonly HashSet<TerrainAreaPin> sources = new();
        private static readonly List<TerrainTileStreamer> residents = new();
        private static int lastFrame = -1;
        private static float lastFixedTime = float.NegativeInfinity;
        internal static HashSet<TerrainAreaPin> Sources => sources;
        internal bool RequiresNavigation => requireNavigation;

        public void Configure(Collider[] shapes, bool navigation = false, Behaviour owner = null)
        {
            collisionShapes = shapes;
            requireNavigation = navigation;
            activeOwner = owner;
            requireActiveOwner = owner != null;
            if (Application.isPlaying && isActiveAndEnabled) RefreshAll();
        }

        private void OnEnable()
        {
            if (!Application.isPlaying) return;
            if (collisionShapes == null) collisionShapes = GetComponentsInChildren<Collider>(true);
            sources.Add(this);
            RefreshAll();
        }
        private void OnDisable() { if (sources.Remove(this)) RefreshAll(); }
        private void Update()
        {
            if (lastFrame == Time.frameCount) return;
            lastFrame = Time.frameCount; RefreshAll();
        }
        private void FixedUpdate()
        {
            if (lastFixedTime == Time.fixedTime) return;
            lastFixedTime = Time.fixedTime; RefreshAll();
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        private static void ResetRegistry()
        {
            sources.Clear(); residents.Clear(); lastFrame = -1; lastFixedTime = float.NegativeInfinity;
        }
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
        private static void RestoreWithoutSceneReload()
        {
            foreach (var pin in FindObjectsByType<TerrainAreaPin>(FindObjectsSortMode.None))
                if (pin.isActiveAndEnabled) sources.Add(pin);
            foreach (var terrain in FindObjectsByType<TerrainTileStreamer>(FindObjectsInactive.Include, FindObjectsSortMode.None))
                terrain.RegisterPinResidency();
            RefreshAll();
        }
        internal static void RegisterResident(TerrainTileStreamer terrain)
        {
            if (!residents.Contains(terrain)) residents.Add(terrain);
            terrain.RefreshPinnedColliders();
        }
        internal static void UnregisterResident(TerrainTileStreamer terrain) => residents.Remove(terrain);

        /// <summary>Refreshes actor footprints; incoming tiles precede retirement.
        /// Normal updates run before physics and character animation movement.</summary>
        public static void RefreshAll()
        {
            for (int i = residents.Count - 1; i >= 0; i--)
                if (residents[i] == null) residents.RemoveAt(i);
                else residents[i].RefreshPinnedColliders();
        }

        internal bool TryGetWorldBounds(out Bounds bounds)
        {
            bounds = default;
            if (!isActiveAndEnabled || requireActiveOwner && (activeOwner == null || !activeOwner.isActiveAndEnabled)) return false;
            bool found = false;
            if (collisionShapes != null)
                foreach (var shape in collisionShapes)
                {
                    if (shape == null || !shape.enabled || !shape.gameObject.activeInHierarchy || shape.isTrigger) continue;
                    // Authored geometry follows transforms even before the physics
                    // sync, including detached hulls, boarding and teleportation.
                    Bounds local;
                    if (shape is MeshCollider mesh && mesh.sharedMesh != null) local = mesh.sharedMesh.bounds;
                    else if (shape is BoxCollider box) local = new Bounds(box.center, box.size);
                    else if (shape is CharacterController character)
                        local = new Bounds(character.center, new Vector3(character.radius * 2, character.height, character.radius * 2));
                    else if (shape is SphereCollider sphere) local = new Bounds(sphere.center, Vector3.one * sphere.radius * 2);
                    else if (shape is CapsuleCollider capsule)
                    {
                        var size = Vector3.one * capsule.radius * 2; size[capsule.direction] = Mathf.Max(capsule.height, size[capsule.direction]);
                        local = new Bounds(capsule.center, size);
                    }
                    else
                    {
                        if (!found) bounds = shape.bounds; else bounds.Encapsulate(shape.bounds);
                        found = true; continue;
                    }
                    var world = TransformBounds(shape.transform.localToWorldMatrix, local);
                    if (!found) bounds = world; else bounds.Encapsulate(world);
                    found = true;
                }
            // Boarding temporarily disables an NPC's capsule. Its actor still
            // owns terrain at the current root; the boat has its own larger pin.
            if (!found) bounds = new Bounds(transform.position, Vector3.one * .5f);
            bounds.Expand(Mathf.Max(0, paddingMetres) * 2f);
            return true;
        }
        internal static Bounds TransformBounds(Matrix4x4 matrix, Bounds bounds)
        {
            var x = matrix.MultiplyVector(new Vector3(bounds.extents.x, 0, 0));
            var y = matrix.MultiplyVector(new Vector3(0, bounds.extents.y, 0));
            var z = matrix.MultiplyVector(new Vector3(0, 0, bounds.extents.z));
            var extent = new Vector3(Mathf.Abs(x.x) + Mathf.Abs(y.x) + Mathf.Abs(z.x),
                Mathf.Abs(x.y) + Mathf.Abs(y.y) + Mathf.Abs(z.y), Mathf.Abs(x.z) + Mathf.Abs(y.z) + Mathf.Abs(z.z));
            return new Bounds(matrix.MultiplyPoint3x4(bounds.center), extent * 2);
        }
    }
}
