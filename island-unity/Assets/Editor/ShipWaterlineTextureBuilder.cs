using System;
using System.Collections.Generic;
using Unity.Collections;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    internal static class ShipWaterlineTextureBuilder
    {
        [Serializable]
        internal sealed class Settings
        {
            public int resolution = 512;
            public float clearance = 1.5f;
            public float blend = 2;
            public float gapClosure = .1f;
            public bool largestRegionOnly = true;
            public bool bowWave = true;
            public float bowOffset = 1;
            public float bowWidth = 1.2f;
        }

        internal readonly struct Segment
        {
            internal readonly Vector2 A, B;
            internal Segment(Vector2 a, Vector2 b) { A = a; B = b; }
        }

        internal sealed class Result
        {
            internal Texture2D Texture;
            internal Rect SectionBounds;
            internal Rect TextureBounds;
            internal int EnclosedPixels;
            internal int Regions;
        }

        internal static List<Segment> Slice(Transform root, MeshFilter source, float worldY, Vector3 forward)
        {
            var segments = new List<Segment>();
            var right = Vector3.Cross(Vector3.up, forward).normalized;
            var origin = root.position;
            foreach (var filter in source != null ? new[] { source } : root.GetComponentsInChildren<MeshFilter>())
            {
                var renderer = filter.GetComponent<Renderer>();
                if (filter.sharedMesh == null || (renderer != null && !renderer.enabled)) continue;
                AddMesh(filter.sharedMesh, filter.transform.localToWorldMatrix, origin, right, forward, worldY, segments);
            }
            if (source == null)
                foreach (var renderer in root.GetComponentsInChildren<SkinnedMeshRenderer>())
                {
                    if (!renderer.enabled || renderer.sharedMesh == null) continue;
                    var baked = new Mesh();
                    try
                    {
                        renderer.BakeMesh(baked);
                        AddMesh(baked, renderer.transform.localToWorldMatrix, origin, right, forward, worldY, segments);
                    }
                    finally { UnityEngine.Object.DestroyImmediate(baked); }
                }
            return segments;
        }

        private static void AddMesh(Mesh mesh, Matrix4x4 localToWorld, Vector3 origin, Vector3 right,
            Vector3 forward, float worldY, List<Segment> segments)
        {
            // Editor access does not require changing Read/Write on the imported asset.
            using var snapshot = MeshUtility.AcquireReadOnlyMeshData(mesh);
            var data = snapshot[0];
            using var vertices = new NativeArray<Vector3>(data.vertexCount, Allocator.Temp);
            data.GetVertices(vertices);
            var projected = new Vector3[vertices.Length];
            for (var i = 0; i < vertices.Length; i++)
            {
                var world = localToWorld.MultiplyPoint3x4(vertices[i]);
                var relative = world - origin;
                projected[i] = new Vector3(Vector3.Dot(relative, right), world.y - worldY, Vector3.Dot(relative, forward));
            }
            for (var submesh = 0; submesh < data.subMeshCount; submesh++)
            {
                var descriptor = data.GetSubMesh(submesh);
                if (descriptor.topology != MeshTopology.Triangles) continue;
                using var indices = new NativeArray<int>(descriptor.indexCount, Allocator.Temp);
                data.GetIndices(indices, submesh);
                for (var i = 0; i + 2 < indices.Length; i += 3)
                    IntersectTriangle(projected[indices[i]], projected[indices[i+1]], projected[indices[i+2]], segments);
            }
        }

        internal static void IntersectTriangle(Vector3 a, Vector3 b, Vector3 c, List<Segment> output)
        {
            const float epsilon = .00001f;
            if ((a.y > epsilon && b.y > epsilon && c.y > epsilon) ||
                (a.y < -epsilon && b.y < -epsilon && c.y < -epsilon)) return;
            var first = Vector2.zero;
            var second = Vector2.zero;
            var count = 0;
            void Point(Vector3 p)
            {
                var value = new Vector2(p.x, p.z);
                if (count == 0) { first = value; count = 1; }
                else if ((value-first).sqrMagnitude > epsilon * epsilon) { second = value; count = 2; }
            }
            void Edge(Vector3 p, Vector3 q)
            {
                if (Mathf.Abs(p.y) <= epsilon && Mathf.Abs(q.y) <= epsilon)
                    output.Add(new Segment(new Vector2(p.x, p.z), new Vector2(q.x, q.z)));
                else if (Mathf.Abs(p.y) <= epsilon) Point(p);
                else if (Mathf.Abs(q.y) <= epsilon) Point(q);
                else if ((p.y < 0) != (q.y < 0)) Point(Vector3.LerpUnclamped(p, q, p.y / (p.y-q.y)));
            }
            Edge(a, b); Edge(b, c); Edge(c, a);
            if (count == 2) output.Add(new Segment(first, second));
        }

        internal static Result Build(IReadOnlyList<Segment> segments, Settings settings)
        {
            if (segments.Count == 0) throw new InvalidOperationException("The waterline does not intersect the selected mesh. Adjust Waterline World Y or choose another source mesh.");
            var min = segments[0].A;
            var max = min;
            foreach (var segment in segments)
            {
                min = Vector2.Min(min, Vector2.Min(segment.A, segment.B));
                max = Vector2.Max(max, Vector2.Max(segment.A, segment.B));
            }
            var clearance = Mathf.Max(0, settings.clearance);
            var blend = Mathf.Max(.01f, settings.blend);
            var bowWidth = Mathf.Max(.1f, settings.bowWidth);
            var padding = clearance + blend + (settings.bowWave ? Mathf.Max(0, settings.bowOffset) + bowWidth * 4 : 0) + 1;
            var section = Rect.MinMaxRect(min.x, min.y, max.x, max.y);
            var rect = Rect.MinMaxRect(min.x-padding, min.y-padding, max.x+padding, max.y+padding);
            var resolution = Mathf.Clamp(settings.resolution, 64, 1024);
            var pixelSize = Mathf.Max(rect.width, rect.height) / resolution;
            var width = Mathf.Max(4, Mathf.CeilToInt(rect.width / pixelSize));
            var height = Mathf.Max(4, Mathf.CeilToInt(rect.height / pixelSize));
            rect.size = new Vector2(width, height) * pixelSize;
            rect.center = section.center;
            var boundary = new bool[width * height];
            foreach (var segment in segments)
            {
                var a = (segment.A-rect.min) / pixelSize;
                var b = (segment.B-rect.min) / pixelSize;
                var steps = Mathf.Max(1, Mathf.CeilToInt(Vector2.Distance(a, b) * 4));
                for (var i = 0; i <= steps; i++)
                {
                    var p = Vector2.Lerp(a, b, i / (float)steps);
                    boundary[Mathf.Clamp((int)p.y, 0, height-1) * width + Mathf.Clamp((int)p.x, 0, width-1)] = true;
                }
            }
            // Seal only explicitly configured small gaps, without convexifying the hull.
            var gapPixels = Mathf.Clamp(Mathf.CeilToInt(Mathf.Max(0, settings.gapClosure) * .5f / pixelSize), 0, 16);
            if (gapPixels > 0)
            {
                var original = (bool[])boundary.Clone();
                for (var y = gapPixels; y < height-gapPixels; y++)
                for (var x = gapPixels; x < width-gapPixels; x++)
                    if (original[y*width+x])
                        for (var dy = -gapPixels; dy <= gapPixels; dy++)
                        for (var dx = -gapPixels; dx <= gapPixels; dx++)
                            if (dx*dx+dy*dy <= gapPixels*gapPixels) boundary[(y+dy)*width+x+dx] = true;
            }
            var visited = new bool[boundary.Length];
            var queue = new int[boundary.Length];
            List<int> Flood(int seed)
            {
                var head = 0;
                var tail = 1;
                queue[0] = seed;
                visited[seed] = true;
                while (head < tail)
                {
                    var p = queue[head++];
                    var x = p % width;
                    void Visit(int next)
                    {
                        if (next < 0 || next >= boundary.Length || visited[next] || boundary[next]) return;
                        visited[next] = true;
                        queue[tail++] = next;
                    }
                    if (x > 0) Visit(p-1);
                    if (x+1 < width) Visit(p+1);
                    Visit(p-width); Visit(p+width);
                }
                var pixels = new List<int>(tail);
                for (var i = 0; i < tail; i++) pixels.Add(queue[i]);
                return pixels;
            }
            Flood(0); // Padding guarantees an exterior seed and connected outside border.
            var drawnBoundary = boundary;
            boundary = visited; // Everything not reachable from outside belongs to the silhouette.
            visited = new bool[boundary.Length];
            var regions = new List<List<int>>();
            for (var i = 0; i < boundary.Length; i++)
                if (!boundary[i] && !visited[i])
                {
                    var region = Flood(i);
                    if (region.Exists(pixel => !drawnBoundary[pixel])) regions.Add(region);
                }
            if (regions.Count == 0) throw new InvalidOperationException("The slice has no enclosed hull region. The mesh may be open at this height; adjust the waterline or increase Gap Closure slightly.");
            regions.Sort((a,b) => b.Count.CompareTo(a.Count));
            var distance = new float[boundary.Length];
            Array.Fill(distance, 1000000f);
            var enclosed = 0;
            for (var region = 0; region < (settings.largestRegionOnly ? 1 : regions.Count); region++)
                foreach (var pixel in regions[region]) { distance[pixel] = 0; enclosed++; }
            // Eight-neighbour chamfer distance is sufficient at authoring texel resolution.
            const float diagonal = 1.41421356f;
            for (var y = 0; y < height; y++)
            for (var x = 0; x < width; x++)
            {
                var i = y*width+x;
                if (x > 0) distance[i] = Mathf.Min(distance[i], distance[i-1]+1);
                if (y > 0)
                {
                    distance[i] = Mathf.Min(distance[i], distance[i-width]+1);
                    if (x > 0) distance[i] = Mathf.Min(distance[i], distance[i-width-1]+diagonal);
                    if (x+1 < width) distance[i] = Mathf.Min(distance[i], distance[i-width+1]+diagonal);
                }
            }
            for (var y = height-1; y >= 0; y--)
            for (var x = width-1; x >= 0; x--)
            {
                var i = y*width+x;
                if (x+1 < width) distance[i] = Mathf.Min(distance[i], distance[i+1]+1);
                if (y+1 < height)
                {
                    distance[i] = Mathf.Min(distance[i], distance[i+width]+1);
                    if (x > 0) distance[i] = Mathf.Min(distance[i], distance[i+width-1]+diagonal);
                    if (x+1 < width) distance[i] = Mathf.Min(distance[i], distance[i+width+1]+diagonal);
                }
            }
            var colours = new Color32[distance.Length];
            for (var y = 0; y < height; y++)
            for (var x = 0; x < width; x++)
            {
                var i = y*width+x;
                var d = distance[i]*pixelSize;
                var down = 1-Mathf.SmoothStep(0, 1, Mathf.InverseLerp(clearance, clearance+blend, d));
                var ridgeDistance = clearance+blend+Mathf.Max(0, settings.bowOffset);
                var bow = Mathf.Exp(-Mathf.Pow((d-ridgeDistance)/bowWidth, 2));
                var z = rect.yMin+(y+.5f)*pixelSize;
                bow *= Mathf.SmoothStep(0, 1, Mathf.InverseLerp(section.yMin+section.height*.45f, section.yMax, z));
                var value = down > 0 ? -down : settings.bowWave ? bow : 0;
                var grey = (byte)Mathf.Clamp(Mathf.RoundToInt(128+value*(value < 0 ? 128 : 127)), 0, 255);
                if (x == 0 || y == 0 || x == width-1 || y == height-1) grey = 128;
                colours[i] = new Color32(grey, grey, grey, 255);
            }
            var texture = new Texture2D(width, height, TextureFormat.RGBA32, false, true)
            { name = "Waterline wave texture preview", filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp };
            texture.SetPixels32(colours);
            texture.Apply();
            return new Result { Texture = texture, SectionBounds = section, TextureBounds = rect, EnclosedPixels = enclosed, Regions = regions.Count };
        }
    }
}
