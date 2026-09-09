using System;
using System.Collections.Generic;
using System.Threading;
using UnityEngine;
using Unity.Profiling;
using Motu.Islands;
using Stopwatch = System.Diagnostics.Stopwatch;

namespace Motu.Interop
{
    internal sealed class IslandPreparedCave
    {
        internal readonly ulong id;
        internal readonly Vector3 entrance, inward, chamber;
        internal readonly Rect footprint;
        internal readonly IslandPreparedMesh[] chunks;
        internal readonly Vector3[][] branchPaths;
        internal IslandPreparedCave(CaveNative.Info info, IslandPreparedMesh[] chunks, float size, Vector3[][] branchPaths = null)
        {
            id = info.id;
            entrance = Position(info.entrance, size);
            chamber = Position(info.chamber, size);
            inward = new Vector3(info.inward.x, 0, info.inward.y);
            footprint = Rect.MinMaxRect(info.minimum.x - size * .5f, info.minimum.y - size * .5f,
                info.maximum.x - size * .5f, info.maximum.y - size * .5f);
            this.chunks = chunks;
            this.branchPaths = branchPaths ?? Array.Empty<Vector3[]>();
        }
        internal static Vector3 Position(Vector3 p, float size) => new Vector3(p.x - size * .5f, p.z, p.y - size * .5f);
    }

    internal sealed class IslandPreparedCaves
    {
        internal static readonly IslandPreparedCaves Empty = new IslandPreparedCaves(Array.Empty<IslandPreparedCave>(), default);
        internal readonly IslandPreparedCave[] caves;
        internal readonly CaveNative.Stats stats;
        internal readonly int branchCount;
        internal readonly double exportMilliseconds, copyMilliseconds;
        // Array payload only; excludes object headers and native export scratch.
        internal readonly long bufferBytes;
        internal IslandPreparedCaves(IslandPreparedCave[] caves, CaveNative.Stats stats,
            double exportMilliseconds = 0, double copyMilliseconds = 0, long bufferBytes = 0)
        {
            this.caves = caves; this.stats = stats;
            foreach (var cave in caves) branchCount += cave.branchPaths.Length;
            this.exportMilliseconds = exportMilliseconds; this.copyMilliseconds = copyMilliseconds;
            this.bufferBytes = bufferBytes;
        }
    }

    internal static class CavePreparation
    {
        private static readonly ProfilerMarker ExportMarker = new ProfilerMarker("Motu.Caves.NativeExport");
        private static readonly ProfilerMarker CopyMarker = new ProfilerMarker("Motu.Caves.CopyMesh");
        internal static IslandPreparedCaves Prepare(IntPtr handle, float size, CancellationToken cancellation)
        {
            var count = CaveNative.GetCaveCount(handle);
            if (count > 4) throw new InvalidOperationException("Native cave count exceeds its limit.");
            if (CaveNative.GetCaveStats(handle, out var stats) == 0)
                throw new InvalidOperationException("Native cave diagnostics are unavailable.");
            var caves = new IslandPreparedCave[count];
            long triangles = 0;
            long bufferBytes = 0;
            double exportMilliseconds = 0, copyMilliseconds = 0;
            var timer = new Stopwatch();
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
                var chunks = new List<IslandPreparedMesh>((int)info.chunkCount + 1);
                for (uint j = 0; j <= info.chunkCount; j++)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var mesh = default(MotuNative.ExportMesh);
                    try
                    {
                        timer.Restart();
                        byte exported;
                        var stones = j == info.chunkCount;
                        using (ExportMarker.Auto()) exported = stones
                            ? CaveNative.CreateCaveFloorStoneMesh(handle, i, out mesh)
                            : CaveNative.CreateCaveMesh(handle, i, j, out mesh);
                        exportMilliseconds += timer.Elapsed.TotalMilliseconds;
                        if (stones && exported != 0 && mesh.vertices.length == 0) continue;
                        if (exported == 0
                            || mesh.vertices.length <= 0 || mesh.vertices.length > 250000
                            || mesh.normals.length != mesh.vertices.length
                            || mesh.triangles.length < 3 || mesh.triangles.length % 3 != 0)
                            throw new InvalidOperationException("Invalid native cave mesh.");
                        if (stones && mesh.triangles.length / 3 > 20480)
                            throw new InvalidOperationException("Cave floor stone limit exceeded.");
                        if (!stones) triangles += mesh.triangles.length / 3;
                        if (triangles > 250000) throw new InvalidOperationException("Native cave triangle budget exceeded.");
                        timer.Restart();
                        using var copySample = CopyMarker.Auto();
                        var copy = IslandMeshInterop.CopyGeneratedMeshData(mesh, size);
                        foreach (var index in copy.triangles)
                            if (index < 0 || index >= copy.vertices.Length)
                                throw new InvalidOperationException("Invalid native cave triangle index.");
                        if (copy.uv.Length != copy.vertices.Length)
                            throw new InvalidOperationException("Missing cave surface attributes.");
                        var terrainUv = new Vector2[copy.vertices.Length];
                        for (var vertex = 0; vertex < terrainUv.Length; vertex++)
                            terrainUv[vertex] = new Vector2(copy.vertices[vertex].x / size + .5f, copy.vertices[vertex].z / size + .5f);
                        chunks.Add(new IslandPreparedMesh(copy.vertices, copy.normals, copy.triangles,
                            terrainUv, copy.material, copy.environment, copy.uv));
                        bufferBytes += (long)(copy.vertices.Length + copy.normals.Length) * 12
                            + (long)copy.triangles.Length * 4 + (long)copy.material.Length * 16
                            + ((long)terrainUv.Length + copy.environment.Length + copy.uv.Length) * 8;
                        copyMilliseconds += timer.Elapsed.TotalMilliseconds;
                    }
                    finally { MotuNative.ReleaseMesh(ref mesh); }
                }
                var branchCount = CaveNative.GetCaveBranchCount(handle, i);
                if (branchCount > 4096) throw new InvalidOperationException("Native branch limit exceeded.");
                var paths = new Vector3[branchCount][];
                for (uint branch = 0; branch < branchCount; branch++)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var nodeCount = CaveNative.GetCaveBranchNodeCount(handle, i, branch);
                    if (nodeCount < 2 || nodeCount > 4096) throw new InvalidOperationException("Invalid cave branch path.");
                    paths[branch] = new Vector3[nodeCount];
                    for (uint node = 0; node < nodeCount; node++)
                    {
                        if (CaveNative.GetCaveBranchNode(handle, i, branch, node, out var position) == 0
                            || !IslandMeshInterop.IsFinite(position)) throw new InvalidOperationException("Invalid cave branch node.");
                        paths[branch][node] = IslandPreparedCave.Position(position, size);
                    }
                    bufferBytes += nodeCount * 12L;
                }
                caves[i] = new IslandPreparedCave(info, chunks.ToArray(), size, paths);
            }
            return new IslandPreparedCaves(caves, stats, exportMilliseconds, copyMilliseconds, bufferBytes);
        }
    }
}
