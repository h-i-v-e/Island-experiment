using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using Motu.Towns;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;
using Motu.Streaming;
using Motu.Editor;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalTownNativeIntegrationTests
    {
        private static readonly Lazy<TownPreparedData> Fixture = new Lazy<TownPreparedData>(() => TownNative.Generate(Recipe(), Input(true)));
        private static readonly Lazy<RealTownScene> RealFixture = new Lazy<RealTownScene>(GenerateRealCoast);

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void EditorBakerPersistsSupportlessPrefabsAndTheirSharedResources()
        {
            var destination = AssetDatabase.GenerateUniqueAssetPath("Assets/TownPrefabBakeTest");
            try
            {
                var paths = TownPrefabBaker.Bake(Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue"), 42, destination);
                Assert.That(paths.Length, Is.EqualTo(4));
                AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
                foreach (var path in paths)
                {
                    var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(path);
                    Assert.That(prefab, Is.Not.Null, path);
                    Assert.That(prefab.GetComponent<LODGroup>().GetLODs().Length, Is.EqualTo(3));
                    Assert.That(prefab.GetComponentsInChildren<MeshFilter>(true).All(f => EditorUtility.IsPersistent(f.sharedMesh)), Is.True);
                    var materials = prefab.GetComponentsInChildren<MeshRenderer>(true).SelectMany(r => r.sharedMaterials).Distinct().ToArray();
                    Assert.That(materials.All(EditorUtility.IsPersistent), Is.True);
                    Assert.That(materials.Where(m => m.mainTexture != null).All(m => EditorUtility.IsPersistent(m.mainTexture)), Is.True);
                    Assert.That(materials.Any(m => m.mainTexture != null), Is.True);
                    foreach (var material in materials)
                        foreach (var property in material.GetTexturePropertyNames())
                        {
                            var texture = material.GetTexture(property);
                            if (texture == null) continue;
                            var importer = (TextureImporter)AssetImporter.GetAtPath(AssetDatabase.GetAssetPath(texture));
                            Assert.That(importer.sRGBTexture, Is.EqualTo(property == "_BaseMap"), property);
                        }
                    Assert.That(prefab.GetComponentsInChildren<MeshCollider>(true), Is.Empty,
                        "Fitted town foundations belong to the runtime support mesh, not reusable shell prefabs.");
                }
            }
            finally { if (AssetDatabase.IsValidFolder(destination)) AssetDatabase.DeleteAsset(destination); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void RealCoastalTerrainExportsTownInputAndAcceptsNativeClearance()
        {
            var result = RealFixture.Value.town;
            Assert.That(result.schema_version, Is.EqualTo(2));
            Assert.That(result.clearance_json, Is.Not.Null.And.Not.Empty);
            Assert.That(result.instances.Length, Is.GreaterThan(0), "The fixed seed-42 coast must exercise actual building/support export and clearance.");
            Assert.That(result.settlements.Length, Is.GreaterThan(1));
            Assert.That(result.settlements.Count(site => site.kind == "capital"), Is.EqualTo(1));
            Assert.That(result.settlements.Select(site => site.river_index).Distinct().Count(), Is.EqualTo(result.settlements.Length));
            Assert.That(result.settlements.Sum(site => site.building_count), Is.EqualTo(result.instances.Length));
            Assert.That(result.mouth, Is.EqualTo(result.settlements.Single(site => site.kind == "capital").mouth));
            TestContext.Progress.WriteLine($"Real coast settlements: {result.settlements.Length} sites, {result.instances.Length} buildings, {result.supports.indices.Length / 3} support triangles.");
        }

        private sealed class RealTownScene
        {
            internal TownPreparedData town;
            internal IslandPreparedMesh terrain;
            internal TownBridge bridge;
            internal IslandPreparedMesh bridgeTerrain, bridgeRiver;
            internal float roadHeightMapError;
            internal bool overviewHasRoadCore;
            internal bool overviewHasCartRuts;
            internal RealWheatScene wheat;
        }

        private sealed class RealWheatScene
        {
            internal IslandPreparedWheat data;
            internal WheatField field;
            internal IslandPreparedMesh detail, distant;
            internal int overviewCanopies;
        }

        private static RealWheatScene PrepareRealWheat(IntPtr handle, IslandPreparedWheat data, IslandPreparedMesh[] overview)
        {
            if (data == null) return null;
            var field = data.Fields.OrderByDescending(plot => plot.HalfExtents.x * plot.HalfExtents.y).First();
            var centre = field.Centre + Vector2.one * 1000f;
            var area = CropArea(centre.x, centre.y, field.HalfExtents.magnitude + 8f);
            IslandPreparedMesh Prepare(int lod) =>
                TerrainGridWorker.Prepare(handle, area, lod, 1, 0, 2000f, default, data.Fields)[0];
            return new RealWheatScene
            {
                data = data, field = field, detail = Prepare(0), distant = Prepare(2),
                overviewCanopies = overview.Count(tile => tile?.wheat?.Canopy != null),
            };
        }

        private static RealTownScene GenerateRealCoast()
        {
            var generation = new IslandGenerationSettings {
                TerrainShape = IslandTerrainShape.CoastalStrip, UseSnapshotCache = false,
                CoastalSectionCount = 8, WaterRatio = 0.8f, MaximumHeightMetres = 400f
            };
            var rivers = new IslandRiverSettings {
                SourceCatchmentHectares = 0.1f, SteepSourceCatchmentMultiplier = 8f, SourceElevationBoost = 20f,
                SourceWidthMetres = 2f, MaximumWidthMetres = 5f, SourceDepthMetres = 0.35f, MaximumDepthMetres = 1f
            };
            var profile = new IslandGenerationProfile(generation, rivers, new IslandForestSettings(),
                new IslandReedSettings(), new IslandFernSettings(), new IslandRenderingSettings(), new IslandDebugSettings(),
                new IslandCaveSettings { Enabled = false });
            var request = new IslandGenerationRequest(42, Vector2Int.zero, profile, default);
            var snapshot = Environment.GetEnvironmentVariable("MOTU_TOWN_RENDER_SNAPSHOT");
            var handle = !string.IsNullOrEmpty(snapshot) && File.Exists(snapshot)
                ? MotuNative.LoadMotuSnapshot(snapshot, out _) : IntPtr.Zero;
            if (handle == IntPtr.Zero) handle = IslandPreparationPipeline.LoadOrGenerateNative(request.RandomSeed, request.Options,
                request.ForestOptions, request.ReedOptions, request.FernOptions, null, 0,
                request.CaveOptions, request.CaveNetworkOptions, request.CaveWalkOptions,
                out _, request.HybridForestOptions, request.CoastalStripOptions);
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero));
            try
            {
                var result = TownNative.Prepare(handle, request.TownRecipeJson, request.TownSeed, request.TownDependencies,
                    request.VillageRecipeJson, connectCoastalSeams: true);
                Assert.That(result.schema_version, Is.EqualTo(2));
                Assert.That(result.seed, Is.EqualTo(request.TownSeed.ToString()));
                Assert.That(result.clearance_json, Is.Not.Null.And.Not.Empty);
                Assert.That(result.instances.Length, Is.GreaterThan(0), "The fixed seed-42 coast must exercise actual building/support export and clearance.");
                var wheat = IslandPreparedWheat.Prepare(result, request.TownSeed, 2000f, default);
                var overview = TerrainTileStreamer.PrepareOverviewTiles(handle, 2000f, wheat?.Fields);
                var capital = Array.FindIndex(result.settlements, site => site.kind == "capital");
                var capitalBuilding = result.instances.First(instance => instance.settlement_index == capital);
                var bridge = result.bridgePlan?.bridges?.Where(span => MeetsRoadLandings(span, result))
                    .OrderByDescending(span => span.arch_count)
                    .ThenByDescending(span => Vector3.Distance(TownRenderer.Vector(span.start), TownRenderer.Vector(span.end)))
                    .FirstOrDefault();
                var mid = bridge == null ? Vector3.zero : (TownRenderer.Vector(bridge.start) + TownRenderer.Vector(bridge.end)) * 0.5f;
                var bridgeArea = bridge == null ? default : CropArea(mid.x, mid.z, 25f + Vector3.Distance(TownRenderer.Vector(bridge.start), mid));
                return new RealTownScene {
                    town = result, terrain = TerrainCrop(handle, CropArea(capitalBuilding.position[0], capitalBuilding.position[1], 170f)),
                    bridge = bridge, bridgeTerrain = bridge == null ? null : TerrainCrop(handle, bridgeArea),
                    bridgeRiver = bridge == null ? null : RiverCrop(handle, bridgeArea),
                    wheat = PrepareRealWheat(handle, wheat, overview),
                    roadHeightMapError = RoadHeightMapError(handle, result.road_mesh),
                    overviewHasRoadCore = overview
                        .Any(tile => tile != null && Array.Exists(tile.roadCoverage, weight => weight >= 0.98f)),
                    overviewHasCartRuts = overview
                        .Any(tile => tile != null && Array.Exists(tile.cartTrackCoverage, weight => weight >= 0.60f)),
                };
            }
            finally { MotuNative.ReleaseMotu(handle); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void RealCoastRoutesCartTracksAndMarksStreamedTerrain()
        {
            var fixture = RealFixture.Value;
            var plan = fixture.town.cartTracks;
            Assert.That(fixture.town.road_exits.Length, Is.GreaterThan(1));
            Assert.That(plan, Is.Not.Null);
            var settlements = fixture.town.settlements;
            var capital = Array.FindIndex(settlements, site => site.kind == "capital");
            Assert.That(capital, Is.GreaterThanOrEqualTo(0));
            var reached = new HashSet<int> { capital };
            foreach (var link in plan.links.Where(link => link.to_settlement < settlements.Length))
            {
                Assert.That(reached.Contains(link.from_settlement), Is.True, "Every branch starts on the capital network.");
                Assert.That(reached.Add(link.to_settlement), Is.True, "A village has only one incoming road.");
            }
            Assert.That(reached.Count, Is.GreaterThan(1));
            Assert.That(plan.run_count, Is.GreaterThanOrEqualTo(plan.links.Length));
            Assert.That(fixture.overviewHasCartRuts, Is.True, "Atlas tiles retain the fine cart-rut mask.");
            foreach (var crossing in plan.bridges)
            {
                Assert.That(crossing.start.Length, Is.EqualTo(3));
                Assert.That(crossing.end.Length, Is.EqualTo(3));
                Assert.That(crossing.kind, Is.EqualTo("river").Or.EqualTo("ravine"));
            }
            TestContext.Progress.WriteLine($"Seed-42 cart tracks: {plan.links.Length} links, "
                + $"{plan.run_count} dry runs, {plan.bridges.Length} requested crossings, {plan.portals.Length} seam approaches.");
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void RealCoastRoadMeshMeetsCarvedTerrainCollision()
        {
            var fixture = RealFixture.Value;
            var roads = fixture.town.road_mesh;
            Assert.That(fixture.town.road_count, Is.GreaterThan(4),
                "The default coast has room for a connected network beyond a single access stub.");
            Assert.That(roads.indices.Length, Is.GreaterThan(0));
            Assert.That(fixture.town.instances.All(instance => instance.frontage?.street_edge?.Length == 3), Is.True);
            Assert.That(fixture.town.stair_feet, Is.Not.Empty);
            Assert.That(fixture.town.footpath_vertex_count, Is.GreaterThan(0),
                "The graded coast should paint existing terrain vertices from stair feet to road edges.");
            Assert.That(fixture.roadHeightMapError, Is.LessThan(0.15f),
                "The streamed player collision height map should follow the finished grade.");
            Assert.That(fixture.overviewHasRoadCore, Is.True, "Roads stay marked on the streamed overview terrain.");
            var ground = new GameObject("Carved town terrain collision");
            var terrainMesh = IslandMeshInterop.CreateTerrainMesh(fixture.terrain, 0);
            try
            {
                var environment = new List<Vector3>();
                terrainMesh.GetUVs(1, environment);
                Assert.That(environment.Exists(weights => weights.z >= 0.98f), Is.True, "Packed road core");
                Assert.That(environment.Exists(weights => Mathf.Abs(weights.z - 0.5f) < 0.05f), Is.True, "Gravel road edge");
                Assert.That(environment.Exists(weights => weights.z == 0f), Is.True, "Ground clear of roads");
                var collider = ground.AddComponent<MeshCollider>();
                collider.sharedMesh = terrainMesh;
                Physics.SyncTransforms();
                float maximumError = 0;
                int samples = 0;
                var bounds = terrainMesh.bounds;
                int stride = Math.Max(1, roads.indices.Length / (3 * 320));
                for (var index = 0; index < roads.indices.Length; index += 3 * stride)
                {
                    var centre = (RoadPosition(roads, roads.indices[index])
                        + RoadPosition(roads, roads.indices[index + 1])
                        + RoadPosition(roads, roads.indices[index + 2])) / 3f;
                    if (centre.x <= bounds.min.x + 0.5f || centre.x >= bounds.max.x - 0.5f
                        || centre.z <= bounds.min.z + 0.5f || centre.z >= bounds.max.z - 0.5f) continue;
                    Assert.That(collider.Raycast(new Ray(centre + Vector3.up * 5f, Vector3.down),
                        out var hit, 10f), Is.True, $"No native terrain collision at {centre}");
                    maximumError = Mathf.Max(maximumError, Mathf.Abs(hit.point.y - centre.y));
                    samples++;
                }
                TestContext.Progress.WriteLine($"Carved road collision. {fixture.town.road_count} road sections, {samples} samples, maximum error {maximumError:F3} m.");
                Assert.That(samples, Is.GreaterThan(0));
                Assert.That(maximumError, Is.LessThan(0.12f), "Roads and native collision should share the finished grade.");
            }
            finally
            {
                Object.DestroyImmediate(ground);
                Object.DestroyImmediate(terrainMesh);
            }
        }

        private static float RoadHeightMapError(IntPtr handle, TownMesh roads)
        {
            if (roads?.indices == null || roads.indices.Length == 0) return float.PositiveInfinity;
            var heights = TerrainPreparation.PrepareColliderHeightMap(handle, 2000f);
            float maximumError = 0;
            int stride = Math.Max(1, roads.indices.Length / (3 * 160));
            for (var index = 0; index < roads.indices.Length; index += 3 * stride)
            {
                var centre = (RoadPosition(roads, roads.indices[index])
                    + RoadPosition(roads, roads.indices[index + 1])
                    + RoadPosition(roads, roads.indices[index + 2])) / 3f;
                var grid = new Vector2(centre.x + 1000f, centre.z + 1000f) * ((heights.dimension - 1) / 2000f);
                var x = Mathf.FloorToInt(grid.x);
                var z = Mathf.FloorToInt(grid.y);
                var left = Mathf.Lerp(heights.WorldHeightAt(x, z), heights.WorldHeightAt(x, z + 1), grid.y - z);
                var right = Mathf.Lerp(heights.WorldHeightAt(x + 1, z), heights.WorldHeightAt(x + 1, z + 1), grid.y - z);
                maximumError = Mathf.Max(maximumError, Mathf.Abs(Mathf.Lerp(left, right, grid.x - x) - centre.y));
            }
            TestContext.Progress.WriteLine($"Road error from the player collision height map. {maximumError:F3} m maximum.");
            return maximumError;
        }

        private static bool MeetsRoadLandings(TownBridge bridge, TownPreparedData town)
        {
            bool Matches(float[] a, float[] b) => bridge.start.SequenceEqual(a) && bridge.end.SequenceEqual(b);
            return bridge.surface == "cart_track"
                ? town.cartTracks?.bridges?.Any(span => Matches(span.start, span.end)) == true
                : town.bridge_candidates?.Any(span => Matches(span.from, span.to)) == true;
        }

        private static TownPreparedData BridgeOnly(RealTownScene fixture)
        {
            Assert.That(fixture.bridge, Is.Not.Null,
                "The seed-42 coast must produce a complete crossing between two road landings. " +
                string.Join(", ", fixture.town.bridgePlan?.diagnostics?.Select(d => d.reason) ?? Array.Empty<string>()));
            return new TownPreparedData { bridgePlan = new TownBridgePlan { bridges = new[] { fixture.bridge } } };
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void RealCoastStoneBridgeMeetsBothRoadsAndRetainsPhysicalParapetsAndOpenArches()
        {
            var fixture = RealFixture.Value;
            var data = BridgeOnly(fixture);
            var bridge = fixture.bridge;
            var root = new GameObject("Stone bridge landing and clearance fixture");
            Mesh ground = null;
            try
            {
                ground = IslandMeshInterop.CreateTerrainMesh(fixture.bridgeTerrain, 0);
                var town = TownRenderer.Install(root.transform, data, 2000f);
                Assert.That(town.BridgeCount, Is.EqualTo(1));
                var group = town.GetComponentsInChildren<LODGroup>().Single();
                var levels = group.GetLODs();
                Assert.That(levels.Length, Is.GreaterThanOrEqualTo(2));
                Assert.That(levels.First().renderers.Sum(Triangles), Is.GreaterThan(levels.Last().renderers.Sum(Triangles)));
                var collision = town.GetComponentsInChildren<MeshCollider>().Single();
                var start = TownRenderer.Position(bridge.start, 2000f);
                var end = TownRenderer.Position(bridge.end, 2000f);
                TestContext.Progress.WriteLine($"Seed-42 bridge {bridge.id} spans {Vector3.Distance(start, end):F2} m " +
                    $"with {bridge.arch_count} arches and a {bridge.width:F2} m {bridge.surface} carriageway.");
                var along = Vector3.ProjectOnPlane(end - start, Vector3.up).normalized;
                var terrain = root.AddComponent<MeshCollider>();
                terrain.sharedMesh = ground;
                Physics.SyncTransforms();
                AssertAbutmentMeetsTerrain(bridge, start, terrain);
                AssertAbutmentMeetsTerrain(bridge, end, terrain);
                // Sample just inside the triangle boundary at both exact road ends.
                foreach (var landing in new[] { start + along * 0.04f, end - along * 0.04f })
                {
                    Assert.That(collision.Raycast(new Ray(landing + Vector3.up, Vector3.down), out var hit, 2f), Is.True);
                    Assert.That(hit.point.y, Is.EqualTo(landing.y).Within(0.03f), "Road-to-bridge step");
                }
                var near = DeckSections(bridge.deck);
                var far = DeckSections(bridge.coarse_deck);
                foreach (var sections in new[] { near, far })
                {
                    AssertLanding(sections.First(), start, bridge.width);
                    AssertLanding(sections.Last(), end, bridge.width);
                }
                var middle = near[near.Length / 2];
                var carriageway = (middle.left + middle.right) * 0.5f;
                Assert.That(collision.Raycast(new Ray(carriageway + Vector3.up, Vector3.down), out var deckHit, 2f), Is.True);
                foreach (var edge in new[] { middle.left, middle.right })
                    Assert.That(collision.Raycast(new Ray(deckHit.point + Vector3.up * 0.45f, (edge - carriageway).normalized),
                        out _, bridge.width + 1f), Is.True, "Both parapets should block a character on the carriageway.");
                AssertOpenArch(levels.First(), near, terrain);
                AssertOpenArch(levels.Last(), far, terrain);
            }
            finally
            {
                Object.DestroyImmediate(root);
                if (ground != null) Object.DestroyImmediate(ground);
            }
        }

        private static int Triangles(Renderer renderer) =>
            renderer.GetComponent<MeshFilter>().sharedMesh.triangles.Length / 3;

        private static void AssertAbutmentMeetsTerrain(TownBridge bridge, Vector3 landing, MeshCollider ground)
        {
            var masonry = bridge.masonry;
            var nearBank = Enumerable.Range(0, masonry.positions.Length / 3).Select(i => RoadPosition(masonry, i))
                .Where(v => Vector3.ProjectOnPlane(v - landing, Vector3.up).magnitude <= bridge.width * 0.5f + 0.6f)
                .OrderBy(v => v.y).First();
            Assert.That(ground.Raycast(new Ray(nearBank + Vector3.up * 128f, Vector3.down), out var hit, 256f), Is.True);
            Assert.That(nearBank.y, Is.LessThan(hit.point.y), "The abutment footing should extend into the actual bank.");
        }

        private static (Vector3 left, Vector3 right)[] DeckSections(TownMesh deck)
        {
            return Enumerable.Range(0, deck.uvs.Length / 2).GroupBy(i => deck.uvs[i * 2])
                .OrderBy(group => group.Key).Select(group => {
                    var offsets = group.OrderBy(i => deck.uvs[i * 2 + 1]).ToArray();
                    return (RoadPosition(deck, offsets.First()), RoadPosition(deck, offsets.Last()));
                }).ToArray();
        }

        private static void AssertLanding((Vector3 left, Vector3 right) section, Vector3 landing, float width)
        {
            Assert.That(Vector3.Distance((section.left + section.right) * 0.5f, landing), Is.LessThan(0.01f));
            Assert.That(Vector3.Distance(section.left, section.right), Is.EqualTo(width).Within(0.01f));
        }

        private static void AssertOpenArch(LOD level, (Vector3 left, Vector3 right)[] sections, MeshCollider terrain)
        {
            // Probe the actual meshes assigned to this LOD, independently of its walking collider.
            var meshes = level.renderers.Select(renderer => {
                var probe = renderer.gameObject.AddComponent<MeshCollider>();
                probe.sharedMesh = renderer.GetComponent<MeshFilter>().sharedMesh;
                return probe;
            }).ToArray();
            Physics.SyncTransforms();
            bool Hit(Ray ray, float range) => meshes.Any(mesh => mesh.Raycast(ray, out _, range));
            foreach (var section in sections)
            {
                var road = (section.left + section.right) * 0.5f;
                if (!terrain.Raycast(new Ray(road + Vector3.up * 8f, Vector3.down), out var bed, 128f)) continue;
                var across = section.right - section.left;
                var offset = across + across.normalized * 2f;
                // Require the opening to be above the actual ravine or riverbed.
                for (var depth = 0.25f; road.y - depth > bed.point.y + 0.05f; depth += 0.25f)
                {
                    var under = road - Vector3.up * depth;
                    if (!Hit(new Ray(under, Vector3.up), depth + 2f)) continue;
                    if (!Hit(new Ray(under - offset, across.normalized), offset.magnitude * 2f)) return;
                }
            }
            Assert.Fail("A transverse sightline must pass above the riverbed and under an arch soffit at this LOD.");
        }

        private static Vector3 RoadPosition(TownMesh mesh, int index)
        {
            var offset = index * 3;
            return new Vector3(mesh.positions[offset] - 1000f, mesh.positions[offset + 2],
                mesh.positions[offset + 1] - 1000f);
        }

        private static MotuNative.ExportArea CropArea(float centreX, float centreY, float radius) =>
            new MotuNative.ExportArea(Mathf.Clamp01((centreX - radius) / 2000f),
                Mathf.Clamp01((centreY - radius) / 2000f), Mathf.Clamp01((centreX + radius) / 2000f),
                Mathf.Clamp01((centreY + radius) / 2000f));

        private static IslandPreparedMesh RiverCrop(IntPtr handle, MotuNative.ExportArea area)
        {
            MotuNative.CreateRiverMeshGrid(handle, ref area, 1, out var grid);
            try
            {
                Assert.That(grid.length, Is.EqualTo(1));
                var tile = Marshal.PtrToStructure<MotuNative.ExportMesh>(grid.data);
                return tile.triangles.length == 0 ? null : IslandMeshInterop.CopyRiverMeshData(tile, 2000f);
            }
            finally { MotuNative.ReleaseMeshGrid(ref grid); }
        }

        private static IslandPreparedMesh TerrainCrop(IntPtr handle, MotuNative.ExportArea area)
        {
            var memory = Marshal.AllocHGlobal(Marshal.SizeOf<MotuNative.ExportArea>());
            try
            {
                Marshal.StructureToPtr(area, memory, false);
                MotuNative.CreateMesh(handle, memory, 0, 0, out var export);
                try { return IslandMeshInterop.CopyTerrainMeshData(export, 0, 2000f, handle); }
                finally { MotuNative.ReleaseMesh(ref export); }
            }
            finally { Marshal.FreeHGlobal(memory); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void NativeBuildingPackageImportsGeometryTexturesDoorsAndCombinedSupports()
        {
            var data = Fixture.Value;
            Assert.That(data.instances.Length, Is.EqualTo(2));
            Assert.That(data.prototypes.Length, Is.EqualTo(1));
            Assert.That(data.prototypes[0].lods.Length, Is.EqualTo(3));
            Assert.That(data.supports.indices.Length, Is.GreaterThan(0));
            Assert.That(data.supports.grub_positions.Length, Is.EqualTo(data.supports.positions.Length));
            Assert.That(data.support_material.grub_uniform.Length, Is.EqualTo(20));
            var owner = new GameObject("Native town import fixture");
            try
            {
                var town = TownRenderer.Install(owner.transform, data, 2000f);
                Assert.That(town.BuildingCount, Is.EqualTo(2));
                Assert.That(town.GetComponentsInChildren<TownDoor>().Length, Is.GreaterThanOrEqualTo(2));
                foreach (var group in town.GetComponentsInChildren<LODGroup>())
                {
                    var lods = group.GetLODs();
                    Assert.That(lods[2].screenRelativeTransitionHeight, Is.Zero);
                    var doors = group.GetComponentsInChildren<TownDoor>().SelectMany(door => door.GetComponentsInChildren<Renderer>());
                    Assert.That(doors, Is.SubsetOf(lods[0].renderers));
                    Assert.That(doors.Intersect(lods[1].renderers.Concat(lods[2].renderers)), Is.Empty);
                    var atlas = lods[2].renderers.Select(renderer => renderer.sharedMaterial)
                        .Single(material => material.name.EndsWith(" atlas", StringComparison.Ordinal));
                    Assert.That(atlas.GetTextureScale("_BaseMap"), Is.EqualTo(new Vector2(1f, -1f)));
                    Assert.That(atlas.GetTextureOffset("_BaseMap"), Is.EqualTo(Vector2.up));
                }
                Assert.That(town.GetComponentsInChildren<MeshCollider>().Length, Is.EqualTo(1));
                var supports = town.GetComponentInChildren<MeshCollider>();
                Assert.That(supports.GetComponent<MeshRenderer>().sharedMaterials.Length, Is.EqualTo(1));
                Physics.SyncTransforms();
                var mesh = supports.sharedMesh;
                var vertices = mesh.vertices;
                var triangles = mesh.triangles;
                var foundSupport = false;
                for (var triangle = 0; triangle < triangles.Length && !foundSupport; triangle += 3)
                {
                    if (mesh.normals[triangles[triangle]].y < 0.9f) continue;
                    var local = (vertices[triangles[triangle]] + vertices[triangles[triangle + 1]] + vertices[triangles[triangle + 2]]) / 3f;
                    var surface = supports.transform.TransformPoint(local);
                    foundSupport = supports.Raycast(new Ray(surface + Vector3.up * 0.1f, Vector3.down), out var hit, 0.2f)
                        && Vector3.Distance(hit.point, surface) < 0.002f;
                }
                Assert.That(foundSupport, Is.True, "The imported native support mesh must provide a physical upward-facing walking surface.");
                Assert.That(town.GetComponentsInChildren<MeshRenderer>().SelectMany(r => r.sharedMaterials)
                    .Any(m => m.mainTexture != null), Is.True);
            }
            finally { Object.DestroyImmediate(owner); }
        }

        [Test, Category("NativeIntegration")]
        public void BuildingLodBudgetPromotesTheClosestBuildingsAcrossTowns()
        {
            var owner = new GameObject("Shared town LOD budget fixture");
            try
            {
                TownRenderer PlaceTown(int first)
                {
                    var data = new TownPreparedData {
                        prototypes = Fixture.Value.prototypes,
                        instances = Enumerable.Range(first, 15)
                            .Select(index => new TownInstance { position = new[] { 1000f + index * 2.5f, 1000f, 0f } })
                            .ToArray()
                    };
                    return TownRenderer.Install(owner.transform, data, 2000f, useBuildingLodBudget: true);
                }

                var west = PlaceTown(0);
                var east = PlaceTown(15);
                var buildings = west.BuildingLods.Concat(east.BuildingLods).ToArray();
                var budget = new TownBuildingLodBudget();
                void MoveViewer(float x)
                {
                    var viewer = Vector3.right * x;
                    budget.Consider(west, viewer);
                    budget.Consider(east, viewer);
                    budget.Apply();
                }

                Assert.That(buildings.All(building => building.Level == 2), Is.True);
                MoveViewer(0f);
                Assert.That(buildings.Count(building => building.Level == 0), Is.EqualTo(TownBuildingLodBudget.InteriorLimit));
                Assert.That(buildings.Count(building => building.Level == 1), Is.EqualTo(TownBuildingLodBudget.ExteriorLimit));
                Assert.That(buildings.First().Level, Is.Zero);
                Assert.That(buildings.Last().Level, Is.EqualTo(2));
                MoveViewer(72.5f);
                Assert.That(buildings.First().Level, Is.EqualTo(2));
                Assert.That(buildings.Last().Level, Is.Zero);
                MoveViewer(500f);
                Assert.That(buildings.All(building => building.Level == 2), Is.True);
            }
            finally { Object.DestroyImmediate(owner); }
        }

        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void NativeRiverlessInputReturnsEmptyPackage()
        {
            var result = TownNative.Generate(Recipe(), Input(false));
            Assert.That(result.instances, Is.Empty);
            Assert.That(result.prototypes, Is.Empty);
            Assert.That(result.mouth, Is.Empty);
        }

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator NativeTownMaterialsRenderOnMetal()
            => RenderTown(Fixture.Value, null, "native-town-fixture");

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator RealCoastalTownAndFittedSupportsRenderOnNativeTerrain()
            => RenderTown(RealFixture.Value.town, RealFixture.Value.terrain, "real-coast-town");

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator RealCoastStoneBridgeRendersAtBothLodsOnNativeTerrain()
        {
            var fixture = RealFixture.Value;
            return RenderTown(BridgeOnly(fixture), fixture.bridgeTerrain, "real-coast-stone-bridge", fixture.bridge, fixture.bridgeRiver);
        }

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator RealCoastCapitalPierRendersWithoutClutter()
            => RenderTown(RealFixture.Value.town, RealFixture.Value.terrain, "real-coast-capital-pier", capturePier: true);

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator RealCoastWheatRendersAsClumpsAndAsATerrainCanopy()
        {
            var fixture = RealFixture.Value;
            var wheat = fixture.wheat;
            Assert.That(wheat, Is.Not.Null, "The coast must leave at least one dry vacant plot.");
            Assert.That(wheat.detail?.wheat?.Plants.Length ?? 0, Is.GreaterThan(100));
            Assert.That(wheat.distant?.wheat?.Canopy?.triangles.Length ?? 0, Is.GreaterThan(0));
            Assert.That(wheat.overviewCanopies, Is.GreaterThan(0));
            TestContext.Progress.WriteLine($"Wheat: {wheat.data.Fields.Length} plots, {wheat.overviewCanopies} overview canopies.");
            return RenderTown(fixture.town, wheat.detail, "real-coast-wheat", wheat: wheat);
        }

        private static IEnumerator RenderTown(TownPreparedData data, IslandPreparedMesh terrain, string fileName,
            TownBridge bridge = null, IslandPreparedMesh river = null, RealWheatScene wheat = null, bool capturePier = false)
        {
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
                Assert.Ignore("Town camera validation requires a graphics device.");
            var oldPipeline = GraphicsSettings.defaultRenderPipeline;
            var oldQualityPipeline = QualitySettings.renderPipeline;
            var oldAmbient = RenderSettings.ambientLight;
            var oldMode = RenderSettings.ambientMode;
            var oldProbe = RenderSettings.ambientProbe;
            var oldTarget = RenderTexture.active;
            var root = new GameObject("Native town camera fixture");
            var renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
            var pipeline = UniversalRenderPipelineAsset.Create(renderer);
            var target = new RenderTexture(1024, 768, 24);
            var image = new Texture2D(1024, 768, TextureFormat.RGB24, false);
            Material groundMaterial = null;
            TerrainMaterialTextureArrays terrainTextures = null;
            Mesh groundMesh = null;
            Mesh riverMesh = null;
            Material riverMaterial = null;
            using var riverNoise = river == null ? null : SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.River);
            try
            {
                GraphicsSettings.defaultRenderPipeline = pipeline;
                QualitySettings.renderPipeline = pipeline;
                if (terrain != null)
                {
                    var colours = new IslandMaterialColours(
                        new Color(0.30f, 0.21f, 0.12f), new Color(0.34f, 0.36f, 0.32f),
                        new Color(0.62f, 0.57f, 0.34f));
                    var bakedMaterials = MaterialPreparation.PrepareMaterialTextures(colours, 128);
                    terrainTextures = new TerrainMaterialTextureArrays(bakedMaterials);
                    groundMaterial = new Material(Shader.Find("Motu/Terrain Unified"));
                    groundMaterial.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                    groundMaterial.SetFloat("_WorldNormalWeight", 0f);
                    terrainTextures.BindTerrain(groundMaterial);
                    Assert.That(groundMaterial.GetFloat("_RoadHeightRange"), Is.EqualTo(
                        bakedMaterials.road.maximumHeight - bakedMaterials.road.minimumHeight));
                    Assert.That(((Texture2DArray)groundMaterial.GetTexture("_TerrainAlbedoArray")).depth, Is.EqualTo(7));
                    groundMesh = IslandMeshInterop.CreateTerrainMesh(terrain, 0);
                }
                var town = TownRenderer.Install(root.transform, data, 2000f, roadsOnTerrain: terrain != null,
                    terrainTextures: terrainTextures);
                if (terrain != null)
                    Assert.That(town.transform.Find("Town roads"), Is.Null, "Roads are part of the terrain mesh.");
                else if (data.road_count > 0)
                    Assert.That(town.transform.Find("Town roads"), Is.Not.Null, "Standalone town previews show roads.");
                var bounds = town.GetComponentsInChildren<MeshRenderer>().Select(r => r.bounds)
                    .Aggregate((a, b) => { a.Encapsulate(b); return a; });
                var ground = new GameObject("Native terrain beneath town");
                ground.transform.SetParent(root.transform, false);
                if (terrain == null)
                {
                    groundMaterial = new Material(Shader.Find("Universal Render Pipeline/Lit"));
                    groundMaterial.SetColor("_BaseColor", new Color(0.16f, 0.23f, 0.09f));
                    groundMesh = new Mesh {
                        vertices = new[] { new Vector3(bounds.min.x - 10, 2, bounds.min.z - 10), new Vector3(bounds.max.x + 10, 2, bounds.min.z - 10),
                            new Vector3(bounds.min.x - 10, 2, bounds.max.z + 10), new Vector3(bounds.max.x + 10, 2, bounds.max.z + 10) },
                        triangles = new[] { 0, 2, 1, 1, 2, 3 }, uv = new[] { Vector2.zero, Vector2.right, Vector2.up, Vector2.one }
                    };
                    groundMesh.RecalculateNormals();
                }
                var groundFilter = ground.AddComponent<MeshFilter>();
                groundFilter.sharedMesh = groundMesh;
                ground.AddComponent<MeshRenderer>().sharedMaterial = groundMaterial;
                if (river != null)
                {
                    var water = new GameObject("River surface under the bridge");
                    water.transform.SetParent(root.transform, false);
                    riverMesh = IslandMeshInterop.CreateRiverMesh(river);
                    riverMaterial = new Material(Shader.Find("Motu/River Water"));
                    riverMaterial.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                    riverMaterial.SetTexture("_NoiseTex", riverNoise.Texture);
                    water.AddComponent<MeshFilter>().sharedMesh = riverMesh;
                    water.AddComponent<MeshRenderer>().sharedMaterial = riverMaterial;
                    pipeline.supportsCameraDepthTexture = true;
                    pipeline.supportsCameraOpaqueTexture = true;
                }
                var cameraRoot = new GameObject("Town review camera");
                cameraRoot.transform.SetParent(root.transform);
                var camera = cameraRoot.AddComponent<Camera>();
                camera.enabled = false;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = new Color(0.08f, 0.14f, 0.22f);
                camera.orthographic = true;
                camera.orthographicSize = terrain == null ? bounds.extents.magnitude * 1.1f : 110f;
                camera.nearClipPlane = 0.1f;
                camera.farClipPlane = 2000f;
                var focus = terrain == null ? bounds.center : town.InspectionLocalPosition;
                camera.transform.position = focus + new Vector3(18f, 15f, -22f).normalized * bounds.extents.magnitude * 3f;
                camera.transform.LookAt(focus);
                var sunRoot = new GameObject("Town review sun");
                sunRoot.transform.SetParent(root.transform);
                var sun = sunRoot.AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 0.55f;
                sun.transform.rotation = Quaternion.Euler(45f, -30f, 0f);
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = new Color(0.6f, 0.6f, 0.6f);
                var probe = new SphericalHarmonicsL2();
                probe.AddAmbientLight(new Color(0.2f, 0.2f, 0.2f));
                RenderSettings.ambientProbe = probe;
                yield return null;
                if (capturePier)
                {
                    CapturePier(town, camera, target, image, fileName);
                    yield break;
                }
                if (wheat != null)
                {
                    CaptureWheat(root.transform, groundFilter, groundMaterial, wheat, camera, target, image, fileName);
                    yield break;
                }
                if (bridge != null)
                {
                    CaptureBridge(bridge, town, camera, target, image, fileName);
                    yield break;
                }
                CaptureFrame(camera, target, image, fileName + ".png");
                if (data.road_count > 0)
                {
                    camera.transform.position = focus + Vector3.up * bounds.size.magnitude;
                    camera.transform.LookAt(focus, Vector3.forward);
                    CaptureFrame(camera, target, image, fileName + "-road-plan.png");
                    if (terrain != null)
                    {
                        groundMaterial.EnableKeyword("MOTU_TERRAIN_LOD2");
                        CaptureFrame(camera, target, image, fileName + "-road-lod2.png");
                        groundMaterial.DisableKeyword("MOTU_TERRAIN_LOD2");
                    }
                }
                camera.orthographicSize = 13f;
                focus = town.InspectionLocalPosition + Vector3.up * 2f;
                camera.transform.position = focus + new Vector3(18f, 15f, -22f);
                camera.transform.LookAt(focus);
                CaptureFrame(camera, target, image, fileName + "-supports.png");
                if (terrain != null)
                {
                    CaptureRoadRelief(terrain, town.InspectionLocalPosition, groundMaterial, camera, target, image);
                    CaptureCartTrack(terrain, town.InspectionLocalPosition, camera, target, image);
                }
                else
                {
                    foreach (var group in town.GetComponentsInChildren<LODGroup>())
                        group.ForceLOD(2);
                    CaptureFrame(camera, target, image, fileName + "-painted-shells.png");
                }
            }
            finally
            {
                GraphicsSettings.defaultRenderPipeline = oldPipeline;
                QualitySettings.renderPipeline = oldQualityPipeline;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.ambientMode = oldMode;
                RenderSettings.ambientProbe = oldProbe;
                RenderTexture.active = oldTarget;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(pipeline);
                Object.DestroyImmediate(renderer);
                if (groundMaterial != null) Object.DestroyImmediate(groundMaterial);
                terrainTextures?.Dispose();
                if (groundMesh != null) Object.DestroyImmediate(groundMesh);
                if (riverMesh != null) Object.DestroyImmediate(riverMesh);
                if (riverMaterial != null) Object.DestroyImmediate(riverMaterial);
            }
        }

        private static void CapturePier(TownRenderer town, Camera camera, RenderTexture target, Texture2D image, string fileName)
        {
            var piers = town.GetComponentsInChildren<TownPierBerth>();
            Assert.That(piers.Length, Is.EqualTo(1), "The capital should own one Explorer berth.");
            var pier = piers[0];
            Assert.That(pier.MinimumDepth, Is.GreaterThanOrEqualTo(5f));
            Assert.That(pier.Length, Is.GreaterThanOrEqualTo(42f));
            Assert.That(pier.Width, Is.GreaterThanOrEqualTo(16f));
            Assert.That(pier.AccessConnected, Is.True, "The landing should reach the capital's roads.");
            Assert.That(Vector3.Dot((pier.Position - pier.ApproachPosition).normalized, pier.Forward),
                Is.GreaterThan(0.99f), "The ship must be able to sail straight into the berth.");
            Assert.That(town.CapitalPierPropPointCount, Is.GreaterThan(0));
            var group = pier.GetComponent<LODGroup>();
            Assert.That(group, Is.Not.Null);
            Assert.That(pier.GetComponent<MeshCollider>()?.sharedMesh, Is.Not.Null);
            var lods = group.GetLODs();
            Assert.That(lods.Length, Is.EqualTo(2));
            var walkway = town.CopyTownNavigationMeshes(town.transform).Last();
            Assert.That(walkway.triangles.Length, Is.GreaterThanOrEqualTo(6));
            Assert.That(pier.GetComponentsInChildren<MeshRenderer>().Length,
                Is.EqualTo(lods.Sum(lod => lod.renderers.Length)), "Prop points should have no prefab instances by default.");
            var bounds = lods[0].renderers.Select(surface => surface.bounds)
                .Aggregate((a, b) => { a.Encapsulate(b); return a; });
            var focus = new Vector3(bounds.center.x, bounds.max.y - 0.4f, bounds.center.z);
            var scale = bounds.size.magnitude;
            camera.orthographicSize = scale * 0.56f;
            camera.transform.position = focus + new Vector3(0.75f, 0.63f, -0.64f) * scale;
            camera.transform.LookAt(focus);
            // Show the structure at the surveyed mean waterline in the test fixture.
            var water = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var waterMaterial = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
            try
            {
                waterMaterial.SetColor("_BaseColor", new Color(0.08f, 0.22f, 0.28f));
                water.GetComponent<MeshRenderer>().sharedMaterial = waterMaterial;
                water.transform.position = pier.Position;
                water.transform.localScale = Vector3.one * 40f;
                pier.gameObject.SetActive(false);
                ReadFrame(camera, target, image);
                var bare = image.GetPixels32();
                pier.gameObject.SetActive(true);
                group.ForceLOD(0);
                CaptureFrame(camera, target, image, fileName + ".png");
                AssertGeometryVisible(image, bare, "Capital pier");
                group.ForceLOD(1);
                CaptureFrame(camera, target, image, fileName + "-distant.png");
                AssertGeometryVisible(image, bare, "Distant capital pier");
            }
            finally
            {
                Object.DestroyImmediate(water);
                Object.DestroyImmediate(waterMaterial);
            }
        }

        private static void CaptureWheat(Transform root, MeshFilter ground, Material groundMaterial, RealWheatScene wheat,
            Camera camera, RenderTexture target, Texture2D image, string fileName)
        {
            var field = wheat.field;
            var plants = wheat.detail.wheat.Plants;
            var middle = plants.OrderBy(plant => (new Vector2(plant.Root.x, plant.Root.z) - field.Centre).sqrMagnitude).First().Root;
            var mask = wheat.data.Mask.Bind(groundMaterial);
            var sourceMesh = ground.sharedMesh;
            Mesh distantGround = null;
            Mesh canopy = null;
            Material canopyMaterial = null;
            var canopyHost = new GameObject("Review wheat canopy");
            canopyHost.transform.SetParent(root, false);
            try
            {
                camera.orthographicSize = 3.7f;
                camera.transform.position = middle + new Vector3(6f, 4f, -7f);
                camera.transform.LookAt(middle + Vector3.up * 0.45f);
                ReadFrame(camera, target, image);
                var bare = image.GetPixels32();
                var clumpMaterial = Resources.Load<Material>("Motu/Variants/WheatClumps");
                Assert.That(clumpMaterial, Is.Not.Null);
                using (var clumps = new WheatClumpRenderer(root, clumpMaterial))
                {
                    clumps.SetTile(Vector2Int.zero, plants);
                    CaptureFrame(camera, target, image, fileName + "-clumps.png");
                    AssertGeometryVisible(image, bare, "Wheat clumps");
                }

                distantGround = IslandMeshInterop.CreateTerrainMesh(wheat.distant, 2);
                ground.sharedMesh = distantGround;
                canopy = IslandMeshInterop.CreateGeneratedMesh(wheat.distant.wheat.Canopy);
                canopyMaterial = new Material(Resources.Load<Shader>("WheatCanopy"));
                camera.orthographicSize = field.HalfExtents.magnitude * 1.15f;
                camera.transform.position = middle + new Vector3(27f, 26f, -35f);
                camera.transform.LookAt(middle);
                ReadFrame(camera, target, image);
                bare = image.GetPixels32();
                canopyHost.AddComponent<MeshFilter>().sharedMesh = canopy;
                var canopyRenderer = canopyHost.AddComponent<MeshRenderer>();
                canopyRenderer.sharedMaterial = canopyMaterial;
                canopyRenderer.shadowCastingMode = ShadowCastingMode.Off;
                CaptureFrame(camera, target, image, fileName + "-canopy.png");
                var ditheredPixels = AssertGeometryVisible(image, bare, "Wheat canopy");
                canopyMaterial.SetFloat("_WheatPorosity", 0f);
                ReadFrame(camera, target, image);
                var solidPixels = AssertGeometryVisible(image, bare, "Solid wheat reference");
                var visibleFraction = (float)ditheredPixels / solidPixels;
                Assert.That(visibleFraction, Is.InRange(0.72f, 0.96f),
                    "The canopy should show scattered patches of ground through its cutout.");
                TestContext.WriteLine($"Distant wheat covers {visibleFraction:P0} of the solid reference.");
            }
            finally
            {
                ground.sharedMesh = sourceMesh;
                Object.DestroyImmediate(canopyHost);
                Object.DestroyImmediate(canopyMaterial);
                Object.DestroyImmediate(canopy);
                Object.DestroyImmediate(distantGround);
                Object.DestroyImmediate(mask);
            }
        }

        private static void CaptureBridge(TownBridge bridge, TownRenderer town, Camera camera,
            RenderTexture target, Texture2D image, string fileName)
        {
            var start = TownRenderer.Position(bridge.start, 2000f);
            var end = TownRenderer.Position(bridge.end, 2000f);
            var focus = (start + end) * 0.5f;
            var span = Vector3.Distance(start, end);
            var along = Vector3.ProjectOnPlane(end - start, Vector3.up).normalized;
            var across = Vector3.Cross(Vector3.up, along);
            var lod = town.GetComponentInChildren<LODGroup>();
            camera.orthographicSize = Mathf.Max(5f, span * 0.46f);
            camera.transform.position = focus + (across + along * 0.18f + Vector3.up * 0.23f) * span;
            camera.transform.LookAt(focus);
            town.gameObject.SetActive(false);
            ReadFrame(camera, target, image);
            var riverAndBanks = image.GetPixels32();
            town.gameObject.SetActive(true);
            lod.ForceLOD(0);
            CaptureFrame(camera, target, image, fileName + ".png");
            AssertGeometryVisible(image, riverAndBanks, "Stone bridge");
            lod.ForceLOD(lod.lodCount - 1);
            CaptureFrame(camera, target, image, fileName + "-distant.png");
            AssertGeometryVisible(image, riverAndBanks, "Stone bridge");
            lod.ForceLOD(0);
            camera.transform.position = focus + (-along + across * 0.32f + Vector3.up * 0.68f) * span;
            camera.transform.LookAt(focus);
            CaptureFrame(camera, target, image, fileName + "-road-landing.png");
        }

        private static int AssertGeometryVisible(Texture2D image, Color32[] background, string description)
        {
            var changed = image.GetPixels32().Where((pixel, index) => {
                var previous = background[index];
                return Mathf.Abs(pixel.r - previous.r) + Mathf.Abs(pixel.g - previous.g) + Mathf.Abs(pixel.b - previous.b) > 28;
            }).Count();
            Assert.That(changed, Is.GreaterThan(background.Length / 100), description + " should occupy at least 1% of the frame.");
            return changed;
        }

        private static void CaptureRoadRelief(IslandPreparedMesh terrain, Vector3 inspection, Material ground,
            Camera camera, RenderTexture target, Texture2D image)
        {
            var focus = terrain.vertices.Where((_, index) => terrain.roadCoverage[index] >= 0.999f)
                .OrderBy(vertex => (vertex - inspection).sqrMagnitude).First();
            camera.orthographicSize = 1.55f;
            camera.transform.position = focus + new Vector3(5f, 1.8f, -4f);
            camera.transform.LookAt(focus);
            CaptureFrame(camera, target, image, "road-relief.png");
            var relief = image.GetPixels32();
            ground.SetFloat("_RoadParallaxStrength", 0f);
            CaptureFrame(camera, target, image, "road-no-parallax.png");
            var flat = image.GetPixels32();
            var changed = relief.Where((pixel, index) => !pixel.Equals(flat[index])).Count();
            Assert.That(changed, Is.GreaterThan(100), "Road relief must affect the close terrain render.");
        }

        private static void CaptureCartTrack(IslandPreparedMesh terrain, Vector3 inspection,
            Camera camera, RenderTexture target, Texture2D image)
        {
            if (terrain.cartTrackCoverage.Length != terrain.vertices.Length) return;
            var hasPaving = terrain.roadCoverage.Length == terrain.vertices.Length;
            var ruts = terrain.vertices.Where((_, i) => terrain.cartTrackCoverage[i] > 0.85f
                && (!hasPaving || terrain.roadCoverage[i] < 0.01f));
            var focus = ruts.OrderBy(point => Mathf.Abs(Vector3.Distance(point, inspection) - 110f))
                .FirstOrDefault();
            if (focus == default) return;
            camera.orthographicSize = 3.5f;
            camera.transform.position = focus + new Vector3(4f, 7f, -5f);
            camera.transform.LookAt(focus);
            CaptureFrame(camera, target, image, "cart-track-ruts.png");
        }

        private static void ReadFrame(Camera camera, RenderTexture target, Texture2D image)
        {
            RenderPipeline.SubmitRenderRequest(camera, new UniversalRenderPipeline.SingleCameraRequest { destination = target });
            RenderTexture.active = target;
            image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0);
            image.Apply();
        }

        private static void CaptureFrame(Camera camera, RenderTexture target, Texture2D image, string fileName)
        {
            ReadFrame(camera, target, image);
            var directory = Environment.GetEnvironmentVariable("MOTU_TOWN_RENDER_DIR")
                ?? Path.Combine(Path.GetTempPath(), "motu-coastal-town-render");
            var path = Path.Combine(directory, fileName);
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllBytes(path, image.EncodeToPNG());
            TestContext.Progress.WriteLine("Native town camera render: " + path);
            var pixels = image.GetPixels32();
            var magenta = pixels.Count(c => c.r > 230 && c.g < 25 && c.b > 230);
            var visible = pixels.Count(c => Mathf.Abs(c.r / 255f - camera.backgroundColor.r) > 0.1f);
            Assert.That(magenta, Is.LessThan(pixels.Length / 100));
            Assert.That(visible, Is.GreaterThan(1000));
            Assert.That(ShaderUtil.ShaderHasError(Resources.Load<Shader>("Towns/TownBuilding")), Is.False);
        }
        [Serializable] private sealed class FixtureRecipe { public int schema_version = 1; public FixtureEntry[] catalogue; }
        [Serializable] private sealed class FixtureEntry
        {
            public string id = "fixture-cottage", placement = "centre", building_json;
            public int min_count = 2, max_count = 2;
        }
        private static string Recipe()
        {
            var building = Resources.Load<TextAsset>("Towns/Recipes/cottage").text.Replace("\"atlas_size\": 1024", "\"atlas_size\": 128");
            return JsonUtility.ToJson(new FixtureRecipe { catalogue = new[] { new FixtureEntry { building_json = building } } });
        }
        private static string Input(bool rivers)
        {
            var heights = string.Join(",", Enumerable.Range(0, 441).Select(i => i < 21 ? "-2" : "2"));
            return "{\"tile_seed\":42,\"sea_level\":0,\"terrain\":{\"origin\":[0,0],\"spacing\":100,\"width\":21,\"height\":21,\"heights\":[" + heights
                + "]},\"rivers\":" + (rivers ? "[{\"points\":[[1000,1600],[1000,0]],\"widths\":[3,8]}]" : "[]") + "}";
        }
    }
}
