using System;
using System.Collections.Generic;
using UnityEngine;

namespace Motu.Islands
{
    internal sealed class WheatFieldOutline
    {
        private const double AreaTolerance = 1e-8;
        private const double CollinearTolerance = 0.0005;
        private readonly Vector2[] vertices;
        internal readonly WheatFieldPatch[] Patches;

        private WheatFieldOutline(Vector2[] vertices)
        {
            this.vertices = vertices;
            Patches = Triangulate(vertices);
        }

        internal static WheatField FromNative(float[] outline, float scale, uint seed)
        {
            if (outline == null || outline.Length < 9 || outline.Length % 3 != 0)
                throw new InvalidOperationException("The native roadside wheat outline is invalid.");

            var points = new Vector2[outline.Length / 3];
            var centre = Vector2.zero;
            for (int i = 0; i < points.Length; i++)
            {
                var x = outline[i * 3];
                var z = outline[i * 3 + 1];
                if (!float.IsFinite(x) || !float.IsFinite(z))
                    throw new InvalidOperationException("The native roadside wheat outline is invalid.");
                points[i] = new Vector2(x - 1000f, z - 1000f) * scale;
                centre += points[i];
            }
            centre /= points.Length;

            // The plan starts on the road-facing edge. Align the seed rows with its first span.
            var frontage = points[1] - points[0];
            var yaw = Mathf.Atan2(frontage.y, frontage.x);
            double cosine = Math.Cos(yaw), sine = Math.Sin(yaw);
            var halfExtents = Vector2.zero;
            points = RemoveCollinearPoints(points);
            for (int i = 0; i < points.Length; i++)
            {
                var delta = points[i] - centre;
                var point = new Vector2((float)(delta.x * cosine + delta.y * sine),
                    (float)(-delta.x * sine + delta.y * cosine));
                points[i] = point;
                halfExtents = Vector2.Max(halfExtents, new Vector2(Mathf.Abs(point.x), Mathf.Abs(point.y)));
            }

            return new WheatField(centre, halfExtents, yaw, seed, new WheatFieldOutline(points));
        }

        private static Vector2[] RemoveCollinearPoints(Vector2[] points)
        {
            // Straight station runs are redundant. Remove them before rotation so
            // roundoff cannot introduce sliver triangles along a field edge.
            var corners = new List<Vector2>(points.Length);
            for (int i = 0; i < points.Length; i++)
            {
                var a = points[(i + points.Length - 1) % points.Length];
                var b = points[i];
                var c = points[(i + 1) % points.Length];
                double dx = (double)c.x - a.x, dz = (double)c.y - a.y;
                var length = Math.Sqrt(dx * dx + dz * dz);
                var along = ((double)b.x - a.x) * dx + ((double)b.y - a.y) * dz;
                if (length > 0d && along >= 0d && along <= length * length
                    && Math.Abs(Cross(a, b, c)) <= CollinearTolerance * length)
                    continue;
                corners.Add(b);
            }
            if (corners.Count < 3)
                throw new InvalidOperationException("The native roadside wheat outline is degenerate.");
            return corners.ToArray();
        }

        internal double InwardDistance(double x, double z)
        {
            bool inside = false;
            double distanceSquared = double.PositiveInfinity;
            var a = vertices[vertices.Length - 1];
            foreach (var b in vertices)
            {
                double dx = (double)b.x - a.x, dz = (double)b.y - a.y;
                double offsetX = x - a.x, offsetZ = z - a.y;
                if ((a.y > z) != (b.y > z) && offsetX < dx * offsetZ / dz)
                    inside = !inside;

                var lengthSquared = dx * dx + dz * dz;
                if (lengthSquared > 0d)
                {
                    var t = Math.Clamp((offsetX * dx + offsetZ * dz) / lengthSquared, 0d, 1d);
                    double edgeX = offsetX - t * dx, edgeZ = offsetZ - t * dz;
                    distanceSquared = Math.Min(distanceSquared, edgeX * edgeX + edgeZ * edgeZ);
                }
                a = b;
            }
            return inside ? Math.Sqrt(distanceSquared) : 0d;
        }

        private static WheatFieldPatch[] Triangulate(Vector2[] points)
        {
            var ring = new List<int>(points.Length);
            double area = 0d;
            for (int i = 0; i < points.Length; i++)
            {
                ring.Add(i);
                area += Cross(Vector2.zero, points[i], points[(i + 1) % points.Length]);
            }
            if (area < 0d) ring.Reverse();

            var patches = new List<WheatFieldPatch>(points.Length - 2);
            while (ring.Count > 3)
            {
                var ear = FindEar(points, ring);
                if (ear < 0)
                    throw new InvalidOperationException("The native roadside wheat outline could not be triangulated.");
                int a = ring[(ear + ring.Count - 1) % ring.Count], b = ring[ear], c = ring[(ear + 1) % ring.Count];
                patches.Add(Triangle(points, a, b, c));
                ring.RemoveAt(ear);
            }
            if (Cross(points[ring[0]], points[ring[1]], points[ring[2]]) > AreaTolerance)
                patches.Add(Triangle(points, ring[0], ring[1], ring[2]));
            if (patches.Count == 0)
                throw new InvalidOperationException("The native roadside wheat outline is degenerate.");
            return patches.ToArray();
        }

        private static int FindEar(Vector2[] points, List<int> ring)
        {
            for (int i = 0; i < ring.Count; i++)
            {
                int previous = ring[(i + ring.Count - 1) % ring.Count];
                int next = ring[(i + 1) % ring.Count];
                var a = points[previous];
                var b = points[ring[i]];
                var c = points[next];
                if (Cross(a, b, c) > AreaTolerance && !CoversOtherPoint(points, ring, previous, ring[i], next))
                    return i;
            }
            return -1;
        }

        private static bool CoversOtherPoint(Vector2[] points, List<int> ring, int a, int b, int c)
        {
            foreach (var index in ring)
            {
                if (index == a || index == b || index == c) continue;
                var point = points[index];
                if (Cross(points[a], points[b], point) >= -AreaTolerance
                    && Cross(points[b], points[c], point) >= -AreaTolerance
                    && Cross(points[c], points[a], point) >= -AreaTolerance)
                    return true;
            }
            return false;
        }

        private static WheatFieldPatch Triangle(Vector2[] points, int a, int b, int c)
        {
            return new WheatFieldPatch(points[a], points[b], points[c], new[]
            {
                Edge(points, a, b), Edge(points, b, c), Edge(points, c, a)
            });
        }

        private static WheatFieldEdge Edge(Vector2[] points, int a, int b)
        {
            double dx = (double)points[b].x - points[a].x, dz = (double)points[b].y - points[a].y;
            double length = Math.Sqrt(dx * dx + dz * dz);
            double x = -dz / length, z = dx / length;
            int gap = Math.Abs(a - b);
            var origin = points[Math.Min(a, b)];
            return new WheatFieldEdge(x, z, -(x * origin.x + z * origin.y),
                gap == 1 || gap == points.Length - 1);
        }

        private static double Cross(Vector2 a, Vector2 b, Vector2 c)
        {
            return ((double)b.x - a.x) * ((double)c.y - a.y) - ((double)b.y - a.y) * ((double)c.x - a.x);
        }
    }

    internal readonly struct WheatFieldPatch
    {
        internal readonly Vector2 Min;
        internal readonly Vector2 Max;
        internal readonly WheatFieldEdge[] Edges;

        internal WheatFieldPatch(Vector2 a, Vector2 b, Vector2 c, WheatFieldEdge[] edges)
        {
            Min = Vector2.Min(a, Vector2.Min(b, c));
            Max = Vector2.Max(a, Vector2.Max(b, c));
            Edges = edges;
        }
    }

    internal readonly struct WheatFieldEdge
    {
        internal readonly double X;
        internal readonly double Z;
        internal readonly bool Perimeter;
        private readonly double offset;

        internal WheatFieldEdge(double x, double z, double offset, bool perimeter)
        {
            X = x;
            Z = z;
            this.offset = offset;
            Perimeter = perimeter;
        }

        internal double Distance(double x, double z) => x * X + z * Z + offset;
    }
}
