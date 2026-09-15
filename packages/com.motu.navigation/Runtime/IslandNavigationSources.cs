using System;
using System.Collections.Generic;
using Motu.Islands;
using UnityEngine;
using UnityEngine.AI;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Navigation
{
    // Owns only Unity mesh copies. Prepared arrays and native island ownership
    // stay with preparation/runtime; no collider GameObjects are needed to bake.
    internal sealed class IslandNavigationSources : IDisposable
    {
        internal readonly List<NavMeshBuildSource> sources = new List<NavMeshBuildSource>();
        private readonly List<Mesh> meshes = new List<Mesh>();
        internal Bounds bounds;
        internal int triangleCount;
        internal long meshBytes;
        private bool hasBounds;
        private readonly Bounds? selection;

        internal IslandNavigationSources(Bounds? chunkBounds = null, float padding = 0f)
        {
            if (!chunkBounds.HasValue) return;
            bounds = chunkBounds.Value;
            hasBounds = true;
            var expanded = bounds; expanded.Expand(padding * 2f);
            selection = expanded;
        }

        private void AddModifier(NavMeshBuildSource source)
        {
            if (selection.HasValue)
            {
                var extents = source.size * .5f;
                var x = source.transform.MultiplyVector(new Vector3(extents.x, 0, 0));
                var y = source.transform.MultiplyVector(new Vector3(0, extents.y, 0));
                var z = source.transform.MultiplyVector(new Vector3(0, 0, extents.z));
                var worldExtents = new Vector3(Mathf.Abs(x.x) + Mathf.Abs(y.x) + Mathf.Abs(z.x),
                    Mathf.Abs(x.y) + Mathf.Abs(y.y) + Mathf.Abs(z.y), Mathf.Abs(x.z) + Mathf.Abs(y.z) + Mathf.Abs(z.z));
                if (!selection.Value.Intersects(new Bounds(source.transform.MultiplyPoint3x4(Vector3.zero), worldExtents * 2f))) return;
            }
            sources.Add(source);
        }

        internal void AddMesh(IslandPreparedMesh prepared, bool excludeRiverBeds = false, IReadOnlyList<int> faces = null)
        {
            if (prepared == null || prepared.triangles.Length == 0) return;
            var faceCount = faces?.Count ?? prepared.triangles.Length / 3;
            var dry = new List<int>(faceCount * 3);
            var river = new List<int>();
            for (var face = 0; face < faceCount; face++)
            {
                var i = faces == null ? face * 3 : faces[face];
                var a = prepared.triangles[i];
                var b = prepared.triangles[i + 1];
                var c = prepared.triangles[i + 2];
                // Material.z is the native river-bed contribution. Keep all source
                // triangles, assigning river-bed faces to the non-walkable area.
                var blocked = excludeRiverBeds && prepared.material.Length == prepared.vertices.Length
                    && (prepared.material[a].b + prepared.material[b].b + prepared.material[c].b) / 3f >= 0.5f;
                var indices = blocked ? river : dry;
                indices.Add(a); indices.Add(b); indices.Add(c);
            }
            if (dry.Count != 0) AddMeshPart(prepared.vertices, dry, 0);
            if (river.Count != 0) AddMeshPart(prepared.vertices, river, 1);
            triangleCount += faceCount;
        }

        private void AddMeshPart(Vector3[] vertices, List<int> indices, int area)
        {
            var mesh = new Mesh { name = "Navigation LOD0 source", indexFormat = IndexFormat.UInt32 };
            meshes.Add(mesh);
            var remap = new Dictionary<int, int>();
            var selected = new List<Vector3>();
            for (var i = 0; i < indices.Count; i++)
            {
                var source = indices[i];
                if (!remap.TryGetValue(source, out var mapped))
                {
                    mapped = selected.Count;
                    remap.Add(source, mapped);
                    selected.Add(vertices[source]);
                }
                indices[i] = mapped;
            }
            mesh.SetVertices(selected);
            mesh.SetTriangles(indices, 0);
            mesh.RecalculateBounds();
            mesh.UploadMeshData(false);
            sources.Add(new NavMeshBuildSource
            {
                shape = NavMeshBuildSourceShape.Mesh, sourceObject = mesh,
                transform = Matrix4x4.identity, area = area,
            });
            meshBytes += (long)mesh.vertexCount * 12 + (long)indices.Count * 4;
            if (!hasBounds) { bounds = mesh.bounds; hasBounds = true; }
            else if (!selection.HasValue) bounds.Encapsulate(mesh.bounds);
        }

        internal void AddForest(IslandPreparedForestData forest)
        {
            if (forest == null) return;
            AddWood(forest.lod0TrunkColliderTiles, false);
            AddWood(forest.lod0LogColliderTiles, true);
        }

        // Non-walkable volumes prevent a walkable span surviving inside a
        // closed rock/trunk mesh. Bounds conservatively match the colliders.
        internal void AddWood(IslandPreparedTreeCollider[][] tiles, bool fallen)
        {
            if (tiles == null) return;
            foreach (var tile in tiles)
            {
                if (tile == null) continue;
                foreach (var wood in tile)
                {
                    var axis = wood.top - wood.bottom;
                    AddModifier(new NavMeshBuildSource
                    {
                        shape = NavMeshBuildSourceShape.ModifierBox,
                        transform = Matrix4x4.TRS((wood.top + wood.bottom) * 0.5f,
                            Quaternion.FromToRotation(Vector3.up, axis.normalized), Vector3.one),
                        size = new Vector3(wood.radius * 2f, axis.magnitude + (fallen ? 0f : wood.radius * 2f), wood.radius * 2f),
                        area = 1,
                    });
                }
            }
        }

        internal void AddBoulders(IslandPreparedBoulderCollider[][] tiles)
        {
            if (tiles == null) return;
            foreach (var tile in tiles)
            {
                if (tile == null) continue;
                foreach (var boulder in tile)
                    AddModifier(new NavMeshBuildSource
                    {
                        shape = NavMeshBuildSourceShape.ModifierBox,
                        transform = Matrix4x4.Translate(boulder.centre),
                        size = Vector3.one * (boulder.radius * 2f), area = 1,
                    });
            }
        }

        internal void ExcludeSubmergedGround(float minimumHeight, float radius)
        {
            if (!hasBounds) return;
            var bottom = bounds.min.y - 1f;
            if (minimumHeight > bottom)
                sources.Add(new NavMeshBuildSource
                {
                    shape = NavMeshBuildSourceShape.ModifierBox, area = 1,
                    transform = Matrix4x4.Translate(new Vector3(bounds.center.x, (bottom + minimumHeight) * 0.5f, bounds.center.z)),
                    size = new Vector3(bounds.size.x + radius * 4f, minimumHeight - bottom, bounds.size.z + radius * 4f),
                });
            if (!selection.HasValue) bounds.Expand(new Vector3(radius * 4f, 4f, radius * 4f));
        }

        public void Dispose()
        {
            sources.Clear();
            foreach (var mesh in meshes) DestroyUnityObject(mesh);
            meshes.Clear();
        }
    }
}
