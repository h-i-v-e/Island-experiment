using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Rendering;
using Unity.Profiling;
using Motu.Interop;
using Motu.Rendering;
using Motu.World;
using Stopwatch = System.Diagnostics.Stopwatch;

namespace Motu.Streaming
{
    /// <summary>Owns a bounded complete cave set with its island. Entrances and
    /// passage colliders remain resident with the surrounding terrain.</summary>
    public sealed class CaveStreamer : MonoBehaviour, IDisposable
    {
        private static readonly ProfilerMarker MeshCreationMarker = new ProfilerMarker("Motu.Caves.CreateMesh");
        private static readonly ProfilerMarker ColliderCookMarker = new ProfilerMarker("Motu.Caves.CookCollider");
        private IslandPreparedCaves prepared = IslandPreparedCaves.Empty;
        private readonly List<Mesh> meshes = new List<Mesh>();
        private readonly List<MeshCollider> colliders = new List<MeshCollider>();
        private Material material;
        private bool ready, released;
        public int CaveCount => prepared.caves.Length;
        public int BranchCount => prepared.branchCount;
        public string Diagnostics => prepared.stats + $"; {BranchCount} branches, {CaveCount + BranchCount} passages";
        public double ExportMilliseconds => prepared.exportMilliseconds;
        public double CopyMilliseconds => prepared.copyMilliseconds;
        public long PreparedBufferBytes => prepared.bufferBytes;
        public double MeshCreationMilliseconds { get; private set; }
        public double ColliderCookMilliseconds { get; private set; }
        public double LongestColliderCookMilliseconds { get; private set; }
        public int RenderTriangles { get; private set; }
        public int ColliderTriangles { get; private set; }
        public int MeshCount => meshes.Count;

        internal async Task InitializeAsync(IslandPreparedCaves data, Material terrainMaterial,
            CancellationToken cancellation, UnityFrameBudget budget)
        {
            prepared = data ?? IslandPreparedCaves.Empty;
            if (CaveCount == 0) { ready = true; return; }
            var shader = Resources.Load<Shader>("CaveSurface");
            if (shader == null) throw new InvalidOperationException("Cave Surface shader is missing.");
            material = new Material(shader) { name = "Island cave stone" };
            material.SetTexture("_TerrainAlbedoArray", terrainMaterial.GetTexture("_TerrainAlbedoArray"));
            material.SetFloat("_UseTextures", terrainMaterial.GetTexture("_TerrainAlbedoArray") != null ? 1 : 0);
            if (terrainMaterial.HasProperty("_TerrainLayerWorldSizesA"))
            {
                var sizes = terrainMaterial.GetVector("_TerrainLayerWorldSizesA");
                material.SetFloat("_DirtSize", Mathf.Max(.1f, sizes.x));
            }
            if (terrainMaterial.HasProperty("_RockColor"))
                material.SetColor("_RockColour", terrainMaterial.GetColor("_RockColor"));
            if (terrainMaterial.HasProperty("_GroundDirtColor"))
                material.SetColor("_DirtColour", terrainMaterial.GetColor("_GroundDirtColor"));
            if (terrainMaterial.HasProperty("_Color"))
                material.SetColor("_Color", terrainMaterial.GetColor("_Color"));
            if (terrainMaterial.HasProperty("_CliffNoise3D"))
                material.SetTexture("_CliffNoise3D", terrainMaterial.GetTexture("_CliffNoise3D"));
            foreach (var property in new[] { "_CliffNoisePeriod", "_CliffNoiseDetailScale", "_CliffNormalStrength" })
                if (terrainMaterial.HasProperty(property))
                    material.SetFloat(property, terrainMaterial.GetFloat(property));
            // Passage normals come from the mesh, not the exterior heightfield.
            var properties = new MaterialPropertyBlock();
            properties.SetFloat("_WorldNormalWeight", 0f);
            var timer = new Stopwatch();
            for (var i = 0; i < CaveCount; i++)
            {
                var root = new GameObject($"Cave {i + 1} [{prepared.caves[i].id}]");
                root.transform.SetParent(transform, false);
                foreach (var source in prepared.caves[i].chunks)
                {
                    cancellation.ThrowIfCancellationRequested();
                    timer.Restart();
                    Mesh mesh;
                    using (MeshCreationMarker.Auto()) mesh = IslandMeshInterop.CreateGeneratedMesh(source);
                    MeshCreationMilliseconds += timer.Elapsed.TotalMilliseconds;
                    meshes.Add(mesh);
                    mesh.name = $"Cave {i + 1} surface {meshes.Count}";
                    var chunk = new GameObject(mesh.name);
                    chunk.transform.SetParent(root.transform, false);
                    chunk.AddComponent<MeshFilter>().sharedMesh = mesh;
                    var collisionOnly = source.caveAttributes.Length > 0 && source.caveAttributes[0].y > 1.5f;
                    var triangleCount = source.triangles.Length / 3;
                    if (!collisionOnly) RenderTriangles += triangleCount;
                    ColliderTriangles += triangleCount;
                    var renderer = chunk.AddComponent<MeshRenderer>();
                    renderer.enabled = !collisionOnly;
                    renderer.sharedMaterial = material;
                    renderer.SetPropertyBlock(properties);
                    renderer.shadowCastingMode = ShadowCastingMode.TwoSided;
                    // The cave texture coordinates are already island-local, independent
                    // of the world's translated island root.
                    timer.Restart();
                    MeshCollider collider;
                    using (ColliderCookMarker.Auto())
                    {
                        // Adding a collider can already cook the MeshFilter's
                        // mesh, so include component creation in this timing.
                        collider = chunk.AddComponent<MeshCollider>();
                        collider.convex = false;
                        collider.sharedMesh = mesh;
                    }
                    var cookMilliseconds = timer.Elapsed.TotalMilliseconds;
                    ColliderCookMilliseconds += cookMilliseconds;
                    LongestColliderCookMilliseconds = Math.Max(LongestColliderCookMilliseconds, cookMilliseconds);
                    colliders.Add(collider);
                    if (collider.sharedMesh == null)
                        throw new InvalidOperationException("Cave collider installation failed.");
                    await budget.YieldIfExceededAsync(cancellation);
                }
            }
            ready = true;
        }

        public bool TryGetEntrance(int index, out Vector3 floor, out Vector3 inward)
        {
            floor = inward = default;
            if (!ready || index < 0 || index >= CaveCount) return false;
            floor = transform.TransformPoint(prepared.caves[index].entrance);
            inward = transform.TransformDirection(prepared.caves[index].inward);
            return true;
        }

        public bool TryGetChamber(int index, out Vector3 floor)
        {
            floor = default;
            if (!ready || index < 0 || index >= CaveCount) return false;
            var p = transform.TransformPoint(prepared.caves[index].chamber);
            return TryFindGround(p, 2f, 3f, out floor);
        }

        public int GetBranchCount(int cave) => cave >= 0 && cave < CaveCount ? prepared.caves[cave].branchPaths.Length : 0;

        public bool TryGetBranchChamber(int cave, int branch, out Vector3 floor)
        {
            floor = default;
            if (!ready || branch < 0 || branch >= GetBranchCount(cave)) return false;
            var path = prepared.caves[cave].branchPaths[branch];
            return TryFindGround(transform.TransformPoint(path[path.Length - 1]), 2f, 3f, out floor);
        }

        /// <summary>Ground near a known interior elevation; never searches from
        /// above the island and accidentally selects the cave roof.</summary>
        public bool TryFindGround(Vector3 near, float above, float below, out Vector3 point)
        {
            point = near;
            if (!ready || !isActiveAndEnabled || above < 0 || below < 0) return false;
            var ray = new Ray(near + Vector3.up * above, Vector3.down);
            if (!Raycast(ray, above + below, out var hit) || hit.normal.y < .7f) return false;
            point = hit.point;
            return true;
        }

        internal bool OwnsCollider(Collider collider) => collider is MeshCollider mesh && colliders.Contains(mesh);

        internal bool Raycast(Ray ray, float distance, out RaycastHit nearest)
        {
            nearest = default;
            var found = false;
            foreach (var collider in colliders)
            {
                if (collider == null || !collider.enabled || !collider.gameObject.activeInHierarchy
                    || !collider.Raycast(ray, out var hit, distance)) continue;
                distance = hit.distance; nearest = hit; found = true;
            }
            return found;
        }

        private void OnDrawGizmosSelected()
        {
            foreach (var cave in prepared.caves)
            {
                Gizmos.color = Color.cyan;
                var entrance = transform.TransformPoint(cave.entrance);
                Gizmos.DrawWireSphere(entrance + Vector3.up, 1f);
                Gizmos.DrawLine(entrance, entrance + transform.TransformDirection(cave.inward) * 5f);
                Gizmos.color = Color.yellow;
                Gizmos.DrawWireSphere(transform.TransformPoint(cave.chamber) + Vector3.up, 1f);
                Gizmos.color = Color.magenta;
                foreach (var path in cave.branchPaths)
                {
                    for (var i = 1; i < path.Length; i++)
                        Gizmos.DrawLine(transform.TransformPoint(path[i - 1]) + Vector3.up,
                            transform.TransformPoint(path[i]) + Vector3.up);
                    Gizmos.DrawWireSphere(transform.TransformPoint(path[path.Length - 1]) + Vector3.up, 1f);
                }
            }
        }

        public void Dispose()
        {
            if (released) return;
            released = true; ready = false;
            foreach (var collider in colliders) if (collider != null) { collider.enabled = false; collider.sharedMesh = null; }
            colliders.Clear();
            foreach (var mesh in meshes) UnityObjectLifetime.DestroyUnityObject(mesh);
            meshes.Clear();
            UnityObjectLifetime.DestroyUnityObject(material);
            material = null;
            prepared = IslandPreparedCaves.Empty;
            MeshCreationMilliseconds = ColliderCookMilliseconds = LongestColliderCookMilliseconds = 0;
            RenderTriangles = ColliderTriangles = 0;
        }
        private void OnDestroy() => Dispose();
    }
}
