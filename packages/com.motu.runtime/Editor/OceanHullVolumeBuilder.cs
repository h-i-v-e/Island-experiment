using System;
using System.Collections.Generic;
using Motu.Rendering;
using Unity.Collections;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Editor
{
    /// <summary>Editor-only convex envelope; no collider cooking or mesh reads at runtime.</summary>
    public static class OceanHullVolumeBuilder
    {
        private readonly struct Face
        {
            public readonly int A, B, C;
            public readonly Vector4 Plane;
            public Face(int a, int b, int c, List<Vector3> points, Vector3 interior)
            {
                var normal = Vector3.Cross(points[b] - points[a], points[c] - points[a]).normalized;
                if (Vector3.Dot(normal, interior - points[a]) > 0)
                { (b, c) = (c, b); normal = -normal; }
                A = a; B = b; C = c;
                Plane = new Vector4(normal.x, normal.y, normal.z, -Vector3.Dot(normal, points[a]));
            }
            public float Distance(Vector3 point) => Vector3.Dot(Plane, point) + Plane.w;
        }

        public static void Bake(OceanHullVolume volume, Mesh source, Matrix4x4 sourceToHull)
        {
            if (volume == null || source == null) throw new ArgumentException("Choose a hull volume and source hull mesh.");
            var points = new List<Vector3>();
            var unique = new HashSet<Vector3Int>();
            using (var snapshot = MeshUtility.AcquireReadOnlyMeshData(source))
            {
                using var vertices = new NativeArray<Vector3>(snapshot[0].vertexCount, Allocator.Temp);
                snapshot[0].GetVertices(vertices);
                foreach (var sourcePoint in vertices)
                {
                    var p = sourceToHull.MultiplyPoint3x4(sourcePoint);
                    if (unique.Add(Vector3Int.RoundToInt(p * 1000))) points.Add(p);
                }
            }
            BakePoints(volume, points);
        }

        public static void BakePoints(OceanHullVolume volume, List<Vector3> points)
        {
            if (points == null || points.Count < 4) throw new ArgumentException("Hull needs at least four points.");
            var bounds = new Bounds(points[0], Vector3.zero);
            foreach (var p in points) bounds.Encapsulate(p);
            float epsilon = Mathf.Max(.0001f, bounds.size.magnitude * .00001f);
            int a = 0, b = 0, c = 0, d = 0;
            int axis = bounds.size.x >= bounds.size.y && bounds.size.x >= bounds.size.z ? 0 : bounds.size.y >= bounds.size.z ? 1 : 2;
            for (int i = 1; i < points.Count; i++)
            {
                if (points[i][axis] < points[a][axis]) a = i;
                if (points[i][axis] > points[b][axis]) b = i;
            }
            var edge = points[b] - points[a];
            if (edge.sqrMagnitude < epsilon * epsilon) throw new ArgumentException("Degenerate hull.");
            float farthest = 0;
            for (int i = 0; i < points.Count; i++)
            {
                float distance = Vector3.Cross(points[i] - points[a], edge).sqrMagnitude / edge.sqrMagnitude;
                if (distance > farthest) { farthest = distance; c = i; }
            }
            if (farthest < epsilon * epsilon) throw new ArgumentException("Collinear hull points.");
            var normal = Vector3.Cross(edge, points[c] - points[a]).normalized;
            farthest = 0;
            for (int i = 0; i < points.Count; i++)
            {
                float distance = Mathf.Abs(Vector3.Dot(normal, points[i] - points[a]));
                if (distance > farthest) { farthest = distance; d = i; }
            }
            if (farthest < epsilon) throw new ArgumentException("Coplanar hull points.");
            var interior = (points[a] + points[b] + points[c] + points[d]) * .25f;
            var faces = new List<Face>
            {
                new Face(a, b, c, points, interior), new Face(a, d, b, points, interior),
                new Face(a, c, d, points, interior), new Face(b, d, c, points, interior),
            };
            var horizon = new Dictionary<(int, int), (int, int)>();
            void Edge(int first, int second)
            {
                var key = first < second ? (first, second) : (second, first);
                if (!horizon.Remove(key)) horizon.Add(key, (first, second));
            }
            for (int p = 0; p < points.Count; p++)
            {
                horizon.Clear();
                for (int f = faces.Count - 1; f >= 0; f--)
                {
                    var face = faces[f];
                    if (face.Distance(points[p]) <= epsilon) continue;
                    Edge(face.A, face.B); Edge(face.B, face.C); Edge(face.C, face.A);
                    faces.RemoveAt(f);
                }
                foreach (var pair in horizon.Values) faces.Add(new Face(pair.Item1, pair.Item2, p, points, interior));
            }

            // Merge coplanar triangles. If needed, retain angularly diverse support
            // planes: this expands the envelope rather than cutting into the ship.
            var candidates = new List<Vector3>();
            foreach (var face in faces)
            {
                Vector3 n = face.Plane;
                if (n.sqrMagnitude < .5f || candidates.Exists(other => Vector3.Dot(n, other) > .999999f)) continue;
                candidates.Add(n);
            }
            var normals = new List<Vector3> { Vector3.right, Vector3.left, Vector3.up, Vector3.down, Vector3.forward, Vector3.back };
            while (normals.Count < OceanHullVolume.MaximumPlanes && candidates.Count > 0)
            {
                int best = 0;
                float bestSeparation = -1;
                for (int i = 0; i < candidates.Count; i++)
                {
                    float similarity = -1;
                    foreach (var n in normals) similarity = Mathf.Max(similarity, Vector3.Dot(n, candidates[i]));
                    if (1 - similarity > bestSeparation) { bestSeparation = 1 - similarity; best = i; }
                }
                if (bestSeparation < .000001f) break;
                normals.Add(candidates[best]);
                candidates.RemoveAt(best);
            }
            var planes = new Vector4[normals.Count];
            for (int i = 0; i < planes.Length; i++)
            {
                float support = float.NegativeInfinity;
                foreach (var point in points) support = Mathf.Max(support, Vector3.Dot(normals[i], point));
                planes[i] = new Vector4(normals[i].x, normals[i].y, normals[i].z, -support);
            }
            var mesh = BoundaryMesh(planes, bounds.size.magnitude * 2);
            mesh.name = volume.name + " Boundary";
            var previous = volume.Boundary;
            volume.SetGeometry(planes, mesh);
            if (AssetDatabase.Contains(volume))
            {
                AssetDatabase.AddObjectToAsset(mesh, volume);
                if (previous != null && AssetDatabase.IsSubAsset(previous)) UnityEngine.Object.DestroyImmediate(previous, true);
                EditorUtility.SetDirty(volume);
            }
            else if (previous != null) UnityEngine.Object.DestroyImmediate(previous);
        }

        private static Mesh BoundaryMesh(Vector4[] planes, float extent)
        {
            var vertices = new List<Vector3>();
            var triangles = new List<int>();
            for (int face = 0; face < planes.Length; face++)
            {
                Vector3 n = planes[face];
                var origin = -planes[face].w * n;
                var u = Vector3.Cross(n, Mathf.Abs(n.y) < .9f ? Vector3.up : Vector3.right).normalized * extent;
                var v = Vector3.Cross(n, u).normalized * extent;
                var polygon = new List<Vector3> { origin - u - v, origin + u - v, origin + u + v, origin - u + v };
                for (int clip = 0; clip < planes.Length && polygon.Count > 0; clip++)
                {
                    if (clip == face) continue;
                    var output = new List<Vector3>();
                    Vector3 cn = planes[clip];
                    for (int i = 0; i < polygon.Count; i++)
                    {
                        var a = polygon[i]; var b = polygon[(i + 1) % polygon.Count];
                        float da = Vector3.Dot(cn, a) + planes[clip].w, db = Vector3.Dot(cn, b) + planes[clip].w;
                        bool insideA = da <= .00001f, insideB = db <= .00001f;
                        if (insideA) output.Add(a);
                        if (insideA != insideB) output.Add(Vector3.LerpUnclamped(a, b, da / (da - db)));
                    }
                    polygon = output;
                }
                if (polygon.Count < 3) continue;
                int first = vertices.Count;
                vertices.AddRange(polygon);
                for (int i = 1; i + 1 < polygon.Count; i++)
                {
                    if (Vector3.Cross(polygon[i] - polygon[0], polygon[i + 1] - polygon[0]).sqrMagnitude < 1e-12f) continue;
                    triangles.Add(first); triangles.Add(first + i); triangles.Add(first + i + 1);
                }
            }
            if (triangles.Count < 12) throw new InvalidOperationException("Could not close the hull volume.");
            var mesh = new Mesh { indexFormat = vertices.Count > 65535 ? IndexFormat.UInt32 : IndexFormat.UInt16 };
            mesh.SetVertices(vertices); mesh.SetTriangles(triangles, 0); mesh.RecalculateNormals(); mesh.RecalculateBounds();
            return mesh;
        }
    }
}
