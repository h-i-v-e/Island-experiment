using System;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ReedStreamingLifecycleTests
    {
        [Test]
        public void PlayerBuildRetainsSharedClumpInstancingVariant()
        {
            var material = Resources.Load<Material>("Motu/Variants/ReedClumps");
            Assert.IsNotNull(material);
            Assert.AreEqual("Motu/Riverbank Reeds", material.shader.name);
            Assert.IsTrue(material.enableInstancing);
            Assert.IsTrue(material.IsKeywordEnabled("MOTU_REED_CLUMPS"));
        }

        [Test]
        public void IslandsShareOneClumpMeshUntilTheFinalStreamerIsDisposed()
        {
            using var first = new ReedFixture();
            using var second = new ReedFixture();
            var mesh = first.Streamer.SharedMesh;
            Assert.AreSame(mesh, second.Streamer.SharedMesh);
            Assert.AreEqual(32, mesh.vertexCount);
            Assert.AreEqual(72, mesh.GetIndexCount(0));
            Assert.AreEqual(0, first.Island.GetComponentsInChildren<MeshRenderer>(true).Length);
            Assert.AreEqual(1, first.Island.transform.childCount,
                "An island owns one visibility root, without tile or clump GameObjects.");

            first.Streamer.Dispose();
            Assert.IsTrue(mesh != null, "The second island still leases the shared mesh.");
            second.Streamer.UpdateLod0Neighborhood(new Vector2Int(32, 32));
            Assert.AreEqual(131, second.Streamer.ActiveInstanceCount);
            second.Streamer.Dispose();
            Assert.IsTrue(mesh == null, "The final lease releases the shared Unity mesh.");
            Assert.DoesNotThrow(first.Streamer.Dispose);
            Assert.DoesNotThrow(second.Streamer.Dispose);
            Assert.AreEqual(0, first.Island.transform.childCount);
            Assert.AreEqual(0, second.Island.transform.childCount);
        }

        [Test]
        public void NeighborhoodsSplitLargeTilesRetireAndRefocusWithoutRetainingBatches()
        {
            using var scene = new ReedFixture();
            scene.Streamer.UpdateLod0Neighborhood(new Vector2Int(32, 32));
            Assert.AreEqual(2, scene.Streamer.ActiveTileCount);
            Assert.AreEqual(131, scene.Streamer.ActiveInstanceCount);
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(3, scene.Streamer.SubmittedBatchCount,
                "The 129-clump tile requires two batches; its neighbor requires one.");

            scene.Streamer.UpdateLod0Neighborhood(new Vector2Int(30, 32));
            Assert.AreEqual(1, scene.Streamer.ActiveTileCount);
            Assert.AreEqual(1, scene.Streamer.ActiveInstanceCount);
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(1, scene.Streamer.SubmittedBatchCount);

            scene.Streamer.ClearPlayerFocus();
            Assert.AreEqual(0, scene.Streamer.ActiveTileCount);
            Assert.AreEqual(0, scene.Streamer.ActiveInstanceCount);
            Assert.AreEqual(0, scene.Streamer.SubmittedBatchCount);
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(0, scene.Streamer.SubmittedBatchCount);

            scene.Streamer.UpdateLod0Neighborhood(new Vector2Int(32, 32));
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(2, scene.Streamer.ActiveTileCount);
            Assert.AreEqual(131, scene.Streamer.ActiveInstanceCount);
            Assert.AreEqual(3, scene.Streamer.SubmittedBatchCount);
            Assert.AreEqual(1, scene.Island.transform.childCount);
        }

        [Test]
        public void VisibilityAndCameraLayersGateSubmissionAndDisposedStreamersStayIdle()
        {
            using var scene = new ReedFixture();
            scene.Streamer.UpdateLod0Neighborhood(new Vector2Int(32, 32));
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(3, scene.Streamer.SubmittedBatchCount);

            scene.Streamer.SetVisible(false);
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(0, scene.Streamer.SubmittedBatchCount);
            Assert.AreEqual(131, scene.Streamer.ActiveInstanceCount,
                "Visibility changes retain the nearby placements for a later reveal.");
            scene.Streamer.SetVisible(true);
            scene.Camera.cullingMask = 1 << 29;
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(0, scene.Streamer.SubmittedBatchCount);
            scene.Camera.cullingMask = 1 << 30;
            scene.Streamer.Submit(scene.Camera);
            Assert.AreEqual(3, scene.Streamer.SubmittedBatchCount);

            scene.Streamer.Dispose();
            Assert.DoesNotThrow(() => scene.Streamer.Submit(scene.Camera));
            Assert.DoesNotThrow(scene.Streamer.Dispose);
            Assert.AreEqual(0, scene.Streamer.SubmittedBatchCount);
            Assert.AreEqual(0, scene.Streamer.ActiveTileCount);
            Assert.AreEqual(0, scene.Streamer.ActiveInstanceCount);
            Assert.AreEqual(0, scene.Streamer.SharedVertexCount);
        }

        private sealed class ReedFixture : IDisposable
        {
            internal readonly GameObject Island = new GameObject("Reed streaming lifecycle island") { layer = 30 };
            internal readonly ReedTileStreamer Streamer = new ReedTileStreamer();
            internal readonly Camera Camera;
            private readonly Material material;

            internal ReedFixture()
            {
                material = new Material(Shader.Find("Motu/Riverbank Reeds"));
                var tiles = new IslandPreparedReedTile[ReedTileStreamer.TileCount];
                tiles[32 * ReedTileStreamer.Resolution + 32] = Tile(129);
                tiles[32 * ReedTileStreamer.Resolution + 33] = Tile(2);
                tiles[32 * ReedTileStreamer.Resolution + 30] = Tile(1);
                Streamer.Initialize(Island.transform, material, tiles, true);
                Camera = new GameObject("Reed streaming lifecycle camera").AddComponent<Camera>();
                Camera.enabled = false;
                Camera.cullingMask = 1 << 30;
                Camera.transform.position = new Vector3(0, 1, -5);
            }

            public void Dispose()
            {
                Streamer.Dispose();
                Object.DestroyImmediate(Camera.gameObject);
                Object.DestroyImmediate(Island);
                Object.DestroyImmediate(material);
            }

            private static IslandPreparedReedTile Tile(int count)
            {
                var placements = new IslandPreparedReedPlacement[count];
                for (var i = 0; i < count; ++i)
                    placements[i] = new IslandPreparedReedPlacement(
                        new Vector3(i % 16 * .4f, 0, 5 + i / 16 * .4f), Vector3.up,
                        1.8f, .6f, new Vector4(i % 2, .5f, .8f, i * .01f));
                return new IslandPreparedReedTile(placements);
            }
        }
    }
}
