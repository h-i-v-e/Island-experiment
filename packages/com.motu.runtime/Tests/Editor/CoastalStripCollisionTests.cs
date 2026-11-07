using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalStripCollisionTests
    {
        private const float WorldSize = 2000f;
        private IslandPreparedColliderHeightMap heightMap;

        [OneTimeSetUp]
        public void PrepareAsymmetricHeightField()
        {
            var dimension = TerrainTileStreamer.Lod1Resolution * (TerrainTileStreamer.ColliderSamplesPerTile - 1) + 1;
            var heights = IslandPreparedColliderHeightMap.AllocateSamples(checked(dimension * dimension));
            for (var z = 0; z < dimension; z++)
            {
                var localZ = -WorldSize * .5f + z * WorldSize / (dimension - 1);
                for (var x = 0; x < dimension; x++)
                {
                    var localX = -WorldSize * .5f + x * WorldSize / (dimension - 1);
                    var index = z * dimension + x;
                    heights[index / IslandPreparedColliderHeightMap.SamplesPerBlock]
                        [index % IslandPreparedColliderHeightMap.SamplesPerBlock] = PlaneHeight(localX, localZ) / WorldSize;
                }
            }
            heightMap = new IslandPreparedColliderHeightMap(dimension,
                TerrainTileStreamer.ColliderSamplesPerTile, heights, WorldSize);
        }

        [OneTimeTearDown]
        public void ReleaseHeightField() => heightMap = null;

        [TestCase(0f, 7, 51)]
        [TestCase(0f, 49, 13)]
        [TestCase(180f, 7, 51)]
        [TestCase(180f, 49, 13)]
        public void StreamedTerrainColliderMatchesTheRenderedPlaneUnderItsWorldTransform(float yaw, int tileX, int tileZ)
        {
            var root = new GameObject("Coastal collider transform regression");
            TerrainTileStreamer.ColliderTile tile = null;
            Mesh mesh = null;
            try
            {
                root.transform.SetPositionAndRotation(new Vector3(317f, 23f, -811f), Quaternion.Euler(0f, yaw, 0f));
                var streamer = root.AddComponent<TerrainTileStreamer>();
                streamer.worldSize = WorldSize;
                streamer.colliderHeightMap = heightMap;
                streamer.colliderRoot = new GameObject("Streamed terrain collision");
                streamer.colliderRoot.transform.SetParent(root.transform, false);
                var key = new Vector2Int(tileX, tileZ);
                tile = streamer.CreateColliderTile(key);
                var meshObject = new GameObject("Rendered terrain plane reference");
                meshObject.transform.SetParent(root.transform, false);
                mesh = CreatePlane(key);
                var meshCollider = meshObject.AddComponent<MeshCollider>();
                meshCollider.sharedMesh = mesh;
                Physics.SyncTransforms();

                var failures = new List<string>();
                var tileSize = WorldSize / TerrainTileStreamer.Lod1Resolution;
                foreach (var z in new[] { .13f, .41f, .77f })
                foreach (var x in new[] { .17f, .48f, .83f })
                {
                    var localX = -WorldSize * .5f + (tileX + x) * tileSize;
                    var localZ = -WorldSize * .5f + (tileZ + z) * tileSize;
                    var expected = root.transform.TransformPoint(new Vector3(localX, PlaneHeight(localX, localZ), localZ));
                    var ray = new Ray(expected + Vector3.up * 500f, Vector3.down);
                    Assert.That(meshCollider.Raycast(ray, out var rendered, 1000f), Is.True,
                        $"Reference mesh missed yaw={yaw}, tile={key}, local=({localX:R},{localZ:R}).");
                    Assert.That(rendered.point.y, Is.EqualTo(expected.y).Within(.001f));
                    if (!tile.collider.Raycast(ray, out var physical, 1000f))
                        failures.Add($"MISS at local=({localX:R},{localZ:R}) world={expected:R}");
                    else if (Mathf.Abs(physical.point.y - rendered.point.y) > .015f)
                        failures.Add($"HEIGHT at local=({localX:R},{localZ:R}): mesh={rendered.point.y:R}, terrain={physical.point.y:R}, delta={physical.point.y - rendered.point.y:R}");
                }
                var diagnostic = $"yaw={yaw}, tile={key}, root={root.transform.position:R}, collider={tile.gameObject.transform.position:R}, collider rotation={tile.gameObject.transform.rotation.eulerAngles:R}, bounds={tile.collider.bounds}; {string.Join("; ", failures)}";
                Debug.Log(diagnostic);
                Assert.That(failures, Is.Empty, diagnostic);
            }
            finally
            {
                TerrainTileStreamer.DestroyColliderTile(tile);
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(mesh);
            }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void GeneratedCoastCollisionMatchesRenderedInteriorNodesOnBothCoasts()
        {
            var handle = IntPtr.Zero;
            Mesh mesh = null;
            try
            {
                var generation = new IslandGenerationSettings
                {
                    TerrainShape = IslandTerrainShape.CoastalStrip,
                    CoastalSectionCount = 8,
                    UseSnapshotCache = false,
                };
                var profile = new IslandGenerationProfile(generation, new IslandRiverSettings(),
                    new IslandForestSettings(), new IslandReedSettings(), new IslandFernSettings(),
                    new IslandRenderingSettings(), new IslandDebugSettings(), new IslandCaveSettings { Enabled = false });
                var request = new IslandGenerationRequest(42, Vector2Int.zero, profile, default);
                handle = IslandPreparationPipeline.LoadOrGenerateNative(request.RandomSeed, request.Options,
                    request.ForestOptions, request.ReedOptions, request.FernOptions, null, 0,
                    request.CaveOptions, request.CaveNetworkOptions, request.CaveWalkOptions,
                    out var generated, request.HybridForestOptions, request.CoastalStripOptions);
                Assert.That(generated, Is.True);
                Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero),
                    Marshal.PtrToStringUTF8(CaveNative.GetLastCaveGenerationError()));
                MotuNative.CreateMesh(handle, IntPtr.Zero, 0, 0, out var exported);
                try { mesh = IslandMeshInterop.CopyTerrainMesh(exported, 0, WorldSize); }
                finally { MotuNative.ReleaseMesh(ref exported); }
                var prepared = TerrainPreparation.PrepareColliderHeightMap(handle, WorldSize);
                var failures = new List<string>();
                var probes = SelectCollisionProbes(prepared);
                foreach (var inlandSouth in new[] { false, true })
                {
                    var placed = IslandGenerationRequest.FromOwnedProfile(42, Vector2Int.zero,
                        profile.Clone(), default, coastalInlandSouth: inlandSouth);
                    CheckGeneratedCollision(mesh, prepared, probes, placed.WorldRotation, failures);
                }
                Assert.That(failures, Is.Empty, string.Join("; ", failures));
            }
            finally
            {
                Object.DestroyImmediate(mesh);
                if (handle != IntPtr.Zero) MotuNative.ReleaseMotu(handle);
            }
        }

        private static void CheckGeneratedCollision(Mesh mesh, IslandPreparedColliderHeightMap prepared,
            CollisionProbe[] probes, Quaternion worldRotation, List<string> failures)
        {
            var root = new GameObject("Generated coastal collision physics");
            TerrainTileStreamer streamer = null;
            var yaw = worldRotation.eulerAngles.y;
            try
            {
                root.transform.SetPositionAndRotation(new Vector3(317f, 23f, -811f), worldRotation);
                streamer = root.AddComponent<TerrainTileStreamer>();
                streamer.worldSize = WorldSize;
                streamer.colliderHeightMap = prepared;
                streamer.colliderRoot = new GameObject("Streamed native terrain collision");
                streamer.colliderRoot.transform.SetParent(root.transform, false);
                var meshObject = new GameObject("Actual rendered LOD0 terrain");
                meshObject.transform.SetParent(root.transform, false);
                var rendered = meshObject.AddComponent<MeshCollider>();
                rendered.cookingOptions &= ~MeshColliderCookingOptions.UseFastMidphase;
                rendered.sharedMesh = mesh;
                var tolerance = Mathf.Max(.015f, prepared.verticalSize / 32767f + .005f);
                foreach (var probe in probes)
                {
                    var node = probe.node;
                    var key = new Vector2Int(node.x / (prepared.samplesPerTile - 1),
                        node.y / (prepared.samplesPerTile - 1));
                    if (!streamer.colliderTiles.TryGetValue(key, out var tile))
                    {
                        tile = streamer.CreateColliderTile(key);
                        streamer.colliderTiles.Add(key, tile);
                    }
                    Physics.SyncTransforms();
                    var local = new Vector3(-WorldSize * .5f + node.x * WorldSize / (prepared.dimension - 1),
                        WorldSize, -WorldSize * .5f + node.y * WorldSize / (prepared.dimension - 1));
                    var world = root.transform.TransformPoint(local);
                    var ray = new Ray(world, Vector3.down);
                    Assert.That(rendered.Raycast(ray, out var visual, WorldSize * 2f), Is.True,
                        $"Rendered native LOD0 missed {probe.feature}, yaw={yaw}, node={node}.");
                    var nativeHeight = prepared.WorldHeightAt(node.x, node.y) + root.transform.position.y;
                    if (Mathf.Abs(nativeHeight - visual.point.y) > .001f)
                        failures.Add($"Native sample {probe.feature}, yaw={yaw}, node={node}: rendered={visual.point.y:R}, sampled={nativeHeight:R}");
                    Assert.That(streamer.HasActiveColliderAt(visual.point), Is.True,
                        $"Coast rotation must preserve native collider dictionary keys at {node}.");
                    if (!tile.collider.Raycast(ray, out var physical, WorldSize * 2f))
                        failures.Add($"Physical MISS {probe.feature}, yaw={yaw}, node={node}");
                    else
                    {
                        var delta = physical.point.y - visual.point.y;
                        Debug.Log($"Native coast physics {probe.feature}, yaw={yaw}, node={node}, tile={key}: rendered={visual.point.y:R}, sampled={nativeHeight:R}, physical={physical.point.y:R}, delta={delta:R}, tolerance={tolerance:R}");
                        if (Mathf.Abs(delta) > tolerance)
                            failures.Add($"Physical height yaw={yaw}, node={node}: delta={delta:R}, tolerance={tolerance:R}");
                    }
                }
            }
            finally
            {
                streamer?.RemoveAllColliderTiles();
                Object.DestroyImmediate(root);
            }
        }

        private readonly struct CollisionProbe
        {
            internal readonly Vector2Int node;
            internal readonly string feature;
            internal CollisionProbe(Vector2Int node, string feature)
            { this.node = node; this.feature = feature; }
        }

        private readonly struct LandCandidate
        {
            internal readonly Vector2Int node;
            internal readonly int cell;
            internal readonly float height;
            internal readonly float gradient;
            internal LandCandidate(Vector2Int node, int cell, float height, float gradient)
            { this.node = node; this.cell = cell; this.height = height; this.gradient = gradient; }
        }

        private static CollisionProbe[] SelectCollisionProbes(IslandPreparedColliderHeightMap prepared)
        {
            var candidates = LandCandidates(prepared);
            Assert.That(candidates.Count, Is.GreaterThan(0), "The bay must retain visible land.");
            var connected = InlandConnectedLand(candidates);
            var probes = new List<CollisionProbe>();
            var half = prepared.dimension / 2;
            AddProbe("shore/beach", _ => true, value => -value.height);
            AddProbe("back mountain", value => value.node.y > prepared.dimension * .85f,
                value => value.height);
            AddProbe("cliff", value => value.gradient < 6f, value => value.gradient);
            AddProbe("western headland", value => value.node.x < prepared.dimension * .25f,
                value => -value.node.y);
            AddProbe("eastern headland", value => value.node.x > prepared.dimension * .75f,
                value => -value.node.y);
            AddProbe("interior land", value => value.node.y > half,
                value => -Mathf.Abs(value.node.x - half) - Mathf.Abs(value.node.y - prepared.dimension * .75f));
            AddProbe("seam collar", value => value.node.x < prepared.dimension * .08f,
                value => value.height);
            AddProbe("detached offshore land", value => !connected[value.cell], value => value.height);
            Assert.That(probes.Count, Is.GreaterThanOrEqualTo(5),
                "Physics must cover distinct land features and multiple native tile keys.");
            Debug.Log("Coastal physics features: " + string.Join("; ", probes.ConvertAll(value => $"{value.feature}@{value.node}")));
            return probes.ToArray();

            void AddProbe(string feature, Func<LandCandidate, bool> accepts, Func<LandCandidate, float> score)
            {
                LandCandidate? best = null;
                var bestScore = float.NegativeInfinity;
                foreach (var value in candidates)
                {
                    if (!accepts(value) || probes.Exists(probe => probe.node == value.node)) continue;
                    var current = score(value);
                    if (current <= bestScore) continue;
                    best = value;
                    bestScore = current;
                }
                if (best.HasValue) probes.Add(new CollisionProbe(best.Value.node, feature));
            }
        }

        private static List<LandCandidate> LandCandidates(IslandPreparedColliderHeightMap prepared)
        {
            const int resolution = TerrainTileStreamer.Lod1Resolution;
            var intervals = prepared.samplesPerTile - 1;
            var spacing = WorldSize / (prepared.dimension - 1);
            var candidates = new List<LandCandidate>();
            for (var z = 0; z < resolution; z++)
            for (var x = 0; x < resolution; x++)
            {
                // Every point is an exact heightfield lattice node inside its tile.
                var node = new Vector2Int(x * intervals + intervals / 2, z * intervals + intervals / 2);
                var height = prepared.WorldHeightAt(node.x, node.y);
                if (height < .25f) continue;
                var dx = (prepared.WorldHeightAt(node.x + 1, node.y) - prepared.WorldHeightAt(node.x - 1, node.y)) / (2f * spacing);
                var dz = (prepared.WorldHeightAt(node.x, node.y + 1) - prepared.WorldHeightAt(node.x, node.y - 1)) / (2f * spacing);
                candidates.Add(new LandCandidate(node, z * resolution + x, height, Mathf.Sqrt(dx * dx + dz * dz)));
            }
            return candidates;
        }

        private static bool[] InlandConnectedLand(List<LandCandidate> candidates)
        {
            const int resolution = TerrainTileStreamer.Lod1Resolution;
            var land = new bool[resolution * resolution];
            var connected = new bool[land.Length];
            var pending = new Queue<int>();
            foreach (var value in candidates)
            {
                land[value.cell] = true;
                if (value.cell < resolution * (resolution - 1)) continue;
                connected[value.cell] = true;
                pending.Enqueue(value.cell);
            }
            while (pending.Count != 0)
            {
                var cell = pending.Dequeue();
                Visit(cell - resolution);
                Visit(cell + resolution);
                if (cell % resolution != 0) Visit(cell - 1);
                if (cell % resolution != resolution - 1) Visit(cell + 1);
            }
            return connected;

            void Visit(int cell)
            {
                if (cell < 0 || cell >= land.Length || !land[cell] || connected[cell]) return;
                connected[cell] = true;
                pending.Enqueue(cell);
            }
        }

        private static float PlaneHeight(float x, float z) => 20f + x * .035f + z * .07f;

        private static Mesh CreatePlane(Vector2Int key)
        {
            var tileSize = WorldSize / TerrainTileStreamer.Lod1Resolution;
            var x = -WorldSize * .5f + key.x * tileSize;
            var z = -WorldSize * .5f + key.y * tileSize;
            Vector3 Point(float px, float pz) => new Vector3(px, PlaneHeight(px, pz), pz);
            var mesh = new Mesh
            {
                name = "Asymmetric reference terrain",
                vertices = new[] { Point(x, z), Point(x + tileSize, z), Point(x, z + tileSize), Point(x + tileSize, z + tileSize) },
                triangles = new[] { 0, 2, 1, 1, 2, 3 },
            };
            mesh.RecalculateBounds();
            return mesh;
        }
    }
}
