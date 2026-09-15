using System;
using System.Collections.Generic;
using System.Threading;
using Motu.Islands;
using Motu.Interop;
using UnityEngine;

namespace Motu.Navigation
{
    // Index original LOD0 faces; never simplify or morph their positions. Only the
    // current chunk is uploaded to Unity. This index can be prepared on a worker.
    internal sealed class IslandNavigationChunks
    {
        internal sealed class MeshPart
        {
            internal IslandPreparedMesh mesh;
            internal bool excludeRiverBeds;
            internal readonly List<int> triangles = new List<int>();
        }
        internal sealed class Chunk
        {
            internal Vector2Int key;
            internal Bounds bounds;
            internal readonly List<MeshPart> parts = new List<MeshPart>();
            internal bool hasGround;
        }
        internal readonly List<Chunk> chunks = new List<Chunk>();
        internal int sourceTriangles;
        internal float size, padding;

        internal static IslandNavigationChunks Prepare(IslandPreparedMesh terrain, IslandPreparedCaves caves,
            float size, float padding, float minimumHeight, bool excludeRiverBeds, CancellationToken cancellation)
        {
            var result = new IslandNavigationChunks { size = size, padding = padding };
            var cells = new Dictionary<Vector2Int, Chunk>();
            var minimumY = float.PositiveInfinity;
            var maximumY = float.NegativeInfinity;
            void Add(IslandPreparedMesh mesh, bool exclude)
            {
                if (mesh == null) return;
                result.sourceTriangles += mesh.triangles.Length / 3;
                for (var index = 0; index < mesh.vertices.Length; index++)
                {
                    if (index % 1024 == 0) cancellation.ThrowIfCancellationRequested();
                    minimumY = Mathf.Min(minimumY, mesh.vertices[index].y);
                    maximumY = Mathf.Max(maximumY, mesh.vertices[index].y);
                }
                var parts = new Dictionary<Vector2Int, MeshPart>();
                for (var face = 0; face < mesh.triangles.Length; face += 3)
                {
                    if (face % 3072 == 0) cancellation.ThrowIfCancellationRequested();
                    var a = mesh.vertices[mesh.triangles[face]];
                    var b = mesh.vertices[mesh.triangles[face + 1]];
                    var c = mesh.vertices[mesh.triangles[face + 2]];
                    var min = Vector3.Min(a, Vector3.Min(b, c));
                    var max = Vector3.Max(a, Vector3.Max(b, c));
                    // Retain submerged neighbours as rasterization context; only
                    // skip baking cells with no potentially walkable ground.
                    var x0 = Mathf.FloorToInt((min.x - padding) / size);
                    var x1 = Mathf.FloorToInt((max.x + padding) / size);
                    var z0 = Mathf.FloorToInt((min.z - padding) / size);
                    var z1 = Mathf.FloorToInt((max.z + padding) / size);
                    for (var z = z0; z <= z1; z++)
                        for (var x = x0; x <= x1; x++)
                        {
                            var key = new Vector2Int(x, z);
                            if (!cells.TryGetValue(key, out var chunk))
                            {
                                chunk = new Chunk { key = key, bounds = new Bounds(
                                    new Vector3((x + .5f) * size, (min.y + max.y) * .5f, (z + .5f) * size),
                                    new Vector3(size, max.y - min.y + 4f, size)) };
                                cells.Add(key, chunk);
                            }
                            var lower = chunk.bounds.min; var upper = chunk.bounds.max;
                            lower.y = Mathf.Min(lower.y, min.y - 2f); upper.y = Mathf.Max(upper.y, max.y + 2f);
                            chunk.bounds.SetMinMax(lower, upper);
                            chunk.hasGround |= max.y >= minimumHeight && max.x >= x * size && min.x < (x + 1) * size
                                && max.z >= z * size && min.z < (z + 1) * size;
                            if (!parts.TryGetValue(key, out var part))
                            {
                                part = new MeshPart { mesh = mesh, excludeRiverBeds = exclude };
                                parts.Add(key, part); chunk.parts.Add(part);
                            }
                            part.triangles.Add(face);
                        }
                }
            }
            Add(terrain, excludeRiverBeds);
            if (caves != null)
                foreach (var cave in caves.caves)
                    foreach (var mesh in cave.chunks) Add(mesh, false);
            foreach (var chunk in cells.Values)
            {
                if (!chunk.hasGround) continue;
                // Use the original whole-source vertical range for every bake.
                // Independent Y origins quantize slopes/steps differently at joins.
                var lower = chunk.bounds.min; var upper = chunk.bounds.max;
                lower.y = minimumY - 2f; upper.y = maximumY + 2f;
                chunk.bounds.SetMinMax(lower, upper);
                result.chunks.Add(chunk);
            }
            result.chunks.Sort((a, b) => a.key.y == b.key.y ? a.key.x.CompareTo(b.key.x) : a.key.y.CompareTo(b.key.y));
            return result;
        }
    }
}
