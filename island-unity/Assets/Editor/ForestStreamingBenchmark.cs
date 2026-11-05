using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using UnityEditor;
using UnityEngine;
using Debug = UnityEngine.Debug;

namespace Motu.Editor
{
    // Measures one shared tree catalog and compact placements. Standing-tree
    // detail geometry is never expanded for individual islands or regions.
    public static class ForestStreamingBenchmark
    {
        [Serializable]
        private sealed class Report
        {
            public string snapshot, unityVersion;
            public int instances, prototypes, variantsPerSpecies, sharedGpuVertices;
            public long compactInstanceBytes, sharedPrototypePayloadBytes;
            public double snapshotLoadMilliseconds, catalogExportMilliseconds, sharedGpuUploadMilliseconds;
        }

        // -executeMethod Motu.Editor.ForestStreamingBenchmark.RunBatch (without -quit)
        public static async void RunBatch()
        {
            try
            {
                var snapshot = Environment.GetEnvironmentVariable("MOTU_FOREST_SNAPSHOT");
                var output = Environment.GetEnvironmentVariable("MOTU_FOREST_REPORT");
                if (!File.Exists(snapshot)) throw new FileNotFoundException("Choose an existing shared-forest snapshot.", snapshot);
                if (string.IsNullOrWhiteSpace(output)) throw new ArgumentException("Set MOTU_FOREST_REPORT to the report destination.");
                var timer = Stopwatch.StartNew();
                using var owner = await Task.Run(() =>
                {
                    var handle = MotuNative.LoadMotuSnapshot(snapshot, out var status);
                    if (handle == IntPtr.Zero) throw new InvalidOperationException($"Snapshot load failed: {status}.");
                    return new NativeIslandHandle(handle);
                });
                var report = new Report { snapshot = Path.GetFileName(snapshot), unityVersion = Application.unityVersion,
                    snapshotLoadMilliseconds = timer.Elapsed.TotalMilliseconds };
                timer.Restart();
                var catalog = await Task.Run(() => HybridForestNative.Prepare(owner.Value, 2000f));
                report.catalogExportMilliseconds = timer.Elapsed.TotalMilliseconds;
                report.instances = catalog.instances.Length;
                report.prototypes = catalog.prototypes.Length;
                report.variantsPerSpecies = catalog.broadleafCount;
                report.compactInstanceBytes = (long)catalog.instances.Length * Marshal.SizeOf<HybridForestNative.Instance>();
                foreach (var prototype in catalog.prototypes)
                    foreach (var mesh in prototype)
                        if (mesh != null) report.sharedPrototypePayloadBytes += MeshBytes(mesh);
                PlanetForestModels models = null;
                try
                {
                    timer.Restart();
                    models = PlanetForestModels.Acquire(catalog, null);
                    report.sharedGpuUploadMilliseconds = timer.Elapsed.TotalMilliseconds;
                    report.sharedGpuVertices = models.vertexCount;
                }
                finally { models?.Release(); }
                File.WriteAllText(output, JsonUtility.ToJson(report, true));
                Debug.Log($"MOTU SHARED FOREST BENCHMARK saved to {output}: {JsonUtility.ToJson(report)}");
                EditorApplication.Exit(0);
            }
            catch (Exception error) { Debug.LogException(error); EditorApplication.Exit(1); }
        }

        private static long MeshBytes(IslandPreparedMesh mesh) =>
            mesh.vertices.LongLength * 12 + mesh.normals.LongLength * 12 + mesh.triangles.LongLength * 4
            + mesh.uv.LongLength * 8 + mesh.material.LongLength * 16 + mesh.environment.LongLength * 8
            + mesh.caveAttributes.LongLength * 8;
    }
}
