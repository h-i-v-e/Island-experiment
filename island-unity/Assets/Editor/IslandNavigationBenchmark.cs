using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using Motu.Navigation;
using Motu.Settings;
using UnityEditor;
using UnityEngine;
using UnityEngine.AI;
using Debug = UnityEngine.Debug;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public static class IslandNavigationBenchmark
    {
        [Serializable]
        private sealed class Report
        {
            public string snapshot, unityVersion;
            public double snapshotAndExportMilliseconds;
            public IslandNavigationBuildReport build;
            public int heightSamples, heightMisses, heightSamplesOutsideSourceFace;
            public float meanHeightErrorMetres, maximumHeightErrorMetres;
            public int pathsComplete, pathsPartial, pathsInvalid;
            public double meanPathMilliseconds, maximumPathMilliseconds;
        }
        private sealed class Prepared
        {
            internal IslandPreparedMesh ground;
            internal IslandPreparedTreeCollider[][] trunks, logs;
            internal IslandPreparedBoulderCollider[][] boulders;
            internal IslandPreparedCaves caves;
        }
        private static bool running;

        [MenuItem("Motu/Navigation/Benchmark Cached Island")]
        private static async void RunMenu()
        {
            if (running) return;
            var path = EditorUtility.OpenFilePanel("Choose a generated island snapshot", IslandSnapshotCache.CacheDirectory, "motusnapshot");
            if (string.IsNullOrEmpty(path)) return;
            var output = EditorUtility.SaveFilePanel("Save navigation benchmark", "", "island-navigation.json", "json");
            if (string.IsNullOrEmpty(output)) return;
            var factory = Object.FindFirstObjectByType<IslandGenerationRequestFactoryBase>();
            var coherent = Object.FindFirstObjectByType<CoherentIslandFactory>();
            var settings = coherent != null ? coherent.NavigationSettings.Copy()
                : factory != null ? factory.NavigationSettings.Copy() : new IslandNavigationSettings();
            try { await RunAsync(path, output, settings); }
            catch (Exception error) { Debug.LogException(error); }
        }

        // -executeMethod Motu.Editor.IslandNavigationBenchmark.RunBatch (without -quit)
        public static async void RunBatch()
        {
            try
            {
                await RunAsync(Environment.GetEnvironmentVariable("MOTU_NAV_SNAPSHOT"),
                    Environment.GetEnvironmentVariable("MOTU_NAV_REPORT"), new IslandNavigationSettings());
                EditorApplication.Exit(0);
            }
            catch (Exception error) { Debug.LogException(error); EditorApplication.Exit(1); }
        }

        private static async Task RunAsync(string snapshot, string output, IslandNavigationSettings settings)
        {
            if (running) throw new InvalidOperationException("A navigation benchmark is already running.");
            if (!File.Exists(snapshot)) throw new FileNotFoundException("Choose an existing island snapshot.", snapshot);
            running = true;
            GameObject host = null;
            try
            {
                var report = new Report { snapshot = Path.GetFileName(snapshot), unityVersion = Application.unityVersion };
                var timer = Stopwatch.StartNew();
                Debug.Log("Navigation benchmark: loading and exporting full LOD0 terrain.");
                var prepared = await Task.Run(() => Prepare(snapshot));
                report.snapshotAndExportMilliseconds = timer.Elapsed.TotalMilliseconds;
                if (prepared.ground == null) throw new InvalidOperationException("The snapshot has no renderable terrain.");
                Debug.Log($"Navigation benchmark: {prepared.ground.vertices.Length} vertices, {prepared.ground.triangles.Length / 3} terrain triangles.");
                host = new GameObject("Navigation Benchmark");
                var navigation = host.AddComponent<IslandNavigation>();
                await navigation.BuildAsync(prepared.ground, null, prepared.boulders, prepared.caves,
                    settings, CancellationToken.None, prepared.trunks, prepared.logs);
                report.build = navigation.Report;
                MeasureQueries(navigation.AgentTypeId, prepared.ground, settings, report);
                File.WriteAllText(output, JsonUtility.ToJson(report, true));
                Debug.Log($"Navigation benchmark saved to {output}: {JsonUtility.ToJson(report)}");
            }
            finally
            {
                if (host != null) Object.DestroyImmediate(host);
                running = false;
            }
        }

        private static Prepared Prepare(string snapshot)
        {
            // Read directly: a benchmark must not trim, rewrite, or touch cache timestamps.
            var handle = MotuNative.LoadMotuSnapshot(snapshot, out var status);
            if (handle == IntPtr.Zero) throw new InvalidOperationException($"Snapshot load failed: status {status}.");
            try
            {
                return new Prepared
                {
                    ground = IslandPreparationPipeline.PrepareNavigationMesh(handle, 2000f),
                    trunks = VegetationPreparation.PrepareForestTrunkColliders(handle, 2000f),
                    logs = VegetationPreparation.PrepareForestTrunkColliders(handle, 2000f, true),
                    boulders = TerrainPreparation.PrepareBoulderColliders(handle, 2000f),
                    caves = CavePreparation.Prepare(handle, 2000f, CancellationToken.None),
                };
            }
            finally { MotuNative.ReleaseMotu(handle); }
        }

        private static void MeasureQueries(int agentType, IslandPreparedMesh mesh, IslandNavigationSettings settings, Report report)
        {
            var filter = new NavMeshQueryFilter { agentTypeID = agentType, areaMask = NavMesh.AllAreas };
            var points = new List<Vector3>();
            var faces = mesh.triangles.Length / 3;
            var stride = Math.Max(1, faces / 2048);
            var minimumNormal = Mathf.Cos(settings.MaximumSlope * Mathf.Deg2Rad);
            for (var face = 0; face < faces; face += stride)
            {
                var a = mesh.vertices[mesh.triangles[face * 3]];
                var b = mesh.vertices[mesh.triangles[face * 3 + 1]];
                var c = mesh.vertices[mesh.triangles[face * 3 + 2]];
                var centre = (a + b + c) / 3f;
                if (centre.y < settings.MinimumGroundHeight || Vector3.Cross(b - a, c - a).normalized.y < minimumNormal) continue;
                if (!NavMesh.SamplePosition(centre, out var hit, .75f, filter)) { report.heightMisses++; continue; }
                points.Add(hit.position);
                // Compare against the original LOD0 triangle at the returned XZ,
                // so a small horizontal displacement on a slope is not counted as height error.
                var ab = new Vector2(b.x - a.x, b.z - a.z);
                var ac = new Vector2(c.x - a.x, c.z - a.z);
                var ap = new Vector2(hit.position.x - a.x, hit.position.z - a.z);
                var determinant = ab.x * ac.y - ab.y * ac.x;
                if (Mathf.Abs(determinant) < 1e-10f) continue;
                var u = (ap.x * ac.y - ap.y * ac.x) / determinant;
                var v = (ab.x * ap.y - ab.y * ap.x) / determinant;
                if (u < 0 || v < 0 || u + v > 1) { report.heightSamplesOutsideSourceFace++; continue; }
                var error = Mathf.Abs(hit.position.y - (a.y + (b.y - a.y) * u + (c.y - a.y) * v));
                report.heightSamples++; report.meanHeightErrorMetres += error;
                report.maximumHeightErrorMetres = Mathf.Max(report.maximumHeightErrorMetres, error);
            }
            if (report.heightSamples > 0) report.meanHeightErrorMetres /= report.heightSamples;
            var path = new NavMeshPath();
            var timer = new Stopwatch();
            var count = Math.Min(128, points.Count / 2);
            for (var i = 0; i < count; i++)
            {
                timer.Restart();
                NavMesh.CalculatePath(points[i], points[points.Count - 1 - i], filter, path);
                timer.Stop();
                report.meanPathMilliseconds += timer.Elapsed.TotalMilliseconds;
                report.maximumPathMilliseconds = Math.Max(report.maximumPathMilliseconds, timer.Elapsed.TotalMilliseconds);
                if (path.status == NavMeshPathStatus.PathComplete) report.pathsComplete++;
                else if (path.status == NavMeshPathStatus.PathPartial) report.pathsPartial++;
                else report.pathsInvalid++;
            }
            if (count > 0) report.meanPathMilliseconds /= count;
        }
    }

    [CustomEditor(typeof(IslandNavigation))]
    public sealed class IslandNavigationEditor : UnityEditor.Editor
    {
        public override void OnInspectorGUI()
        {
            var navigation = (IslandNavigation)target;
            EditorGUILayout.LabelField("Status", navigation.IsReady ? "Ready" : "Pending / unavailable");
            EditorGUILayout.Toggle("Registered", navigation.IsRegistered);
            EditorGUILayout.IntField("Agent type", navigation.AgentTypeId);
            var report = navigation.Report;
            if (report == null) return;
            using (new EditorGUI.DisabledScope(true))
            {
                EditorGUILayout.FloatField("Agent radius (m)", report.radius);
                EditorGUILayout.FloatField("Agent height (m)", report.height);
                EditorGUILayout.IntField("Source triangles", report.sourceTriangles);
                EditorGUILayout.IntField("Sources (all chunk uploads)", report.sourceCount);
                EditorGUILayout.IntField("Chunks completed", report.completedChunks);
                EditorGUILayout.IntField("Chunks total", report.chunkCount);
                EditorGUILayout.FloatField("Chunk width (m)", report.chunkSize);
                EditorGUILayout.IntField("Peak chunk triangles", report.peakChunkTriangles);
                EditorGUILayout.DoubleField("Peak chunk source mesh MiB", report.peakChunkSourceMeshBytes / 1048576.0);
                EditorGUILayout.DoubleField("Bake seconds", report.bakeMilliseconds / 1000);
                EditorGUILayout.DoubleField("Sampled process rise (MiB)",
                    (report.sampledProcessResidentPeakBytes - report.processResidentBaselineBytes) / 1048576.0);
            }
            if (!string.IsNullOrEmpty(report.failure)) EditorGUILayout.HelpBox(report.failure, MessageType.Error);
            EditorGUILayout.HelpBox("Agent dimensions are configured on the island request factory. Reload the island to apply changes.", MessageType.Info);
        }
    }

}
