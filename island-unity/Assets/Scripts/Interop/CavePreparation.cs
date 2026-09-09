using System;
using System.Threading;
using UnityEngine;
using Motu.Islands;

namespace Motu.Interop
{
    internal sealed class IslandPreparedCave
    {
        internal readonly ulong id;
        internal readonly Vector3 entrance, inward, chamber;
        internal readonly Rect footprint;
        internal readonly IslandPreparedMesh[] chunks;
        internal IslandPreparedCave(CaveNative.Info info, IslandPreparedMesh[] chunks, float size)
        {
            id = info.id;
            entrance = Position(info.entrance, size);
            chamber = Position(info.chamber, size);
            inward = new Vector3(info.inward.x, 0, info.inward.y);
            footprint = Rect.MinMaxRect(info.minimum.x - size * .5f, info.minimum.y - size * .5f,
                info.maximum.x - size * .5f, info.maximum.y - size * .5f);
            this.chunks = chunks;
        }
        private static Vector3 Position(Vector3 p, float size) => new Vector3(p.x - size * .5f, p.z, p.y - size * .5f);
    }

    internal sealed class IslandPreparedCaves
    {
        internal static readonly IslandPreparedCaves Empty = new IslandPreparedCaves(Array.Empty<IslandPreparedCave>(), default);
        internal readonly IslandPreparedCave[] caves;
        internal readonly CaveNative.Stats stats;
        internal IslandPreparedCaves(IslandPreparedCave[] caves, CaveNative.Stats stats) { this.caves = caves; this.stats = stats; }
    }

    internal static class CavePreparation
    {
        internal static IslandPreparedCaves Prepare(IntPtr handle, float size, CancellationToken cancellation)
        {
            var count = CaveNative.GetCaveCount(handle);
            if (count > 4) throw new InvalidOperationException("Native cave count exceeds its limit.");
            if (CaveNative.GetCaveStats(handle, out var stats) == 0)
                throw new InvalidOperationException("Native cave diagnostics are unavailable.");
            var caves = new IslandPreparedCave[count];
            long triangles = 0;
            var totalChunks = 0u;
            for (uint i = 0; i < count; i++)
            {
                cancellation.ThrowIfCancellationRequested();
                if (CaveNative.GetCaveInfo(handle, i, out var info) == 0 || info.chunkCount > 128
                    || !IslandMeshInterop.IsFinite(info.entrance) || !IslandMeshInterop.IsFinite(info.chamber)
                    || !float.IsFinite(info.minimum.x) || !float.IsFinite(info.minimum.y)
                    || !float.IsFinite(info.maximum.x) || !float.IsFinite(info.maximum.y)
                    || info.maximum.x <= info.minimum.x || info.maximum.y <= info.minimum.y)
                    throw new InvalidOperationException("Invalid native cave descriptor.");
                totalChunks += info.chunkCount;
                if (totalChunks > 128) throw new InvalidOperationException("Native cave chunk budget exceeded.");
                var chunks = new IslandPreparedMesh[info.chunkCount];
                for (uint j = 0; j < chunks.Length; j++)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var mesh = default(MotuNative.ExportMesh);
                    try
                    {
                        if (CaveNative.CreateCaveMesh(handle, i, j, out mesh) == 0
                            || mesh.vertices.length <= 0 || mesh.vertices.length > 250000
                            || mesh.normals.length != mesh.vertices.length
                            || mesh.triangles.length < 3 || mesh.triangles.length % 3 != 0)
                            throw new InvalidOperationException("Invalid native cave mesh.");
                        triangles += mesh.triangles.length / 3;
                        if (triangles > 250000) throw new InvalidOperationException("Native cave triangle budget exceeded.");
                        var copy = IslandMeshInterop.CopyGeneratedMeshData(mesh, size);
                        foreach (var index in copy.triangles)
                            if (index < 0 || index >= copy.vertices.Length)
                                throw new InvalidOperationException("Invalid native cave triangle index.");
                        if (copy.uv.Length != copy.vertices.Length)
                            throw new InvalidOperationException("Missing cave surface attributes.");
                        var terrainUv = new Vector2[copy.vertices.Length];
                        for (var vertex = 0; vertex < terrainUv.Length; vertex++)
                            terrainUv[vertex] = new Vector2(copy.vertices[vertex].x / size + .5f, copy.vertices[vertex].z / size + .5f);
                        chunks[j] = new IslandPreparedMesh(copy.vertices, copy.normals, copy.triangles,
                            terrainUv, copy.material, copy.environment, copy.uv);
                    }
                    finally { MotuNative.ReleaseMesh(ref mesh); }
                }
                caves[i] = new IslandPreparedCave(info, chunks, size);
            }
            return new IslandPreparedCaves(caves, stats);
        }
    }
}
