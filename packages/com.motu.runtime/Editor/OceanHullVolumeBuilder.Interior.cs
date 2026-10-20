using System;
using System.Collections.Generic;
using Motu.Rendering;
using Unity.Collections;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Editor
{
    public static partial class OceanHullVolumeBuilder
    {
        /// <summary>
        /// Fits a conservative convex core to the largest connected shell below
        /// the deck. Detached appendages do not enlarge the water exclusion.
        /// Heights and inset are measured in the volume's local space.
        /// </summary>
        public static void BakeMainHull(OceanHullVolume volume, Mesh source, Matrix4x4 sourceToHull,
            float deckHeight, float insetMetres = .05f)
        {
            if (volume == null || source == null || !float.IsFinite(deckHeight)
                || !float.IsFinite(insetMetres) || insetMetres < 0)
                throw new ArgumentException("Choose a source hull, finite deck height and nonnegative inset.");
            var points = MainShellPoints(source, sourceToHull, deckHeight);
            var bounds = new Bounds(points[0], Vector3.zero);
            foreach (var point in points) bounds.Encapsulate(point);

            // A 64-vertex convex polyhedron has at most 124 triangular faces.
            // Add the most poorly represented source point each time, keeping
            // chords inside the detailed convex envelope instead of expanding
            // its support planes outward when simplifying to the GPU budget.
            var selected = new List<Vector3>();
            for (int axis = 0; axis < 3; axis++)
            {
                var minimum = points[0]; var maximum = points[0];
                foreach (var point in points)
                {
                    if (point[axis] < minimum[axis]) minimum = point;
                    if (point[axis] > maximum[axis]) maximum = point;
                }
                if (!selected.Contains(minimum)) selected.Add(minimum);
                if (!selected.Contains(maximum)) selected.Add(maximum);
            }
            while (selected.Count < 64)
            {
                var faces = BuildFaces(selected, bounds);
                float largestError = .001f;
                Vector3 next = default;
                foreach (var point in points)
                {
                    float distance = float.NegativeInfinity;
                    foreach (var face in faces) distance = Mathf.Max(distance, face.Distance(point));
                    if (distance > largestError) { largestError = distance; next = point; }
                }
                if (largestError <= .001f) break;
                selected.Add(next);
            }
            BakePoints(volume, selected, true, insetMetres);
        }

        public static List<Vector3> MainShellPoints(Mesh source, Matrix4x4 sourceToHull, float deckHeight)
        {
            using var snapshot = MeshUtility.AcquireReadOnlyMeshData(source);
            var data = snapshot[0];
            using var nativeVertices = new NativeArray<Vector3>(data.vertexCount, Allocator.Temp);
            data.GetVertices(nativeVertices);
            var vertices = new Vector3[nativeVertices.Length];
            var parent = new int[vertices.Length];
            var welds = new Dictionary<Vector3Int, int>();
            for (int i = 0; i < vertices.Length; i++)
            {
                vertices[i] = sourceToHull.MultiplyPoint3x4(nativeVertices[i]);
                var key = Vector3Int.RoundToInt(vertices[i] * 1000);
                if (!welds.TryGetValue(key, out int first)) { first = i; welds.Add(key, i); }
                parent[i] = first;
            }
            int Find(int i)
            {
                while (parent[i] != i) { parent[i] = parent[parent[i]]; i = parent[i]; }
                return i;
            }
            var triangles = new List<int>();
            for (int sub = 0; sub < data.subMeshCount; sub++)
            {
                var descriptor = data.GetSubMesh(sub);
                if (descriptor.topology != MeshTopology.Triangles) continue;
                using var indices = new NativeArray<int>(descriptor.indexCount, Allocator.Temp);
                data.GetIndices(indices, sub);
                for (int i = 0; i < indices.Length; i++) triangles.Add(indices[i]);
                for (int i = 0; i < indices.Length; i += 3)
                {
                    parent[Find(indices[i + 1])] = Find(indices[i]);
                    parent[Find(indices[i + 2])] = Find(indices[i]);
                }
            }
            // Surface area rather than vertex count prevents a highly subdivided
            // rudder or fitting from winning over the main body of the ship.
            var areas = new Dictionary<int, float>();
            int largest = -1; float largestArea = 0;
            for (int i = 0; i < triangles.Count; i += 3)
            {
                int component = Find(triangles[i]);
                float area = Vector3.Cross(vertices[triangles[i + 1]] - vertices[triangles[i]],
                    vertices[triangles[i + 2]] - vertices[triangles[i]]).magnitude * .5f;
                areas.TryGetValue(component, out float total);
                areas[component] = total += area;
                if (total > largestArea) { largestArea = total; largest = component; }
            }
            var points = new List<Vector3>();
            var unique = new HashSet<Vector3Int>();
            void Add(Vector3 point)
            {
                if (unique.Add(Vector3Int.RoundToInt(point * 10000))) points.Add(point);
            }
            for (int i = 0; i < triangles.Count; i += 3)
            {
                if (Find(triangles[i]) != largest) continue;
                for (int edge = 0; edge < 3; edge++)
                {
                    var a = vertices[triangles[i + edge]];
                    var b = vertices[triangles[i + (edge + 1) % 3]];
                    if (a.y <= deckHeight) Add(a);
                    if ((a.y < deckHeight) != (b.y < deckHeight))
                        Add(Vector3.LerpUnclamped(a, b, (deckHeight - a.y) / (b.y - a.y)));
                }
            }
            if (points.Count < 4) throw new InvalidOperationException("No main hull shell below the selected deck height.");
            return points;
        }
    }
}
