using System;
using System.Linq;
using Motu.Towns;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class TownDecorationAnchorTests
    {
        [Test]
        public void BudgetedDecorationsUseAnchorSpaceAndReuseDuringTransfers()
        {
            var owner = new GameObject("Decorated town");
            var prefab = new GameObject("Sconce");
            prefab.transform.localScale = new Vector3(1f, 2f, 3f);
            var catalogue = Catalogue("sconce", prefab, null);
            try
            {
                var town = TownRenderer.Install(owner.transform, TownData(2, Sconce("lamp", 1f),
                    Sconce("no-binding", 2f, "Sconce")), 1000f, useBuildingLodBudget: true, decorationCatalogue: catalogue);
                var buildings = town.GetComponentsInChildren<TownDecorationAnchors>();
                Assert.That(buildings.Length, Is.EqualTo(2));
                Assert.That(buildings.All(building => building.transform.Find("Sconce(Clone)") == null), Is.True);

                town.BuildingLods[0].SetLevel(0);
                var prop = buildings[0].transform.Find("Sconce(Clone)");
                Assert.That(prop != null && prop.gameObject.activeInHierarchy, Is.True);
                Assert.That(buildings[0].transform.childCount, Is.EqualTo(4), "Only the exact label should resolve.");
                Assert.That(prop.localPosition, Is.EqualTo(new Vector3(1f, 3f, 2f)));
                Assert.That(prop.localScale, Is.EqualTo(prefab.transform.localScale));
                Assert.That(Vector3.Dot(prop.up, buildings[0].transform.right), Is.EqualTo(1f).Within(0.0001f));
                Assert.That(Vector3.Dot(prop.forward, buildings[0].transform.up), Is.EqualTo(1f).Within(0.0001f));

                town.BuildingLods[0].SetLevel(1);
                Assert.That(prop.gameObject.activeSelf, Is.False);
                Assert.That(prop.parent, Is.Not.EqualTo(buildings[0].transform));
                var poolRoot = prop.parent.gameObject;
                town.BuildingLods[1].SetLevel(0);
                Assert.That(buildings[1].transform.Find("Sconce(Clone)"), Is.SameAs(prop));

                buildings[1].gameObject.SetActive(false);
                Assert.That(prop == null, Is.True, "A disabled hierarchy disposes its active lease in place.");
                buildings[1].gameObject.SetActive(true);
                Assert.That(buildings[1].transform.Find("Sconce(Clone)"), Is.Null);
                buildings[1].SendMessage("LateUpdate");
                var remounted = buildings[1].transform.Find("Sconce(Clone)");
                Assert.That(remounted, Is.Not.Null);
                town.BuildingLods[1].SetLevel(1);

                town.BuildingLods[0].SetLevel(0);
                var replacement = buildings[0].transform.Find("Sconce(Clone)");
                Assert.That(replacement, Is.SameAs(remounted));
                town.BuildingLods[0].SetLevel(1);
                Assert.That(replacement.parent.gameObject, Is.SameAs(poolRoot));
                owner.SetActive(false);
                Assert.That(poolRoot == null && replacement == null, Is.True,
                    "Disabling the last owner should dispose the scene's idle pool.");
            }
            finally
            {
                Object.DestroyImmediate(owner);
                Object.DestroyImmediate(catalogue);
                Object.DestroyImmediate(prefab);
            }
        }

        [TestCase(false, TownDecorationAxis.PositiveZ, TownDecorationAxis.PositiveY, 1f)]
        [TestCase(true, TownDecorationAxis.NegativeZ, TownDecorationAxis.Default, -1f)]
        public void SignPrefabAxesFollowTheWallAnchorAcrossPoolReuse(
            bool useOverride, TownDecorationAxis facingAxis, TownDecorationAxis forwardAxis, float facingSign)
        {
            var owner = new GameObject("Decorated sign test");
            var prefab = new GameObject("Shop sign");
            var catalogue = Catalogue("wall.sign", prefab);
            try
            {
                var serialized = new SerializedObject(catalogue);
                var binding = serialized.FindProperty("entries").GetArrayElementAtIndex(0);
                var axes = binding;
                if (useOverride)
                {
                    binding.FindPropertyRelative("facingAxis").intValue = (int)TownDecorationAxis.PositiveX;
                    var overrides = binding.FindPropertyRelative("axisOverrides");
                    overrides.arraySize = 1;
                    axes = overrides.GetArrayElementAtIndex(0);
                    axes.FindPropertyRelative("prefab").objectReferenceValue = prefab;
                }
                axes.FindPropertyRelative("facingAxis").intValue = (int)facingAxis;
                axes.FindPropertyRelative("forwardAxis").intValue = (int)forwardAxis;
                serialized.ApplyModifiedPropertiesWithoutUndo();

                // Use a turned building and a twisted wall anchor to verify composition in mount space.
                var anchor = Sconce("sign", 1f, "wall.sign");
                anchor.forward = new[] { 0f, 1f, 0f };
                var town = TownRenderer.Install(owner.transform, TownData(1, anchor), 2000f,
                    useBuildingLodBudget: true, decorationCatalogue: catalogue);
                var building = town.GetComponentInChildren<TownDecorationAnchors>().transform;
                var lod = town.BuildingLods[0];
                lod.SetLevel(0);
                var sign = building.Find("Shop sign(Clone)");
                Assert.That(sign, Is.Not.Null);
                Assert.That(sign.localPosition, Is.EqualTo(new Vector3(1f, 3f, 2f)));
                Assert.That(Vector3.Dot(sign.forward * facingSign, building.right), Is.EqualTo(1f).Within(0.0001f));
                Assert.That(Vector3.Dot(sign.up, building.forward), Is.EqualTo(1f).Within(0.0001f));

                lod.SetLevel(1);
                lod.SetLevel(0);
                Assert.That(building.Find("Shop sign(Clone)"), Is.SameAs(sign));
                Assert.That(Vector3.Dot(sign.forward * facingSign, building.right), Is.EqualTo(1f).Within(0.0001f));
                Assert.That(Vector3.Dot(sign.up, building.forward), Is.EqualTo(1f).Within(0.0001f));
            }
            finally
            {
                Object.DestroyImmediate(owner);
                Object.DestroyImmediate(catalogue);
                Object.DestroyImmediate(prefab);
            }
        }

        [Test]
        public void ReorderingAnchorsPreservesTheVariantAtEachPlacedMount()
        {
            var owner = new GameObject("Decorated town variants");
            var variants = new[] { new GameObject("Iron"), new GameObject("Copper") };
            var catalogue = Catalogue("sconce", variants);
            try
            {
                var original = TownRenderer.Install(owner.transform, TownData(8, Sconce("first", 1f), Sconce("second", 2f)),
                    2000f, useBuildingLodBudget: true, decorationCatalogue: catalogue);
                var reordered = TownRenderer.Install(owner.transform, TownData(8, Sconce("second", 2f), Sconce("first", 1f)),
                    2000f, useBuildingLodBudget: true, decorationCatalogue: catalogue);

                var chosen = Variants(original);
                Assert.That(chosen.Distinct().Count(), Is.EqualTo(variants.Length));
                Assert.That(Variants(reordered), Is.EqualTo(chosen));
            }
            finally
            {
                Object.DestroyImmediate(owner);
                Object.DestroyImmediate(catalogue);
                foreach (var variant in variants) Object.DestroyImmediate(variant);
            }
        }

        [Test]
        public void StandaloneDecorationsRentOnlyAtTheClosestCameraLevel()
        {
            var owner = new GameObject("Standalone decoration test");
            var prefab = new GameObject("Sconce");
            var catalogue = Catalogue("sconce", prefab);
            var lodBias = QualitySettings.lodBias;
            var maximumLod = QualitySettings.maximumLODLevel;
            try
            {
                QualitySettings.lodBias = 1f;
                QualitySettings.maximumLODLevel = 0;
                var town = TownRenderer.Install(owner.transform, TownData(0, Sconce("lamp", 1f)), 2000f,
                    includeUnplacedPrototypes: true);
                var building = Object.Instantiate(town.SharedPrototypes[0], owner.transform, false);
                var anchors = building.GetComponent<TownDecorationAnchors>();
                var group = building.GetComponent<LODGroup>();
                group.size = 2f;
                group.localReferencePoint = Vector3.zero;
                anchors.ConfigureBaked(catalogue, "42", "test");

                var observer = new GameObject("Camera").AddComponent<Camera>();
                observer.transform.SetParent(owner.transform, false);
                observer.enabled = false;
                observer.fieldOfView = 60f;
                observer.transform.localPosition = Vector3.back * 20f;
                anchors.UpdateCameraLevel(observer);
                Assert.That(building.transform.Find("Sconce(Clone)"), Is.Null);

                observer.transform.localPosition = Vector3.back * 2f;
                anchors.UpdateCameraLevel(observer);
                var prop = building.transform.Find("Sconce(Clone)");
                Assert.That(prop != null && prop.gameObject.activeInHierarchy, Is.True);

                observer.orthographic = true;
                observer.orthographicSize = 4f;
                anchors.UpdateCameraLevel(observer);
                Assert.That(prop.gameObject.activeSelf, Is.False);
                observer.orthographicSize = 2f;
                anchors.UpdateCameraLevel(observer);
                Assert.That(building.transform.Find("Sconce(Clone)"), Is.SameAs(prop));
                anchors.UpdateCameraLevel(null);
                Assert.That(prop.gameObject.activeSelf, Is.False);

                anchors.UpdateCameraLevel(observer);
                group.SetLODs(Array.Empty<LOD>());
                anchors.UpdateCameraLevel(observer);
                Assert.That(prop.gameObject.activeSelf, Is.False);
            }
            finally
            {
                QualitySettings.lodBias = lodBias;
                QualitySettings.maximumLODLevel = maximumLod;
                Object.DestroyImmediate(owner);
                Object.DestroyImmediate(catalogue);
                Object.DestroyImmediate(prefab);
            }
        }

        private static string[] Variants(TownRenderer town)
        {
            foreach (var lod in town.BuildingLods) lod.SetLevel(0);
            return town.GetComponentsInChildren<TownDecorationAnchors>().SelectMany(building =>
                building.transform.Cast<Transform>().Where(child => child.name.EndsWith("(Clone)", StringComparison.Ordinal))
                    .OrderBy(child => child.localPosition.x).Select(child => child.name)).ToArray();
        }

        private static TownDecorationCatalogue Catalogue(string label, params GameObject[] prefabs)
        {
            var catalogue = ScriptableObject.CreateInstance<TownDecorationCatalogue>();
            var serialized = new SerializedObject(catalogue);
            var entries = serialized.FindProperty("entries");
            entries.arraySize = 1;
            var entry = entries.GetArrayElementAtIndex(0);
            entry.FindPropertyRelative("label").stringValue = label;
            var variants = entry.FindPropertyRelative("prefabs");
            variants.arraySize = prefabs.Length;
            for (var index = 0; index < prefabs.Length; index++)
                variants.GetArrayElementAtIndex(index).objectReferenceValue = prefabs[index];
            serialized.ApplyModifiedPropertiesWithoutUndo();
            return catalogue;
        }

        private static TownDecorationAnchorData Sconce(string id, float x, string label = "sconce") => new()
        {
            id = id, label = label, position = new[] { x, 2f, 3f },
            normal = new[] { 1f, 0f, 0f }, forward = new[] { 0f, 0f, 1f },
        };

        private static TownPreparedData TownData(int count, params TownDecorationAnchorData[] anchors)
        {
            var triangle = new TownMesh
            {
                positions = new[] { 0f, 0f, 0f, 1f, 0f, 0f, 0f, 1f, 0f },
                normals = new[] { 0f, 0f, 1f, 0f, 0f, 1f, 0f, 0f, 1f },
                uvs = new[] { 0f, 0f, 1f, 0f, 0f, 1f }, indices = new[] { 0, 1, 2 },
            };
            var lod = new TownLod { parts = new[] { new TownPart { material = "wall", mesh = triangle } } };
            return new TownPreparedData
            {
                seed = "42",
                prototypes = new[] { new TownPrototype
                {
                    entry_id = "test", lods = new[] { lod, lod, lod }, decoration_anchors = anchors,
                } },
                instances = Enumerable.Range(0, count).Select(index => new TownInstance
                {
                    position = new[] { 1000f + 8f * index, 1000f, 3f }, yaw_radians = 1.2f,
                }).ToArray(),
            };
        }
    }
}
