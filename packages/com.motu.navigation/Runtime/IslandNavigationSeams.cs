using System;
using System.Collections.Generic;
using System.Threading;
using Motu.Settings;
using UnityEngine;
using UnityEngine.AI;

namespace Motu.Navigation
{
    internal static class IslandNavigationSeams
    {
        internal readonly struct Crossing
        {
            internal readonly Vector3 a, b;
            internal Crossing(Vector3 a, Vector3 b) { this.a = a; this.b = b; }
        }

        // Crossings come from walkable original faces, including separate cave
        // levels. Never create a link merely because two distant samples exist.
        internal static List<Crossing> Prepare(IslandNavigationChunks.Chunk chunk, bool acrossX,
            IslandNavigationSettings settings, CancellationToken cancellation)
        {
            var result = new List<Crossing>();
            var seen = new HashSet<Vector3Int>();
            var voxel = settings.VoxelSize > 0 ? settings.VoxelSize : settings.AgentRadius / 3f;
            var spacing = Mathf.Max(.5f, settings.AgentRadius);
            var plane = acrossX ? chunk.bounds.min.x : chunk.bounds.min.z;
            var minimum = acrossX ? chunk.bounds.min.z : chunk.bounds.min.x;
            var maximum = acrossX ? chunk.bounds.max.z : chunk.bounds.max.x;
            var slope = Mathf.Cos(settings.MaximumSlope * Mathf.Deg2Rad);
            var intersections = new Vector3[3];
            foreach (var part in chunk.parts)
            {
                var mesh = part.mesh;
                foreach (var face in part.triangles)
                {
                    if (face % 3072 == 0) cancellation.ThrowIfCancellationRequested();
                    var ia = mesh.triangles[face]; var ib = mesh.triangles[face + 1]; var ic = mesh.triangles[face + 2];
                    if (part.excludeRiverBeds && mesh.material.Length == mesh.vertices.Length
                        && (mesh.material[ia].b + mesh.material[ib].b + mesh.material[ic].b) / 3f >= .5f) continue;
                    var a = mesh.vertices[ia]; var b = mesh.vertices[ib]; var c = mesh.vertices[ic];
                    var normal = Vector3.Cross(b - a, c - a).normalized;
                    if (normal.y < slope) continue;
                    var count = 0;
                    void Intersect(Vector3 from, Vector3 to)
                    {
                        var d0 = (acrossX ? from.x : from.z) - plane;
                        var d1 = (acrossX ? to.x : to.z) - plane;
                        if (d0 == 0f) { if (count < 3) intersections[count++] = from; }
                        else if (d0 * d1 < 0f && count < 3)
                            intersections[count++] = Vector3.Lerp(from, to, d0 / (d0 - d1));
                    }
                    Intersect(a, b); Intersect(b, c); Intersect(c, a);
                    if (count < 2) continue;
                    var fromPoint = intersections[0]; var toPoint = intersections[1];
                    var fromValue = acrossX ? fromPoint.z : fromPoint.x;
                    var toValue = acrossX ? toPoint.z : toPoint.x;
                    if (fromValue > toValue)
                    {
                        (fromPoint, toPoint) = (toPoint, fromPoint);
                        (fromValue, toValue) = (toValue, fromValue);
                    }
                    var low = Mathf.Max(minimum, fromValue); var high = Mathf.Min(maximum, toValue);
                    if (high - low < 1e-5f) continue;
                    void Add(float along)
                    {
                        var point = Vector3.Lerp(fromPoint, toPoint, (along - fromValue) / (toValue - fromValue));
                        if (point.y < settings.MinimumGroundHeight) return;
                        var key = new Vector3Int(Mathf.RoundToInt(point.x / (voxel * .5f)),
                            Mathf.RoundToInt(point.y / (voxel * .5f)), Mathf.RoundToInt(point.z / (voxel * .5f)));
                        if (!seen.Add(key)) return;
                        var offset = acrossX ? new Vector3(voxel, -normal.x / normal.y * voxel, 0)
                            : new Vector3(0, -normal.z / normal.y * voxel, voxel);
                        result.Add(new Crossing(point - offset, point + offset));
                    }
                    var first = Mathf.Ceil(low / spacing) * spacing;
                    if (first > high) Add((low + high) * .5f);
                    else for (var along = first; along <= high; along += spacing) Add(along);
                }
            }
            return result;
        }

        internal static bool HasNearbyConnection(NavMeshLinkData candidate, List<NavMeshLinkData> existing,
            Transform transform, NavMeshQueryFilter filter, float voxel)
        {
            var from = transform.TransformPoint(candidate.startPosition);
            var to = transform.TransformPoint(candidate.endPosition);
            bool Reaches(Vector3 a, Vector3 b) => !NavMesh.Raycast(a, b, out var hit, filter)
                && (hit.position - b).sqrMagnitude <= voxel * voxel * 9f;
            foreach (var link in existing)
            {
                if ((link.startPosition - candidate.startPosition).sqrMagnitude > 64f
                    || (link.endPosition - candidate.endPosition).sqrMagnitude > 64f) continue;
                // Reuse a crossing only when BOTH sides can walk to it. Keep
                // separate narrow passages and cave levels; avoid thousands of
                // redundant off-mesh nodes exhausting long-distance path queries.
                if (Reaches(from, transform.TransformPoint(link.startPosition))
                    && Reaches(to, transform.TransformPoint(link.endPosition))) return true;
            }
            return false;
        }

        internal static bool TryCreate(Crossing crossing, bool acrossX, Transform transform,
            NavMeshQueryFilter filter, float voxel, out NavMeshLinkData link)
        {
            link = default;
            var a = transform.TransformPoint(crossing.a); var b = transform.TransformPoint(crossing.b);
            if (!NavMesh.SamplePosition(a, out var from, voxel * 3f, filter)
                || !NavMesh.SamplePosition(b, out var to, voxel * 3f, filter)) return false;
            var localA = transform.InverseTransformPoint(from.position);
            var localB = transform.InverseTransformPoint(to.position);
            var displacementA = localA - crossing.a; var displacementB = localB - crossing.b;
            if (new Vector2(displacementA.x, displacementA.z).magnitude > voxel * 1.5f
                || new Vector2(displacementB.x, displacementB.z).magnitude > voxel * 1.5f) return false;
            var centre = (crossing.a + crossing.b) * .5f;
            var plane = acrossX ? centre.x : centre.z;
            if ((acrossX ? localA.x : localA.z) >= plane - voxel * .1f
                || (acrossX ? localB.x : localB.z) <= plane + voxel * .1f) return false;
            bool BeforeJoin(Vector3 edge)
            {
                var local = transform.InverseTransformPoint(edge) - centre;
                // NavMesh raycasts follow simplified polygons rather than the
                // height mesh; their Y error must not reject a valid border.
                return new Vector2(local.x, local.z).magnitude > voxel * 1.5f;
            }
            // An obstacle before the join must not become a shortcut via a link.
            if (NavMesh.Raycast(from.position, b, out var edgeA, filter)
                && BeforeJoin(edgeA.position)) return false;
            if (NavMesh.Raycast(to.position, a, out var edgeB, filter)
                && BeforeJoin(edgeB.position)) return false;
            link = new NavMeshLinkData { startPosition = localA, endPosition = localB,
                bidirectional = true, width = 0f, area = 0, agentTypeID = filter.agentTypeID, costModifier = -1f };
            return true;
        }
    }
}
