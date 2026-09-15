using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using UnityEditor;
using UnityEngine;
using Debug = UnityEngine.Debug;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    // Measures payloads region by region; deliberately never reconstructs the old
    // simultaneous whole-island forest arrays on a memory-constrained machine.
    public static class ForestStreamingBenchmark
    {
        [Serializable]
        private sealed class LodReport
        {
            public int lod, regions, nonEmptyRegions, meshUploads;
            public long wholeIslandManagedPayloadBytes, largestRegionPayloadBytes;
            public long triangles;
            public Vector2Int largestRegion;
            public double allRegionExportMilliseconds, largestRegionUploadMilliseconds, longestMeshUploadMilliseconds;
        }

        [Serializable]
        private sealed class Report
        {
            public string snapshot, unityVersion;
            public double snapshotLoadMilliseconds;
            public long avoidedPersistentDetailPayloadBytes;
            public LodReport[] lods;
        }

        // -executeMethod Motu.Editor.ForestStreamingBenchmark.RunBatch (without -quit)
        public static async void RunBatch()
        {
            try
            {
                var snapshot = Environment.GetEnvironmentVariable("MOTU_FOREST_SNAPSHOT");
                var output = Environment.GetEnvironmentVariable("MOTU_FOREST_REPORT");
                if (!File.Exists(snapshot)) throw new FileNotFoundException("Choose an existing snapshot.", snapshot);
                var timer = Stopwatch.StartNew();
                using var owner = await Task.Run(() =>
                {
                    var handle = MotuNative.LoadMotuSnapshot(snapshot, out var status);
                    if (handle == IntPtr.Zero) throw new InvalidOperationException($"Snapshot load failed: {status}.");
                    return new NativeIslandHandle(handle);
                });
                var report = new Report { snapshot = Path.GetFileName(snapshot), unityVersion = Application.unityVersion,
                    snapshotLoadMilliseconds = timer.Elapsed.TotalMilliseconds, lods = new LodReport[2] };
                for (var lod = 0; lod <= 1; lod++)
                {
                    var level = lod;
                    var measured = await Task.Run(() => Measure(owner.Value, level));
                    report.lods[lod] = measured;
                    report.avoidedPersistentDetailPayloadBytes += measured.wholeIslandManagedPayloadBytes;
                    var region = await ForestGridWorker.PrepareAsync(owner, 2000f, lod, measured.largestRegion, CancellationToken.None);
                    foreach (var meshes in new[] { region.foliage, region.wood })
                    {
                        for (var index = 0; index < meshes.Length; index++)
                        {
                            if (meshes[index] == null) continue;
                            timer.Restart();
                            var mesh = IslandMeshInterop.CreateGeneratedMesh(meshes[index]);
                            var elapsed = timer.Elapsed.TotalMilliseconds;
                            measured.meshUploads++;
                            measured.largestRegionUploadMilliseconds += elapsed;
                            measured.longestMeshUploadMilliseconds = Math.Max(measured.longestMeshUploadMilliseconds, elapsed);
                            Object.DestroyImmediate(mesh);
                            meshes[index] = null;
                            await Task.Yield();
                        }
                    }
                    Debug.Log($"MOTU FOREST LOD {lod}: {JsonUtility.ToJson(measured)}");
                }
                File.WriteAllText(output, JsonUtility.ToJson(report, true));
                Debug.Log($"MOTU FOREST BENCHMARK saved to {output}");
                EditorApplication.Exit(0);
            }
            catch (Exception error) { Debug.LogException(error); EditorApplication.Exit(1); }
        }

        private static LodReport Measure(IntPtr handle, int lod)
        {
            var report = new LodReport { lod = lod };
            var resolution = lod == 0 ? ForestTileStreamer.Lod1Resolution : ForestTileStreamer.Lod2Resolution;
            var timer = Stopwatch.StartNew();
            for (var y = 0; y < resolution; y++)
                for (var x = 0; x < resolution; x++)
                {
                    var key = new Vector2Int(x, y);
                    var region = ForestGridWorker.Prepare(handle, 2000f, lod, key, CancellationToken.None);
                    long bytes = 0;
                    foreach (var meshes in new[] { region.foliage, region.wood })
                        foreach (var mesh in meshes)
                        {
                            if (mesh == null) continue;
                            bytes += MeshBytes(mesh);
                            report.triangles += mesh.triangles.Length / 3;
                        }
                    report.regions++;
                    if (bytes > 0) report.nonEmptyRegions++;
                    report.wholeIslandManagedPayloadBytes += bytes;
                    if (bytes > report.largestRegionPayloadBytes)
                    {
                        report.largestRegionPayloadBytes = bytes;
                        report.largestRegion = key;
                    }
                }
            report.allRegionExportMilliseconds = timer.Elapsed.TotalMilliseconds;
            return report;
        }

        private static long MeshBytes(IslandPreparedMesh mesh) =>
            mesh.vertices.LongLength * 12 + mesh.normals.LongLength * 12 + mesh.triangles.LongLength * 4
            + mesh.uv.LongLength * 8 + mesh.material.LongLength * 16 + mesh.environment.LongLength * 8
            + mesh.caveAttributes.LongLength * 8;
    }
}
