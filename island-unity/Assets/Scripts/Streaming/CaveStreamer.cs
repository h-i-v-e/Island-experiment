using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Rendering;
using Motu.Interop;
using Motu.Rendering;
using Motu.World;

namespace Motu.Streaming
{
    /// <summary>Owns a bounded complete cave set with its island. Entrances and
    /// passage colliders remain resident with the surrounding terrain.</summary>
    public sealed class CaveStreamer : MonoBehaviour, IDisposable
    {
        private IslandPreparedCaves prepared = IslandPreparedCaves.Empty;
        private readonly List<Mesh> meshes = new List<Mesh>();
        private readonly List<MeshCollider> colliders = new List<MeshCollider>();
        private Material material;
        private bool ready, released;
        public int CaveCount => prepared.caves.Length;
        public string Diagnostics => prepared.stats.ToString();

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
            for (var i = 0; i < CaveCount; i++)
            {
                var root = new GameObject($"Cave {i + 1} [{prepared.caves[i].id}]");
                root.transform.SetParent(transform, false);
                foreach (var source in prepared.caves[i].chunks)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var mesh = IslandMeshInterop.CreateGeneratedMesh(source);
                    meshes.Add(mesh);
                    mesh.name = $"Cave {i + 1} surface {meshes.Count}";
                    var chunk = new GameObject(mesh.name);
                    chunk.transform.SetParent(root.transform, false);
                    chunk.AddComponent<MeshFilter>().sharedMesh = mesh;
                    var collisionOnly = source.caveAttributes.Length > 0 && source.caveAttributes[0].y > 1.5f;
                    var renderer = chunk.AddComponent<MeshRenderer>();
                    renderer.enabled = !collisionOnly;
                    renderer.sharedMaterial = material;
                    renderer.SetPropertyBlock(properties);
                    renderer.shadowCastingMode = ShadowCastingMode.TwoSided;
                    // The cave texture coordinates are already island-local, independent
                    // of the world's translated island root.
                    var collider = chunk.AddComponent<MeshCollider>();
                    collider.convex = false;
                    collider.sharedMesh = mesh;
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
        }
        private void OnDestroy() => Dispose();
    }
}
