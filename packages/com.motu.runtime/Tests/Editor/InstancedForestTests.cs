using System;
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
    public sealed class InstancedForestTests
    {
        [TestCase("PlanetForest0", "Motu/Planet Forest")]
        [TestCase("PlanetForest1", "Motu/Planet Forest")]
        [TestCase("PlanetForest2", "Motu/Planet Forest")]
        [TestCase("PlanetForest3", "Motu/Planet Forest")]
        [TestCase("PlanetForest4", "Motu/Planet Forest")]
        public void PlayerBuildRetainsFoliageInstancingVariants(string asset, string shader)
        {
            var material = Resources.Load<Material>("Motu/Variants/" + asset);
            Assert.IsNotNull(material, "Runtime-created materials need packaged instancing variant references.");
            Assert.AreEqual(shader, material.shader.name);
            Assert.IsTrue(material.enableInstancing);
        }

        [Test]
        public void TwigCardsUseSharedTwoSidedAssetsAndExtendPrototypeBounds()
        {
            var source = Triangle();
            var twig = new IslandPreparedMesh(
                new[] { new Vector3(8, 1, 0), new Vector3(9, 1, 0), new Vector3(9, 2, 0), new Vector3(8, 2, 0) },
                new[] { Vector3.forward, Vector3.forward, Vector3.forward, Vector3.forward },
                new[] { 0, 1, 2, 0, 2, 3 },
                new[] { Vector2.zero, Vector2.right, Vector2.one, Vector2.up }, Array.Empty<Color>(), Array.Empty<Vector2>());
            var prototypes = new[] { Prototype(source, twig), Prototype(source, twig) };
            var data = new HybridForestData(prototypes, Array.Empty<HybridForestNative.Instance>(), 1, 1000);
            PlanetForestModels models = null, lease = null, otherCatalog = null;
            try
            {
                models = PlanetForestModels.Acquire(data, null);
                lease = PlanetForestModels.Acquire(data, null);
                otherCatalog = PlanetForestModels.Acquire(new HybridForestData(
                    new[] { Prototype(source, twig), Prototype(source, twig) }, data.instances, 1, 1000), null);
                Assert.AreSame(models, lease, "Islands with the same prototypes lease the same GPU meshes.");
                Assert.AreNotSame(models, otherCatalog);
                Assert.AreSame(models.Material(5), otherCatalog.Material(5), "All catalogs share the twig-card material.");
                Assert.AreEqual(4, models.Material(5).GetFloat("_ForestKind"));
                Assert.AreEqual((float)UnityEngine.Rendering.CullMode.Off, models.Material(5).GetFloat("_Cull"));
                var texture = Resources.Load<Texture2D>("Motu/TreeBranchLeafy");
                Assert.IsNotNull(texture, "Player builds must include the shared twig alpha texture.");
                Assert.AreSame(texture, models.Material(5).GetTexture("_TwigCard"));
                Assert.AreNotSame(models.Material(0), models.Material(5), "Twig cutouts have their own shared material.");
                var bark = models.Material(0).GetTexture("_ForestBarkAlbedoHeight") as Texture2DArray;
                Assert.IsNotNull(bark, "Instanced wood must use the original baked bark recipes.");
                Assert.AreEqual(3, bark.depth, "Broadleaf, pine and palm bark share one array.");
                Assert.AreSame(bark, otherCatalog.Material(0).GetTexture("_ForestBarkAlbedoHeight"),
                    "Separate islands must share bark textures.");
                Assert.AreSame(bark, models.Material(5).GetTexture("_ForestBarkAlbedoHeight"),
                    "Twig wood keeps the same bark colour as the trunk.");
                for (var p = 0; p < 2; ++p)
                {
                    Assert.IsNotNull(models.meshes[p][5], "Both broadleaf and conifer prototypes retain twig cards.");
                    Assert.AreEqual(4, models.meshes[p][5].vertexCount);
                    Assert.AreEqual(6, models.meshes[p][5].GetIndexCount(0));
                    Assert.IsTrue(models.meshes[p][5].HasVertexAttribute(UnityEngine.Rendering.VertexAttribute.TexCoord0),
                        "Twig quads retain their texture coordinates after GPU upload.");
                    Assert.GreaterOrEqual(models.bounds[p].max.x, 9, "Card extents participate in island and draw culling bounds.");
                }
                Assert.AreEqual(47, models.vertexCount, "Shared GPU statistics include cards and the intermediate pine crown.");
            }
            finally
            {
                otherCatalog?.Release();
                lease?.Release();
                models?.Release();
            }
        }

        [TestCase(0u)]
        [TestCase(1u)]
        public void TwigCardsFollowActualFineWoodDistanceWithinTheDetailedBudget(uint prototype)
        {
            var island = new GameObject("Twig distance forest");
            var cameraObject = new GameObject("Twig distance camera", typeof(Camera));
            try
            {
                var source = Triangle();
                var data = new HybridForestData(new[] { Prototype(source, source), Prototype(source, source) },
                    new[] { new HybridForestNative.Instance { anchor = new Vector3(.5f, .5f, 0), scale = 1, prototype = prototype } }, 1, 1000);
                var forest = island.AddComponent<InstancedForestRenderer>();
                forest.Initialize(island.transform, data);
                var camera = cameraObject.GetComponent<Camera>();
                camera.transform.position = Vector3.back * 29f;
                forest.Submit(camera);
                Assert.AreEqual(1, forest.SubmittedTwigCardTrees);
                Assert.AreEqual(1, forest.SubmittedDetailedCrowns);
                camera.transform.position = Vector3.back * 30f;
                forest.Submit(camera);
                Assert.AreEqual(0, forest.SubmittedTwigCardTrees, "Cards leave with fine wood at exactly 30 m.");
                Assert.AreEqual(1, forest.SubmittedDetailedCrowns, "A selected crown alone does not retain twig cards.");
                camera.transform.position = Vector3.back * 31f;
                forest.Submit(camera);
                Assert.AreEqual(0, forest.SubmittedTwigCardTrees);
                Assert.AreEqual(1, forest.SubmittedDetailedCrowns);
                camera.transform.position = Vector3.back * 29.99f;
                forest.Submit(camera);
                Assert.AreEqual(1, forest.SubmittedTwigCardTrees, "Cards use the live root distance even while nearest-tree selection is cached.");
                Assert.AreEqual(1, island.GetComponentsInChildren<Transform>().Length);
            }
            finally
            {
                island.GetComponent<InstancedForestRenderer>()?.Release();
                Object.DestroyImmediate(island);
                Object.DestroyImmediate(cameraObject);
            }
        }

        [Test]
        public void SharedForestClimateIsCopiedAndNativeSnapshotsRemainEnabled()
        {
            var forest = new IslandForestSettings { ForestLatitudeDegrees = 68f };
            JsonUtility.FromJsonOverwrite("{\"altitudeCooling\":17.5,\"variantsPerSpecies\":2}", forest);
            var generation = new IslandGenerationSettings { UseSnapshotCache = true };
            var request = new IslandGenerationRequest(42, Vector2Int.zero, generation, new IslandRiverSettings(),
                forest, new IslandReedSettings(), new IslandFernSettings(), new IslandRenderingSettings(), default);
            forest.ForestLatitudeDegrees = 0;
            JsonUtility.FromJsonOverwrite("{\"altitudeCooling\":40,\"variantsPerSpecies\":8}", forest);
            generation.UseSnapshotCache = false;
            Assert.AreEqual(68f, request.HybridForestOptions.latitudeDegrees);
            Assert.AreEqual(17.5f, request.HybridForestOptions.altitudeCooling);
            Assert.AreEqual(2u, request.HybridForestOptions.variantsPerSpecies);
            Assert.AreEqual(request.HybridForestOptions.variantsPerSpecies, (uint)request.ForestOptions.prototypeCount,
                "Placement variation must span the requested per-species catalog.");
            Assert.IsNotNull(request.SnapshotPath, "The shared forest participates in native snapshot caching.");
            Assert.AreEqual(32, Marshal.SizeOf<MotuNative.ForestOptions>(), "Legacy native ABI must remain compatible.");
            Assert.AreEqual(12, Marshal.SizeOf<HybridForestNative.Options>());
            Assert.AreEqual(24, Marshal.SizeOf<HybridForestNative.Instance>());
        }

        [TestCase(false)]
        [TestCase(true)]
        public void OldExperimentFlagsCannotSelectTheRetiredForest(bool oldFlag)
        {
            var forest = new IslandForestSettings();
            JsonUtility.FromJsonOverwrite("{\"experimentalInstancedTrees\":" + (oldFlag ? "true" : "false")
                + ",\"forestPrototypeCount\":1}", forest);
            var request = ForestRequest(forest);
            Assert.AreEqual(40f, request.HybridForestOptions.latitudeDegrees);
            Assert.AreEqual(22f, request.HybridForestOptions.altitudeCooling);
            Assert.AreEqual(4u, request.HybridForestOptions.variantsPerSpecies,
                "Old prototype-count fields must not override the four variants for each new species.");
            Assert.AreEqual(request.HybridForestOptions.variantsPerSpecies, (uint)request.ForestOptions.prototypeCount,
                "Old prototype-count JSON must not collapse native placement variation to a single variant.");
            Assert.AreEqual(ForestRequest(new IslandForestSettings()).SnapshotPath, request.SnapshotPath);
            StringAssert.DoesNotContain("experimentalInstancedTrees", JsonUtility.ToJson(request.Forest));
            StringAssert.DoesNotContain("forestPrototypeCount", JsonUtility.ToJson(request.Forest));
        }

        [Test]
        public void ExistingClimateProfilesKeepTheirSpeciesControlsWhenTheFlagIsRemoved()
        {
            var forest = new IslandForestSettings();
            JsonUtility.FromJsonOverwrite("{\"experimentalInstancedTrees\":false,\"forestLatitudeDegrees\":-33,"
                + "\"altitudeCooling\":13.5,\"variantsPerSpecies\":6,\"showForests\":false}", forest);
            var request = ForestRequest(forest);
            Assert.AreEqual(-33f, request.HybridForestOptions.latitudeDegrees);
            Assert.AreEqual(13.5f, request.HybridForestOptions.altitudeCooling);
            Assert.AreEqual(6u, request.HybridForestOptions.variantsPerSpecies);
            Assert.IsFalse(request.Forest.ShowForests);
            Assert.IsNotNull(request.SnapshotPath);
            forest.ShowForests = true;
            Assert.AreEqual(ForestRequest(forest).SnapshotPath, request.SnapshotPath,
                "Runtime forest visibility does not alter native generation or cached placement identity.");
        }

        [TestCase("{\"forestLatitudeDegrees\":41}")]
        [TestCase("{\"altitudeCooling\":23}")]
        [TestCase("{\"variantsPerSpecies\":5}")]
        public void SnapshotIdentityIncludesEverySharedForestClimateOption(string changedClimate)
        {
            var baseline = ForestRequest(new IslandForestSettings());
            var changed = new IslandForestSettings();
            JsonUtility.FromJsonOverwrite(changedClimate, changed);
            Assert.AreNotEqual(baseline.SnapshotPath, ForestRequest(changed).SnapshotPath,
                "A climate change must not restore cached trees placed for different species probabilities or catalog size.");
            Assert.AreEqual(ForestRequest(changed).SnapshotPath, ForestRequest(changed).SnapshotPath);
        }

        private static IslandGenerationRequest ForestRequest(IslandForestSettings forest) =>
            new IslandGenerationRequest(42, Vector2Int.zero, new IslandGenerationSettings { UseSnapshotCache = true },
                new IslandRiverSettings(), forest, new IslandReedSettings(), new IslandFernSettings(),
                new IslandRenderingSettings(), default);

        [TestCase(29.9f, 0)]
        [TestCase(30f, 1)]
        [TestCase(80f, 1)]
        [TestCase(250f, 2)]
        public void PineCrownsUseAllThreeDistanceLods(float distance, int expected)
        {
            var island = new GameObject("Pine LOD forest");
            var cameraObject = new GameObject("Pine LOD camera", typeof(Camera));
            try
            {
                var source = Triangle();
                var data = new HybridForestData(new[] { Prototype(source, source), Prototype(source, source) },
                    new[] { new HybridForestNative.Instance { anchor = new Vector3(.5f, .5f, 0), scale = 1, prototype = 1 } }, 1, 1000);
                var forest = island.AddComponent<InstancedForestRenderer>();
                forest.Initialize(island.transform, data);
                var camera = cameraObject.GetComponent<Camera>();
                camera.transform.position = Vector3.back * distance;
                forest.Submit(camera);
                Assert.AreEqual(expected, forest.LastPineFoliageLod);
                Assert.IsNotNull(forest.Models.meshes[1][6]);
                Assert.AreNotSame(forest.Models.meshes[1][2], forest.Models.meshes[1][6]);
                Assert.AreSame(forest.Models.Material(2), forest.Models.Material(6), "Fine and middle crowns share their palette.");
                Assert.AreEqual(expected == 0 ? 1 : 0, forest.SubmittedShellTrees, "Needles accompany LOD0 only.");
            }
            finally
            {
                island.GetComponent<InstancedForestRenderer>()?.Release();
                Object.DestroyImmediate(island);
                Object.DestroyImmediate(cameraObject);
            }
        }

        [TestCase(23f, 0)]
        [TestCase(24f, 0)]
        [TestCase(27f, 0)]
        [TestCase(29.9f, 0)]
        [TestCase(30f, 1)]
        [TestCase(80f, 1)]
        [TestCase(228f, 1)]
        [TestCase(234f, 1)]
        [TestCase(240f, 2)]
        [TestCase(250f, 2)]
        public void PalmsUseThreeGeometricLodsWithoutNeedlesOrTwigCards(float distance, int expected)
        {
            var island = new GameObject("Palm LOD forest");
            var cameraObject = new GameObject("Palm LOD camera", typeof(Camera));
            try
            {
                var source = Triangle();
                // Supplying nonempty shell/card source deliberately verifies species-based suppression.
                var prototypes = new[] { Prototype(source, source), Prototype(source, source), Prototype(source, source) };
                var data = new HybridForestData(prototypes,
                    new[] { new HybridForestNative.Instance { anchor = new Vector3(.5f, .5f, 0), scale = 1, prototype = 2 } }, 1, 1000);
                var forest = island.AddComponent<InstancedForestRenderer>();
                forest.Initialize(island.transform, data);
                var camera = cameraObject.GetComponent<Camera>();
                camera.transform.position = Vector3.back * distance;
                forest.Submit(camera);
                Assert.AreEqual(expected, forest.LastPalmFoliageLod);
                Assert.AreEqual(-1, forest.LastPineFoliageLod, "Palms never enter pine diagnostics or selection.");
                Assert.IsTrue(forest.Models.IsPalm(2));
                Assert.IsFalse(forest.Models.IsPine(2));
                Assert.IsNull(forest.Models.meshes[2][3]);
                Assert.IsNull(forest.Models.meshes[2][5]);
                Assert.IsNotNull(forest.Models.meshes[2][6]);
                Assert.AreNotSame(forest.Models.meshes[2][2], forest.Models.meshes[2][6]);
                Assert.AreSame(forest.Models.Material(2), forest.Models.Material(6));
                Assert.AreEqual(0, forest.SubmittedShellTrees);
                Assert.AreEqual(0, forest.SubmittedTwigCardTrees);
                Assert.AreEqual(expected == 2 ? 0 : 1, forest.SubmittedDetailedCrowns);
                Assert.AreEqual(distance > 228 ? 1 : 0, forest.SubmittedFarTrees);
                Assert.AreEqual(distance < 30 ? 1 : 0, forest.SubmittedPalmFineTrees);
                Assert.AreEqual(distance > 24 && distance < 240 ? 1 : 0, forest.SubmittedPalmMiddleTrees);
                Assert.AreEqual(distance > 228 ? 1 : 0, forest.SubmittedPalmFarTrees);
                var expectedBatches = (distance < 30 ? 2 : 0) + (distance > 24 && distance < 240 ? 2 : 0)
                    + (distance > 228 ? 1 : 0);
                Assert.AreEqual(expectedBatches, forest.SubmittedBatches, "Only stages with nonzero coverage are submitted.");
                Assert.AreEqual(0, forest.Models.Material(2).GetFloat("_PalmLodEnabled"),
                    "Per-view fades never alter shared materials or standalone gallery defaults.");
                Assert.AreSame(Resources.Load<Texture2D>("Motu/PalmFrond"), forest.Models.Material(2).GetTexture("_PalmFrond"));
                Assert.AreSame(forest.Models.Material(2).GetTexture("_PalmFrond"), forest.Models.Material(4).GetTexture("_PalmFrond"));
                Assert.AreEqual(UnityEngine.Rendering.VertexAttributeFormat.Float32,
                    forest.Models.meshes[2][2].GetVertexAttributeFormat(UnityEngine.Rendering.VertexAttribute.Color),
                    "The explicit palm species tag of two must survive GPU upload.");
                var tile = forest.Models.Material(0).GetVector("_ForestPalmBarkTileSize");
                var bark = HybridForestNative.SharedBarkMaps[2];
                Assert.AreEqual(bark.physicalTileWidthMetres, tile.x);
                Assert.AreEqual(bark.physicalTileHeightMetres, tile.y);
            }
            finally
            {
                island.GetComponent<InstancedForestRenderer>()?.Release();
                Object.DestroyImmediate(island);
                Object.DestroyImmediate(cameraObject);
            }
        }

        [TestCase(10f)]
        [TestCase(27f)]
        [TestCase(35f)]
        [TestCase(240f)]
        public void PalmFadeCoveragePartitionsBothTransitionsIncludingDenseStands(float radius)
        {
            var limits = PlanetForestWorld.PalmLodDistances(radius);
            Assert.AreEqual(24, limits.x);
            Assert.AreEqual(30, limits.y);
            Assert.AreEqual(radius, limits.w);
            Assert.LessOrEqual(limits.w - limits.z, 12);
            for (var distance = 0f; distance < 260; distance += .25f)
            {
                var weights = PlanetForestWorld.PalmLodWeights(distance, limits);
                Assert.That(weights.x + weights.y + weights.z, Is.EqualTo(1).Within(.00001f));
                Assert.That(weights.x, Is.InRange(0, 1));
                Assert.That(weights.y, Is.InRange(0, 1));
                Assert.That(weights.z, Is.InRange(0, 1));
                // A fixed raster-pixel threshold belongs to exactly one of the three stage intervals.
                for (var cell = 0; cell < 17; ++cell)
                {
                    var threshold = (cell + .5f) / 17;
                    var fine = threshold < weights.x;
                    var middle = threshold >= weights.x && threshold < 1 - weights.z;
                    var far = threshold >= 1 - weights.z;
                    Assert.AreEqual(1, (fine ? 1 : 0) + (middle ? 1 : 0) + (far ? 1 : 0));
                }
            }
        }

        [Test]
        public void DensePalmBoundaryKeepsUnselectedFarTreesAndSharedBatchBudget()
        {
            var island = new GameObject("Dense palm forest");
            var cameraObject = new GameObject("Dense palm camera", typeof(Camera));
            try
            {
                var source = Triangle();
                var trees = new HybridForestNative.Instance[120];
                for (var i = 0; i < trees.Length; ++i)
                    trees[i] = new HybridForestNative.Instance { anchor = new Vector3(.5f, .527f, 0), scale = 1, prototype = 2 };
                var forest = island.AddComponent<InstancedForestRenderer>();
                forest.Initialize(island.transform, new HybridForestData(
                    new[] { Prototype(source, source), Prototype(source, source), Prototype(source, source) }, trees, 1, 1000));
                forest.Submit(cameraObject.GetComponent<Camera>());
                Assert.AreEqual(120, forest.SubmittedPalmFarTrees, "Selected boundary and unselected palms both retain far coverage.");
                Assert.LessOrEqual(forest.SubmittedDetailedCrowns, PlanetForestWorld.DetailedLimit);
                Assert.AreEqual(0, forest.SubmittedShellTrees);
                Assert.LessOrEqual(forest.SubmittedBatches, 2, "Full far and fading far each pack into one shared batch.");
                Assert.AreEqual(1, island.GetComponentsInChildren<Transform>().Length);
                Assert.AreEqual(0, forest.Models.Material(4).GetFloat("_PalmLodEnabled"));
            }
            finally
            {
                island.GetComponent<InstancedForestRenderer>()?.Release();
                Object.DestroyImmediate(island);
                Object.DestroyImmediate(cameraObject);
            }
        }

        [Test]
        public void PlanetBudgetsAndGpuAssetsAreSharedAcrossIslands()
        {
            var first = new GameObject("Forest one");
            var second = new GameObject("Forest two");
            second.transform.position = Vector3.right * 30;
            var cameraObject = new GameObject("Forest camera", typeof(Camera));
            try
            {
                var source = new IslandPreparedMesh(new[] { Vector3.zero, Vector3.up * 4, Vector3.right },
                    new[] { Vector3.forward, Vector3.forward, Vector3.forward }, new[] { 0, 1, 2 },
                    Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
                var prototypes = new[] { Prototype(source, source), Prototype(source, source) };
                var trees = new HybridForestNative.Instance[600];
                for (var i = 0; i < trees.Length; ++i)
                    trees[i] = new HybridForestNative.Instance { anchor = new Vector3(.51f, .51f, .02f), scale = 1, prototype = (uint)(i % 2) };
                var data = new HybridForestData(prototypes, trees, 1, 1000);
                var a = first.AddComponent<InstancedForestRenderer>();
                var b = second.AddComponent<InstancedForestRenderer>();
                a.Initialize(first.transform, data);
                b.Initialize(second.transform, data);
                var models = a.Models;
                Assert.AreSame(models, b.Models, "Islands lease one GPU model catalog.");
                Assert.AreSame(models.Material(0), b.Models.Material(0));
                Assert.AreEqual("Motu/Planet Forest", models.Material(0).shader.name);
                Assert.AreSame(models.Material(5), b.Models.Material(5));
                Assert.AreEqual(45, a.SharedVertexCount);
                Assert.AreEqual(1, first.GetComponentsInChildren<Transform>().Length, "No per-tree rendering objects.");
                var camera = cameraObject.GetComponent<Camera>();
                camera.transform.position = new Vector3(10, 22, 0);
                camera.transform.LookAt(new Vector3(10, 22, 10));
                a.Submit(camera);
                Assert.AreEqual(96, a.SubmittedDetailedCrowns + b.SubmittedDetailedCrowns, "The 96-tree budget is global.");
                Assert.AreEqual(96, a.SubmittedTwigCardTrees + b.SubmittedTwigCardTrees, "Cards share the existing nearest-tree selection.");
                Assert.AreEqual(12, a.SubmittedShellTrees + b.SubmittedShellTrees, "Only the nearest twelve get shells.");
                Assert.LessOrEqual(a.SubmittedBatches + b.SubmittedBatches, 10, "Dense trees are packed, never submitted one by one.");
                camera.transform.position = new Vector3(10, 22, -90);
                a.Submit(camera);
                Assert.AreEqual(96, a.SubmittedDetailedCrowns + b.SubmittedDetailedCrowns, "Detailed crowns persist beyond 64 m.");
                Assert.AreEqual(0, a.SubmittedShellTrees + b.SubmittedShellTrees);
                Assert.AreEqual(0, a.SubmittedTwigCardTrees + b.SubmittedTwigCardTrees);
                camera.transform.position = new Vector3(10, 22, -390);
                a.Submit(camera);
                Assert.AreEqual(0, a.SubmittedDetailedCrowns + b.SubmittedDetailedCrowns);
                Assert.AreEqual(1200, a.SubmittedFarTrees + b.SubmittedFarTrees);
                Assert.AreEqual(6, a.SubmittedBatches + b.SubmittedBatches, "Distant draws pack across islands with the same mesh/material.");
                camera.transform.rotation = Quaternion.Euler(0, 180, 0);
                a.Submit(camera);
                Assert.AreEqual(0, a.SubmittedBatches + b.SubmittedBatches, "Invisible distant trees submit nothing.");
                camera.transform.rotation = Quaternion.identity;
                second.transform.position += Vector3.right * 10000;
                a.Submit(camera);
                Assert.AreEqual(600, a.SubmittedFarTrees + b.SubmittedFarTrees, "Moved islands invalidate their cached bounds.");
                a.Release();
                Object.DestroyImmediate(first);
                Assert.IsTrue(models.meshes[0][0] != null, "Unloading one island must not free another island's shared geometry.");
                second.transform.position = Vector3.zero;
                camera.transform.position = new Vector3(10, 22, 0);
                b.Submit(camera);
                Assert.AreEqual(96, b.SubmittedDetailedCrowns);
                camera.cullingMask = 0;
                b.Submit(camera);
                Assert.AreEqual(0, b.SubmittedBatches);
                b.Release();
                Object.DestroyImmediate(second);
                Assert.IsTrue(models.meshes[0][0] == null, "The last lease releases shared meshes.");
            }
            finally
            {
                if (first != null) { first.GetComponent<InstancedForestRenderer>()?.Release(); Object.DestroyImmediate(first); }
                if (second != null) { second.GetComponent<InstancedForestRenderer>()?.Release(); Object.DestroyImmediate(second); }
                Object.DestroyImmediate(cameraObject);
            }
        }

        private static IslandPreparedMesh Triangle() => new IslandPreparedMesh(
            new[] { Vector3.zero, Vector3.up * 4, Vector3.right },
            new[] { Vector3.forward, Vector3.forward, Vector3.forward }, new[] { 0, 1, 2 },
            Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());

        private static IslandPreparedMesh[] Prototype(IslandPreparedMesh source, IslandPreparedMesh twigs)
            => new[] { source, source, source, source, source, source, source, twigs };
    }
}
