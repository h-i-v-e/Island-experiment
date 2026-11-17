using Motu.Islands;
using Motu.Settings;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;
using UnityEditor;

namespace Motu.Tests
{
    public sealed class CoastalTownAdapterTests
    {
        [Test]
        public void PackagedShaderImportsWithoutErrors()
        {
            var shader = Resources.Load<Shader>("Towns/TownBuilding");
            Assert.That(shader, Is.Not.Null);
            Assert.That(ShaderUtil.ShaderHasError(shader), Is.False);
        }
        [Test]
        public void DefaultCatalogueContainsCathedralAndCopiesBuildingStudioRecipes()
        {
            var catalogue = Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue");
            Assert.That(catalogue, Is.Not.Null);
            Assert.That(catalogue.Entries.Length, Is.EqualTo(3));
            Assert.That(catalogue.Entries[0].placement, Is.EqualTo(TownPlacement.Centre));
            Assert.That(catalogue.Entries[0].minimumCount, Is.EqualTo(1));
            Assert.That(catalogue.CaptureJson(), Does.Contain("building_json").And.Contain("gothic").IgnoreCase);
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
                Assert.That(first.TownSeed, Is.EqualTo(second.TownSeed));
                island.TownSettings.enabled = false;
                Assert.That(island.CreateIslandGenerationRequest(Vector2Int.zero).TownRecipeJson, Is.Null);
            }
            finally { Object.DestroyImmediate(factoryRoot); }
        }

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
