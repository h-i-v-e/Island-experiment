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
                for (int index = 0; index < indices.Length; index += 3)
                {
                    if ((index & 255) == 0) token.ThrowIfCancellationRequested();
                    int a = indices[index], b = indices[index + 1], c = indices[index + 2];
                    var face = new GroundTriangle(ground[a], ground[b], ground[c]);
                    if (!face.Covers(space.Bounds)) continue;

                    var normal = Vector3.Cross(face.B - face.A, face.C - face.A).normalized;
                    polygon[0] = new ClipVertex(space, face.A, hasNormals ? paintedNormals[a] : normal);
                    polygon[1] = new ClipVertex(space, face.B, hasNormals ? paintedNormals[b] : normal);
                    polygon[2] = new ClipVertex(space, face.C, hasNormals ? paintedNormals[c] : normal);
                    int count = Clip(polygon, scratch, 3, true, -half.x, true);
                    count = Clip(scratch, polygon, count, true, half.x, false);
                    count = Clip(polygon, scratch, count, false, -half.y, true);
                    count = Clip(scratch, polygon, count, false, half.y, false);
                    if (count >= 3) AppendPolygon(space, count, tint);
                }
            }

            internal IslandPreparedMesh ToMesh()
            {
                return triangles.Count == 0 ? null : new IslandPreparedMesh(vertices.ToArray(),
                    normals.ToArray(), triangles.ToArray(), uv.ToArray(), colours.ToArray(), Array.Empty<Vector2>());
            }

            private void AppendPolygon(FieldSpace space, int count, float tint)
            {
                int start = vertices.Count;
                int previousIndexCount = triangles.Count;
                for (int i = 1; i < count - 1; i++)
                {
                    if (ClipVertex.Area(polygon[0], polygon[i], polygon[i + 1]) <= 1e-8d) continue;
                    triangles.Add(start);
                    triangles.Add(start + i);
                    triangles.Add(start + i + 1);
                }
                // Tile or field edges touching at a line alone do not own a patch or wall.
                if (triangles.Count == previousIndexCount) return;

                for (int i = 0; i < count; i++)
                {
                    var vertex = polygon[i];
                    AddVertex(space, vertex, vertex.Normal.normalized, CanopyHeight, tint);
                }

                var half = space.Field.HalfExtents;
                for (int i = 0; i < count; i++)
                {
                    var a = polygon[i];
                    var b = polygon[(i + 1) % count];
                    // The clipper pins field-boundary coordinates exactly. Mesh and tile
                    // edges inside the rectangle are shared by tops alone.
                    bool fieldEdge = (a.X == b.X && Math.Abs(a.X) == half.x)
                        || (a.Z == b.Z && Math.Abs(a.Z) == half.y);
                    if (!fieldEdge) continue;
                    var delta = space.ToIsland(b.X, b.Z, 0) - space.ToIsland(a.X, a.Z, 0);
                    if (delta.sqrMagnitude > 1e-8f)
                        AddWall(space, a, b, new Vector3(-delta.z, 0f, delta.x).normalized, tint);
                }
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

            private static int Clip(
                ClipVertex[] input, ClipVertex[] output, int count, bool xAxis, double limit, bool keepGreater)
            {
                if (count == 0) return 0;

                int written = 0;
                var previous = input[count - 1];
                double previousCoordinate = xAxis ? previous.X : previous.Z;
                bool previousInside = keepGreater ? previousCoordinate >= limit : previousCoordinate <= limit;
                for (int index = 0; index < count; index++)
                {
                    var current = input[index];
                    double coordinate = xAxis ? current.X : current.Z;
                    bool inside = keepGreater ? coordinate >= limit : coordinate <= limit;
                    if (inside != previousInside)
                    {
                        // Interpolate in coordinate order so a shared terrain edge yields
                        // the same intersection when processed in either direction.
                        var intersection = previousCoordinate < coordinate
                            ? ClipVertex.Intersect(previous, current, previousCoordinate, coordinate, xAxis, limit)
                            : ClipVertex.Intersect(current, previous, coordinate, previousCoordinate, xAxis, limit);
                        Push(output, ref written, intersection);
                    }
                    if (inside) Push(output, ref written, current);
                    previous = current;
                    previousInside = inside;
                    previousCoordinate = coordinate;
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
                ClipVertex from, ClipVertex to, double lower, double upper, bool xAxis, double limit)
            {
                if (limit == lower) return from;
                if (limit == upper) return to;
                double t = (limit - lower) / (upper - lower);
                return new ClipVertex(
                    xAxis ? limit : from.X + (to.X - from.X) * t,
                    xAxis ? from.Z + (to.Z - from.Z) * t : limit,
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
