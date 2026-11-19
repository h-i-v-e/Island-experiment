using System;
using System.IO;
using Motu.Islands;
using Motu.Settings;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;
using UnityEditor;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalTownAdapterTests
    {
        [TestCase("Towns/TownBuilding")]
        [TestCase("Towns/TownRoad")]
        public void PackagedTownShadersImportWithoutErrors(string path)
        {
            var shader = Resources.Load<Shader>(path);
            Assert.That(shader, Is.Not.Null);
            Assert.That(ShaderUtil.ShaderHasError(shader), Is.False);
        }
        [Test]
        public void DefaultCatalogueContainsCathedralAndCopiesBuildingStudioRecipes()
        {
            var catalogue = Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue");
            Assert.That(catalogue, Is.Not.Null);
            Assert.That(catalogue.Entries.Length, Is.EqualTo(4));
            Assert.That(catalogue.Entries[0].placement, Is.EqualTo(TownPlacement.Centre));
            Assert.That(catalogue.Entries[0].minimumCount, Is.EqualTo(1));
            Assert.That(catalogue.CaptureJson(), Does.Contain("building_json").And.Contain("gothic").IgnoreCase);
        }

        [Test]
        public void CoastalSettingsUseDistinctDefaultCatalogues()
        {
            var settings = new CoastalTownSettings();
            var villages = Resources.Load<TownRecipeCatalogue>("Towns/DefaultVillageCatalogue");
            Assert.That(villages, Is.Not.Null);
            Assert.That(villages.Entries, Is.Not.Empty);
            var capital = settings.CaptureJson(out _, out var village);
            Assert.That(capital, Does.Contain("\"id\":\"cathedral\""));
            Assert.That(village, Does.Contain("\"id\":\"cottage\"").And.Not.Contain("\"id\":\"cathedral\""));
            Assert.That(village, Does.Contain("\"town_radius\":50"));
            settings.catalogue = villages;
            Assert.That(settings.CaptureJson(out _, out _), Is.EqualTo(village));
        }

        [Test]
        public void MaterialDependenciesAreCapturedPortablyAndMissingBindingsAreRejected()
        {
            var catalogue = ScriptableObject.CreateInstance<TownRecipeCatalogue>();
            var building = new TextAsset("{\"materials\":{\"wall_recipe\":\"materials/test-wall.json\"}}");
            var material = new TextAsset("{\"recipe\":\"test\"}");
            try
            {
                var serialized = new SerializedObject(catalogue);
                var entries = serialized.FindProperty("entries");
                entries.arraySize = 1;
                var entry = entries.GetArrayElementAtIndex(0);
                entry.FindPropertyRelative("id").stringValue = "test";
                entry.FindPropertyRelative("buildingRecipe").objectReferenceValue = building;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                Assert.Throws<System.ArgumentException>(() => catalogue.CaptureJson());
                var dependencies = serialized.FindProperty("materialDependencies");
                dependencies.arraySize = 1;
                dependencies.GetArrayElementAtIndex(0).FindPropertyRelative("referencePath").stringValue = "materials/test-wall.json";
                dependencies.GetArrayElementAtIndex(0).FindPropertyRelative("recipe").objectReferenceValue = material;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                var json = catalogue.CaptureJson(out var captured);
                Assert.That(captured.Length, Is.EqualTo(1));
                Assert.That(captured[0].json, Is.EqualTo(material.text));
                Assert.That(json, Does.Contain(captured[0].fileName).And.Not.Contain("materials/test-wall.json"));
            }
            finally { Object.DestroyImmediate(catalogue); Object.DestroyImmediate(building); Object.DestroyImmediate(material); }
        }

        [Test]
        public void SettlementCataloguesShareIdenticalMaterialFilesAndKeepDistinctContentForTheSamePath()
        {
            var capital = ScriptableObject.CreateInstance<TownRecipeCatalogue>();
            var village = ScriptableObject.CreateInstance<TownRecipeCatalogue>();
            var building = new TextAsset("{\"materials\":{\"wall_recipe\":\"wall.json\"}}");
            var material = new TextAsset("{\"recipe\":\"stone\"}");
            var otherMaterial = new TextAsset("{\"recipe\":\"wood\"}");
            try
            {
                SetMaterialBinding(capital, building, material);
                SetMaterialBinding(village, building, material);
                var settings = new CoastalTownSettings { capitalCatalogue = capital, villageCatalogue = village };
                var capitalJson = settings.CaptureJson(out var dependencies, out var villageJson);
                Assert.That(dependencies.Length, Is.EqualTo(1));
                Assert.That(capitalJson, Is.EqualTo(villageJson));

                SetMaterialBinding(village, building, otherMaterial);
                capitalJson = settings.CaptureJson(out dependencies, out villageJson);
                Assert.That(dependencies.Length, Is.EqualTo(2));
                Assert.That(capitalJson, Is.Not.EqualTo(villageJson));
                Assert.That(dependencies[0].fileName, Is.Not.EqualTo(dependencies[1].fileName));
            }
            finally
            {
                Object.DestroyImmediate(capital);
                Object.DestroyImmediate(village);
                Object.DestroyImmediate(building);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(otherMaterial);
            }
        }

        private static void SetMaterialBinding(TownRecipeCatalogue catalogue, TextAsset building, TextAsset material)
        {
            var serialized = new SerializedObject(catalogue);
            var entries = serialized.FindProperty("entries");
            entries.arraySize = 1;
            var entry = entries.GetArrayElementAtIndex(0);
            entry.FindPropertyRelative("id").stringValue = "house";
            entry.FindPropertyRelative("buildingRecipe").objectReferenceValue = building;
            var dependencies = serialized.FindProperty("materialDependencies");
            dependencies.arraySize = 1;
            dependencies.GetArrayElementAtIndex(0).FindPropertyRelative("referencePath").stringValue = "wall.json";
            dependencies.GetArrayElementAtIndex(0).FindPropertyRelative("recipe").objectReferenceValue = material;
            serialized.ApplyModifiedPropertiesWithoutUndo();
        }

        [Test]
        public void CanonicalCoastalSectionsShareTownSeedAndRecipe()
        {
            var factoryRoot = new GameObject("Town request test");
            try
            {
                var island = factoryRoot.AddComponent<SingleIsland>();
                island.GenerationSettings.TerrainShape = IslandTerrainShape.CoastalStrip;
                var first = island.CreateIslandGenerationRequest(Vector2Int.zero);
                var second = new IslandGenerationRequest(first.RandomSeed,
                    new Vector2Int(first.Generation.CoastalSectionCount, 0), first.Profile, first.MaterialColours);
                Assert.That(first.TownRecipeJson, Is.EqualTo(second.TownRecipeJson));
                Assert.That(first.VillageRecipeJson, Is.EqualTo(second.VillageRecipeJson).And.Not.Null);
                Assert.That(first.TownSeed, Is.EqualTo(second.TownSeed));
                island.TownSettings.enabled = false;
                var disabled = island.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.That(disabled.TownRecipeJson, Is.Null);
                Assert.That(disabled.VillageRecipeJson, Is.Null);
            }
            finally { Object.DestroyImmediate(factoryRoot); }
        }

        [Test]
        public void TownRoadHorizonPrefersItsOwnRecipeAndKeepsTheBaseCache()
        {
            var root = new GameObject("Town road horizon test");
            var villages = Object.Instantiate(Resources.Load<TownRecipeCatalogue>("Towns/DefaultVillageCatalogue"));
            var cache = Path.Combine(Path.GetTempPath(), "motu-town-horizon-" + Guid.NewGuid().ToString("N"));
            try
            {
                var factory = root.AddComponent<SingleIsland>();
                factory.GenerationSettings.TerrainShape = IslandTerrainShape.CoastalStrip;
                factory.GenerationSettings.UseSnapshotCache = true;
                factory.GenerationSettings.SnapshotCacheDirectory = cache;
                var roads = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.That(roads.TownRecipeJson, Does.Contain("\"roads\":"));
                Assert.That(roads.TownRoadHorizonPath, Is.Not.Null);
                Assert.That(roads.TownRoadHorizonPath, Is.Not.EqualTo(IslandHorizonCache.PathFor(roads.SnapshotPath)));

                var baseMesh = HorizonMesh(2);
                Assert.That(IslandHorizonCache.TrySave(roads.SnapshotPath, baseMesh, 1024 * 1024), Is.True);
                Assert.That(IslandPreparationPipeline.PrepareHorizon(roads, default).vertices[0].y, Is.EqualTo(2));

                Assert.That(IslandHorizonCache.TrySave(roads.TownRoadHorizonPath, HorizonMesh(1), 1024 * 1024), Is.True);
                Assert.That(IslandPreparationPipeline.PrepareHorizon(roads, default).vertices[0].y, Is.EqualTo(1));

                factory.TownSettings.roads.half_width = 4f;
                var wider = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.That(wider.TownRoadHorizonPath, Is.Not.EqualTo(roads.TownRoadHorizonPath));
                Assert.That(wider.SnapshotPath, Is.EqualTo(roads.SnapshotPath));
                Assert.That(IslandPreparationPipeline.PrepareHorizon(wider, default).vertices[0].y, Is.EqualTo(2));

                factory.TownSettings.villageCatalogue = villages;
                villages.Settings.building_gap += 1f;
                var villageChange = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.That(villageChange.TownRecipeJson, Is.EqualTo(wider.TownRecipeJson));
                Assert.That(villageChange.TownRoadHorizonPath, Is.Not.EqualTo(wider.TownRoadHorizonPath));
                Assert.That(villageChange.SnapshotPath, Is.EqualTo(wider.SnapshotPath));

                factory.TownSettings.roadsEnabled = false;
                var disabled = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.That(disabled.TownRecipeJson, Does.Not.Contain("\"roads\":"));
                Assert.That(disabled.VillageRecipeJson, Does.Not.Contain("\"roads\":"));
                Assert.That(disabled.TownRoadHorizonPath, Is.Null);
                Assert.That(IslandPreparationPipeline.PrepareHorizon(disabled, default).vertices[0].y, Is.EqualTo(2));
            }
            finally
            {
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(villages);
                if (Directory.Exists(cache)) Directory.Delete(cache, true);
            }
        }

        private static IslandPreparedMesh HorizonMesh(float height) => new IslandPreparedMesh(
            new[] { Vector3.up * height, Vector3.right, Vector3.forward }, Array.Empty<Vector3>(),
            new[] { 0, 2, 1 }, Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());

        [TestCase(2000f), TestCase(1000f)]
        public void AdapterPreservesMetreScaleWindingAndSharedPrototypeResources(float worldSize)
        {
            var owner = new GameObject("Town adapter test");
            try
            {
                var triangle = new TownMesh {
                    positions = new[] { 0f, 0f, 0f, 1f, 0f, 0f, 0f, 1f, 0f },
                    normals = new[] { 0f, 0f, 1f, 0f, 0f, 1f, 0f, 0f, 1f },
                    uvs = new[] { 0f, 0f, 1f, 0f, 0f, 1f }, indices = new[] { 0, 1, 2 }
                };
                TownLod Lod() => new TownLod { parts = new[] { new TownPart { material = "wall", mesh = triangle } } };
                var data = new TownPreparedData {
                    prototypes = new[] { new TownPrototype { entry_id = "test", hash = "shared", lods = new[] { Lod(), Lod(), Lod() },
                        doors = new[] { new TownDoorData { id = "entrance", hinge = new[] { 0f, 0f, 0f }, mesh = triangle,
                            closed_angle = 0.25f, swing_angle = 1.5f, width = 1f, height = 2f, thickness = 0.1f } },
                        colliders = new[] { new TownBox { min = new[] { 0f, 0f, 0f }, max = new[] { 1f, 2f, 3f } } } } },
                    instances = new[] {
                        new TownInstance { position = new[] { 1010f, 1020f, 3f }, prototype_index = 0 },
                        new TownInstance { position = new[] { 1040f, 1050f, 5f }, prototype_index = 0 }
                    }, supports = triangle
                };
                var town = TownRenderer.Install(owner.transform, data, worldSize);
                Assert.That(town.BuildingCount, Is.EqualTo(2));
                var first = town.transform.GetChild(1);
                var second = town.transform.GetChild(2);
                Assert.That(first.localPosition, Is.EqualTo(new Vector3(10f, 3f, 20f)));
                Assert.That(first.position, Is.EqualTo(new Vector3(10f, 3f, 20f) * (worldSize / 2000f)));
                Assert.That(first.GetComponentInChildren<MeshFilter>().sharedMesh,
                    Is.SameAs(second.GetComponentInChildren<MeshFilter>().sharedMesh));
                Assert.That(first.GetComponentInChildren<MeshRenderer>().sharedMaterial,
                    Is.SameAs(second.GetComponentInChildren<MeshRenderer>().sharedMaterial));
                var mesh = first.GetComponentInChildren<MeshFilter>().sharedMesh;
                Assert.That(mesh.triangles, Is.EqualTo(new[] { 0, 2, 1 }));
                Assert.That(Vector3.Cross(mesh.vertices[2] - mesh.vertices[0], mesh.vertices[1] - mesh.vertices[0]),
                    Is.EqualTo(Vector3.up));
                Assert.That(first.GetComponent<BoxCollider>().size, Is.EqualTo(new Vector3(1f, 3f, 2f)));
                Assert.That(town.GetComponentsInChildren<MeshCollider>().Length, Is.EqualTo(1));
                Assert.That(first.GetComponent<LODGroup>().GetLODs().Length, Is.EqualTo(3));
                var door = first.GetComponentInChildren<TownDoor>();
                door.Toggle();
                Assert.That(door.DoorId, Is.EqualTo("entrance"));
                Assert.That(Quaternion.Angle(door.transform.localRotation,
                    Quaternion.Euler(0f, -1.75f * Mathf.Rad2Deg, 0f)), Is.LessThan(0.001f));
            }
            finally { Object.DestroyImmediate(owner); }
        }

        [Test]
        public void CapitalInspectionUsesItsOwnFrontageWhenVillageIsFirst()
        {
            var owner = new GameObject("Settlement adapter test");
            try
            {
                var data = new TownPreparedData {
                    prototypes = new[] { new TownPrototype { entry_id = "test",
                        lods = new[] { new TownLod { parts = Array.Empty<TownPart>() } } } },
                    mouth = new[] { 1050f, 1000f },
                    settlements = new[] {
                        new TownSettlement { kind = "village", river_index = 2, building_count = 1,
                            centre = new[] { 300f, 400f } },
                        new TownSettlement { kind = "capital", river_index = 1, building_count = 1,
                            centre = new[] { 1050f, 1040f } },
                        new TownSettlement { kind = "village", river_index = 3, skipped_count = 2,
                            mouth = new[] { 1300f, 1000f } }
                    },
                    instances = new[] {
                        new TownInstance { position = new[] { 300f, 400f, 5f }, settlement_index = 0, entry_id = "village-test",
                            frontage = new TownFrontage { street_edge = new[] { 310f, 410f, 5f } } },
                        new TownInstance { position = new[] { 1050f, 1040f, 6f }, settlement_index = 1,
                            frontage = new TownFrontage { street_edge = new[] { 1060f, 1040f, 6f } } }
                    }
                };
                var town = TownRenderer.Install(owner.transform, data, 2000f);
                Assert.That(town.SettlementCount, Is.EqualTo(2));
                Assert.That(town.RoadFrontageCount, Is.EqualTo(2));
                Assert.That(town.transform.Find("Village (river 2)/village-test").position,
                    Is.EqualTo(new Vector3(-700f, 5f, -600f)));
                Assert.That(town.transform.Find("Capital (river 1)/test").position,
                    Is.EqualTo(new Vector3(50f, 6f, 40f)));
                Assert.That(town.transform.Find("Village (river 3)").position,
                    Is.EqualTo(new Vector3(300f, 0f, 0f)));
                Assert.That(town.InspectionLocalPosition, Is.EqualTo(new Vector3(60f, 6f, 40f)));
            }
            finally { Object.DestroyImmediate(owner); }
        }

        [Test]
        public void RiverlessOutputCreatesNoTownObjects()
        {
            var owner = new GameObject("Riverless town test");
            try
            {
                Assert.That(TownRenderer.Install(owner.transform, new TownPreparedData(), 2000f), Is.Null);
                Assert.That(owner.transform.childCount, Is.Zero);
            }
            finally { Object.DestroyImmediate(owner); }
        }
    }
}
