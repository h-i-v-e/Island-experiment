using System;
using System.Collections.Generic;
using System.Threading;
using Motu.Islands;
using UnityEngine;

namespace Motu.Streaming
{
    internal static partial class WheatTileGeometry
    {
        private sealed class CanopyBuilder
        {
            private const double EdgeTolerance = 1e-7;
            private readonly List<Vector3> vertices = new List<Vector3>();
            private readonly List<Vector3> normals = new List<Vector3>();
            private readonly List<int> triangles = new List<int>();
            private readonly List<Vector2> uv = new List<Vector2>();
            private readonly List<Color> colours = new List<Color>();
            // Clipping a convex triangle by a rectangle introduces up to four corners.
            private readonly ClipVertex[] polygon = new ClipVertex[8];
            private readonly ClipVertex[] scratch = new ClipVertex[8];

            internal void Append(IslandPreparedMesh terrain, FieldSpace space, CancellationToken token)
            {
                var indices = terrain.triangles;
                var ground = terrain.vertices;
                var paintedNormals = terrain.normals;
                var hasNormals = paintedNormals.Length == ground.Length;
                var tint = Unit(space.Field.Seed ^ 0x2c1b3c6du);
                var half = space.Field.HalfExtents;
                var patches = space.Field.Outline?.Patches;
                var rectangle = patches == null ? new[]
                {
                    new WheatFieldEdge(1, 0, half.x, true),
                    new WheatFieldEdge(-1, 0, half.x, true),
                    new WheatFieldEdge(0, 1, half.y, true),
                    new WheatFieldEdge(0, -1, half.y, true),
                } : null;
                for (int index = 0; index < indices.Length; index += 3)
                {
                    if ((index & 255) == 0) token.ThrowIfCancellationRequested();
                    int a = indices[index], b = indices[index + 1], c = indices[index + 2];
                    var face = new GroundTriangle(ground[a], ground[b], ground[c]);
                    if (!face.Covers(space.Bounds)) continue;

                    var normal = Vector3.Cross(face.B - face.A, face.C - face.A).normalized;
                    var va = new ClipVertex(space, face.A, hasNormals ? paintedNormals[a] : normal);
                    var vb = new ClipVertex(space, face.B, hasNormals ? paintedNormals[b] : normal);
                    var vc = new ClipVertex(space, face.C, hasNormals ? paintedNormals[c] : normal);
                    if (patches == null)
                        Slice(space, va, vb, vc, rectangle, tint);
                    else
                        SlicePatches(space, va, vb, vc, patches, tint);
                }
            }

            internal IslandPreparedMesh ToMesh()
            {
                return triangles.Count == 0 ? null : new IslandPreparedMesh(vertices.ToArray(),
                    normals.ToArray(), triangles.ToArray(), uv.ToArray(), colours.ToArray(), Array.Empty<Vector2>());
            }

            private void SlicePatches(FieldSpace space, ClipVertex a, ClipVertex b, ClipVertex c,
                WheatFieldPatch[] patches, float tint)
            {
                double minX = Math.Min(a.X, Math.Min(b.X, c.X)), maxX = Math.Max(a.X, Math.Max(b.X, c.X));
                double minZ = Math.Min(a.Z, Math.Min(b.Z, c.Z)), maxZ = Math.Max(a.Z, Math.Max(b.Z, c.Z));
                foreach (var patch in patches)
                    if (maxX > patch.Min.x && minX < patch.Max.x && maxZ > patch.Min.y && minZ < patch.Max.y)
                        Slice(space, a, b, c, patch.Edges, tint);
            }

            private void Slice(FieldSpace space, ClipVertex a, ClipVertex b, ClipVertex c,
                WheatFieldEdge[] edges, float tint)
            {
                var input = polygon;
                var output = scratch;
                input[0] = a;
                input[1] = b;
                input[2] = c;
                int count = 3;
                foreach (var edge in edges)
                {
                    count = Clip(input, output, count, edge);
                    if (count < 3) return;
                    (input, output) = (output, input);
                }
                AppendPolygon(space, input, count, edges, tint);
            }

            private void AppendPolygon(FieldSpace space, ClipVertex[] clipped, int count,
                WheatFieldEdge[] edges, float tint)
            {
                int start = vertices.Count;
                int previousIndexCount = triangles.Count;
                for (int i = 1; i < count - 1; i++)
                {
                    if (ClipVertex.Area(clipped[0], clipped[i], clipped[i + 1]) <= 1e-8d) continue;
                    triangles.Add(start);
                    triangles.Add(start + i);
                    triangles.Add(start + i + 1);
                }
                // Tile or field edges touching at a line alone do not own a patch or wall.
                if (triangles.Count == previousIndexCount) return;

                for (int i = 0; i < count; i++)
                {
                    var vertex = clipped[i];
                    AddVertex(space, vertex, vertex.Normal.normalized, CanopyHeight, tint);
                }

                for (int i = 0; i < count; i++)
                {
                    var a = clipped[i];
                    var b = clipped[(i + 1) % count];
                    if (!OnPerimeter(a, b, edges)) continue;
                    var delta = space.ToIsland(b.X, b.Z, 0) - space.ToIsland(a.X, a.Z, 0);
                    if (delta.sqrMagnitude > 1e-8f)
                        AddWall(space, a, b, new Vector3(-delta.z, 0f, delta.x).normalized, tint);
                }
            }

            private static bool OnPerimeter(ClipVertex a, ClipVertex b, WheatFieldEdge[] edges)
            {
                // Field walls only belong to the original perimeter, including faces
                // clipped across a tile seam or an internal triangulation edge.
                foreach (var edge in edges)
                    if (edge.Perimeter && Math.Abs(edge.Distance(a.X, a.Z)) <= EdgeTolerance
                        && Math.Abs(edge.Distance(b.X, b.Z)) <= EdgeTolerance)
                        return true;
                return false;
            }

            private void AddWall(FieldSpace space, ClipVertex a, ClipVertex b, Vector3 normal, float tint)
            {
                int start = vertices.Count;
                AddVertex(space, a, normal, CanopyHeight, tint);
                AddVertex(space, a, normal, 0f, tint);
                AddVertex(space, b, normal, CanopyHeight, tint);
                AddVertex(space, b, normal, 0f, tint);
                triangles.Add(start);
                triangles.Add(start + 1);
                triangles.Add(start + 2);
                triangles.Add(start + 2);
                triangles.Add(start + 1);
                triangles.Add(start + 3);
            }

            private void AddVertex(FieldSpace space, ClipVertex vertex, Vector3 normal, float rise, float tint)
            {
                vertices.Add(space.ToIsland(vertex.X, vertex.Z, vertex.Height + rise));
                normals.Add(normal);
                uv.Add(new Vector2((float)vertex.X, (float)vertex.Z));
                colours.Add(new Color(rise / CanopyHeight, tint, 0f, 1f));
            }

            private static int Clip(ClipVertex[] input, ClipVertex[] output, int count, WheatFieldEdge edge)
            {
                int written = 0;
                var previous = input[count - 1];
                double previousDistance = edge.Distance(previous.X, previous.Z);
                bool previousInside = previousDistance >= 0d;
                for (int index = 0; index < count; index++)
                {
                    var current = input[index];
                    double distance = edge.Distance(current.X, current.Z);
                    bool inside = distance >= 0d;
                    if (inside != previousInside)
                    {
                        // Interpolate in coordinate order so a shared terrain edge yields
                        // the same intersection when processed in either direction.
                        var intersection = previousDistance < distance
                            ? ClipVertex.Intersect(previous, current, previousDistance, distance, edge)
                            : ClipVertex.Intersect(current, previous, distance, previousDistance, edge);
                        Push(output, ref written, intersection);
                    }
                    if (inside) Push(output, ref written, current);
                    previous = current;
                    previousInside = inside;
                    previousDistance = distance;
                }
                if (written > 1 && output[written - 1].SamePoint(output[0])) written--;
                return written;
            }

            private static void Push(ClipVertex[] output, ref int count, ClipVertex vertex)
            {
                if (count == 0 || !output[count - 1].SamePoint(vertex)) output[count++] = vertex;
            }
        }

        private readonly struct ClipVertex
        {
            internal readonly double X;
            internal readonly double Z;
            internal readonly double Height;
            internal readonly Vector3 Normal;

            internal ClipVertex(FieldSpace space, Vector3 position, Vector3 normal)
            {
                space.ToField(position, out X, out Z);
                Height = position.y;
                Normal = normal;
            }

            private ClipVertex(double x, double z, double height, Vector3 normal)
            {
                X = x;
                Z = z;
                Height = height;
                Normal = normal;
            }

            internal static ClipVertex Intersect(
                ClipVertex from, ClipVertex to, double lower, double upper, WheatFieldEdge edge)
            {
                if (lower == 0d) return from;
                if (upper == 0d) return to;
                double t = -lower / (upper - lower);
                double x = from.X + (to.X - from.X) * t;
                double z = from.Z + (to.Z - from.Z) * t;
                double residual = edge.Distance(x, z);
                return new ClipVertex(
                    x - residual * edge.X,
                    z - residual * edge.Z,
                    from.Height + (to.Height - from.Height) * t,
                    Vector3.LerpUnclamped(from.Normal, to.Normal, (float)t));
            }

            internal bool SamePoint(ClipVertex other) => X == other.X && Z == other.Z;

            internal static double Area(ClipVertex a, ClipVertex b, ClipVertex c)
            {
                return (b.Z - a.Z) * (c.X - a.X) - (b.X - a.X) * (c.Z - a.Z);
            }
        }
    }
}
