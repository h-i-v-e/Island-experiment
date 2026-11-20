using System.Collections;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.Navigation;
using Motu.Settings;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.AI;
using UnityEngine.TestTools;

namespace Motu.Editor
{
    public sealed class CoastalBridgeNavigationTests
    {
        [UnityTest]
        public IEnumerator BridgeCrossesExcludedRiverAndChunkSeamOnRotatedIsland()
        {
            var host = new GameObject("Bridge navigation fixture");
            try
            {
                host.transform.SetPositionAndRotation(new Vector3(320f, 5f, -260f), Quaternion.Euler(0f, 35f, 0f));
                var runtime = host.AddComponent<IslandRuntime>();
                var bridge = FlatBridge();
                var prepared = new TownPreparedData { bridgePlan = new TownBridgePlan { bridges = new[] { bridge } } };
                prepared.prototypes = new[] { new TownPrototype {
                    entry_id = "church", is_church = true, materials = new[] { TownNative.ChurchStone },
                    lods = new[] { new TownLod { parts = new[] { new TownPart { material = "wall", mesh = bridge.masonry } } } },
                } };
                var town = TownRenderer.Install(host.transform, prepared, 1000f);
                runtime.SetTownRenderer(town);

                var levels = town.GetComponentInChildren<LODGroup>().GetLODs();
                Assert.That(levels[0].renderers.Length, Is.EqualTo(1));
                Assert.That(levels[1].renderers.Length, Is.EqualTo(1));
                var surface = levels[0].renderers[0];
                var mesh = surface.GetComponent<MeshFilter>().sharedMesh;
                Assert.That(mesh.subMeshCount, Is.EqualTo(1));
                Assert.That(mesh.vertexCount, Is.EqualTo(12), "The deck, masonry and dressing share a mesh.");
                Assert.That(mesh.uv[0].y, Is.EqualTo(-2f), "The span-aligned UVs use the church wall scale.");
                var stone = surface.sharedMaterial;
                Assert.That(levels[1].renderers[0].sharedMaterial, Is.SameAs(stone));
                Assert.That(town.SharedPrototypes[0].GetComponentInChildren<MeshRenderer>(true).sharedMaterial,
                    Is.SameAs(stone), "The bridge shares the church wall material.");
                Assert.That(stone.shader.name, Is.EqualTo("Motu/TownBuilding"));
                Assert.That(stone.GetVector("_Grub0").x, Is.GreaterThan(0f));
                Assert.That(stone.GetTexture("_BaseMap"), Is.Not.Null);

                var ground = IslandNavigationTests.Ground();
                var material = new Color[ground.vertices.Length];
                for (var i = 0; i < material.Length; i++)
                {
                    var river = Mathf.Abs(ground.vertices[i].x) <= 4f;
                    // Grade the dry bank from the deck landing down to the excluded bed.
                    ground.vertices[i].y = Mathf.Lerp(-2f, 3f, Mathf.InverseLerp(4f, 10f, Mathf.Abs(ground.vertices[i].x)));
                    material[i] = river ? Color.blue : Color.clear;
                }
                ground = new IslandPreparedMesh(ground.vertices, ground.normals, ground.triangles,
                    ground.uv, material, ground.environment);

                var navRoot = new GameObject("Island Navigation");
                navRoot.transform.SetParent(host.transform, false);
                var navigation = navRoot.AddComponent<IslandNavigation>();
                var task = navigation.BuildAsync(ground, null, null, IslandPreparedCaves.Empty,
                    new IslandNavigationSettings { LogBuildStatistics = false }, CancellationToken.None);
                while (!task.IsCompleted) yield return null;
                task.GetAwaiter().GetResult();
                Assert.That(navigation.Report.sourceTriangles, Is.EqualTo(ground.triangles.Length / 3 + 2));
                var filter = new NavMeshQueryFilter { agentTypeID = navigation.AgentTypeId, areaMask = NavMesh.AllAreas };
                Vector3 World(float x, float z) => host.transform.TransformPoint(new Vector3(x, 3f, z));
                Assert.That(NavMesh.SamplePosition(World(0, 8), out _, .5f, filter), Is.True);
                Assert.That(NavMesh.SamplePosition(World(0, 13), out _, .5f, filter), Is.False);
                var path = new NavMeshPath();
                Assert.That(NavMesh.CalculatePath(World(-15, 8), World(15, 8), filter, path), Is.True);
                Assert.That(path.status, Is.EqualTo(NavMeshPathStatus.PathComplete));
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(host);
            }
        }

        private static TownBridge FlatBridge()
        {
            var deck = new TownMesh {
                positions = new[] { 980f, 1012f, 6f, 1020f, 1012f, 6f, 1020f, 1020f, 6f, 980f, 1020f, 6f },
                normals = new[] { 0f, 0f, 1f, 0f, 0f, 1f, 0f, 0f, 1f, 0f, 0f, 1f },
                uvs = new[] { 0f, -4f, 40f, -4f, 40f, 4f, 0f, 4f }, indices = new[] { 0, 1, 2, 0, 2, 3 },
            };
            return new TownBridge {
                id = "bridge", surface = "cart_track", start = new[] { 980f, 1016f, 6f },
                end = new[] { 1020f, 1016f, 6f }, width = 8f, arch_count = 1,
                masonry = deck, dressing = deck, deck = deck, coarse_masonry = deck, coarse_deck = deck, collision = deck,
            };
        }
    }
}
