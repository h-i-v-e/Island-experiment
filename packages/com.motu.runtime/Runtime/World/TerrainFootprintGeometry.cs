using System;
using UnityEngine;

namespace Motu.World
{
    internal static class TerrainFootprintGeometry
    {
        internal const int PolygonCapacity = 16;

        internal static bool TryGetCorners(TerrainFootprint footprint, Span<Vector2> corners,
            out Rect bounds, out double area)
        {
            bounds = default;
            area = 0d;
            if (corners.Length < 4 || !Finite(footprint.Center.x) || !Finite(footprint.Center.y)
                || !Finite(footprint.Size.x) || !Finite(footprint.Size.y)
                || !Finite(footprint.YawDegrees) || footprint.Size.x <= 0f || footprint.Size.y <= 0f)
                return false;

            var yaw = footprint.YawDegrees % 360f * Mathf.Deg2Rad;
            var right = new Vector2(Mathf.Cos(yaw), -Mathf.Sin(yaw)) * (footprint.Size.x * .5f);
            var forward = new Vector2(Mathf.Sin(yaw), Mathf.Cos(yaw)) * (footprint.Size.y * .5f);
            corners[0] = footprint.Center - right - forward;
            corners[1] = footprint.Center + right - forward;
            corners[2] = footprint.Center + right + forward;
            corners[3] = footprint.Center - right + forward;
            var minimum = corners[0];
            var maximum = corners[0];
            for (var i = 0; i < 4; i++)
            {
                if (!Finite(corners[i].x) || !Finite(corners[i].y)) return false;
                minimum = Vector2.Min(minimum, corners[i]);
                maximum = Vector2.Max(maximum, corners[i]);
            }
            bounds = Rect.MinMaxRect(minimum.x, minimum.y, maximum.x, maximum.y);
            area = Area(corners.Slice(0, 4));
            return area > 0d && Finite(bounds.width) && Finite(bounds.height);
        }

        // Input, output and scratch must not overlap. Successive clips preserve
        // convexity; a footprint clipped by section and grid rectangles fits 16 vertices.
        internal static int ClipToRect(ReadOnlySpan<Vector2> polygon, Rect rectangle,
            Span<Vector2> output, Span<Vector2> scratch)
        {
            polygon.CopyTo(output);
            var count = ClipAxis(output.Slice(0, polygon.Length), scratch, 0, rectangle.xMin, true);
            count = ClipAxis(scratch.Slice(0, count), output, 0, rectangle.xMax, false);
            count = ClipAxis(output.Slice(0, count), scratch, 1, rectangle.yMin, true);
            return ClipAxis(scratch.Slice(0, count), output, 1, rectangle.yMax, false);
        }

        private static int ClipAxis(ReadOnlySpan<Vector2> input, Span<Vector2> output,
            int axis, float boundary, bool keepGreater)
        {
            if (input.Length == 0) return 0;
            var count = 0;
            var previous = input[input.Length - 1];
            var previousInside = keepGreater ? previous[axis] >= boundary : previous[axis] <= boundary;
            foreach (var current in input)
            {
                var inside = keepGreater ? current[axis] >= boundary : current[axis] <= boundary;
                if (inside != previousInside)
                {
                    var t = ((double)boundary - previous[axis]) / ((double)current[axis] - previous[axis]);
                    var intersection = new Vector2((float)(previous.x + t * ((double)current.x - previous.x)),
                        (float)(previous.y + t * ((double)current.y - previous.y)));
                    intersection[axis] = boundary;
                    output[count++] = intersection;
                }
                if (inside) output[count++] = current;
                previous = current;
                previousInside = inside;
            }
            return count;
        }

        internal static double Area(ReadOnlySpan<Vector2> polygon)
        {
            if (polygon.Length < 3) return 0d;
            // Translate before the products so large world coordinates do not
            // cancel a small building's area in the shoelace formula.
            var origin = polygon[0];
            var sum = 0d;
            for (var i = 1; i + 1 < polygon.Length; i++)
            {
                var a = polygon[i];
                var b = polygon[i + 1];
                sum += ((double)a.x - origin.x) * ((double)b.y - origin.y)
                    - ((double)a.y - origin.y) * ((double)b.x - origin.x);
            }
            return Math.Abs(sum) * .5d;
        }

        internal static bool HasSupportedTransform(Transform owner)
        {
            var matrix = owner.localToWorldMatrix;
            var scale = owner.lossyScale;
            return Finite(owner.position.x) && Finite(owner.position.y) && Finite(owner.position.z)
                && Mathf.Approximately(scale.x, 1f) && Mathf.Approximately(scale.y, 1f)
                && Mathf.Approximately(scale.z, 1f) && matrix.m11 > 0f
                && matrix.m01 == 0f && matrix.m21 == 0f && matrix.m10 == 0f && matrix.m12 == 0f
                && Finite(matrix.m00) && Finite(matrix.m02) && Finite(matrix.m20) && Finite(matrix.m22)
                && Math.Abs((double)matrix.m00 * matrix.m02 + (double)matrix.m20 * matrix.m22) < .00001d;
        }

        private static bool Finite(float value) => !float.IsNaN(value) && !float.IsInfinity(value);
    }
}
