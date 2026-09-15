using System;
using System.Collections;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.Navigation;
using Motu.Settings;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.AI;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class IslandNavigationTests
    {
        internal static IslandPreparedMesh Ground(int cells = 40, float size = 40f)
        {
            var vertices = new Vector3[(cells + 1) * (cells + 1)];
            var indices = new int[cells * cells * 6];
            var offset = 0;
            for (var z = 0; z <= cells; z++)
                for (var x = 0; x <= cells; x++)
                {
                    var px = x * size / cells - size * .5f;
                    vertices[z * (cells + 1) + x] = new Vector3(px, 2f + Mathf.Sin(px * .3f) * .7f, z * size / cells - size * .5f);
                    if (x == cells || z == cells) continue;
                    var a = z * (cells + 1) + x;
                    indices[offset++] = a; indices[offset++] = a + cells + 1; indices[offset++] = a + 1;
                    indices[offset++] = a + 1; indices[offset++] = a + cells + 1; indices[offset++] = a + cells + 2;
                }
            return new IslandPreparedMesh(vertices, Array.Empty<Vector3>(), indices,
                Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
        }

        [Test]
        public void AgentDimensionsDefaultToHalfMetreAndProfilesCopyThem()
        {
            var settings = new IslandNavigationSettings();
            var type = NavMesh.CreateSettings().agentTypeID;
            try
            {
                Assert.AreEqual(.5f, settings.AgentRadius);
                var copied = settings.Copy();
                settings.AgentRadius = .75f;
                var build = settings.BuildSettings(type);
                Assert.AreEqual(.75f, build.agentRadius);
                Assert.AreEqual(.25f, build.voxelSize);
                Assert.IsTrue(build.buildHeightMesh);
                Assert.AreEqual(.5f, copied.AgentRadius);
                Assert.AreEqual(128f, copied.ChunkSize);
                Assert.AreEqual(1u, build.maxJobWorkers);
                settings.ChunkSize = 0;
                Assert.Throws<ArgumentException>(() => settings.BuildSettings(type));
                settings.ChunkSize = 64;
                settings.MaximumBuildWorkers = 0;
                Assert.Throws<ArgumentException>(() => settings.BuildSettings(type));
                settings.MaximumBuildWorkers = 1;
                settings.AgentHeight = .5f;
                Assert.Throws<ArgumentException>(() => settings.BuildSettings(type));
            }
            finally { NavMesh.RemoveSettings(type); }
        }

        [UnityTest]
        public IEnumerator FullDetailBuildFindsPathsAroundBouldersAndRetainsHeight()
        {
            var host = new GameObject("Navigation test");
            var navigation = host.AddComponent<IslandNavigation>();
            var agentRoot = new GameObject("Agent");
            agentRoot.SetActive(false);
            try
            {
                var task = navigation.BuildAsync(Ground(), null,
                    new[] { new[] { new IslandPreparedBoulderCollider(new Vector3(0, 2, 0), 3) } },
                    IslandPreparedCaves.Empty, new IslandNavigationSettings { LogBuildStatistics = false }, CancellationToken.None);
                while (!task.IsCompleted) yield return null;
                task.GetAwaiter().GetResult();
                Assert.IsTrue(navigation.IsReady);
                Assert.IsTrue(navigation.IsRegistered);
                Assert.AreEqual(3200, navigation.Report.sourceTriangles);
                Assert.Greater(navigation.ChunkCount, 1);
                Assert.AreEqual(navigation.Report.chunkCount, navigation.Report.completedChunks);
                Assert.Less(navigation.Report.peakChunkTriangles, navigation.Report.sourceTriangles);
                var filter = new NavMeshQueryFilter { agentTypeID = navigation.AgentTypeId, areaMask = NavMesh.AllAreas };
                Assert.IsTrue(NavMesh.SamplePosition(new Vector3(-10, 2, 0), out var from, 2, filter));
                Assert.IsTrue(NavMesh.SamplePosition(new Vector3(10, 2, 0), out var to, 2, filter));
                var path = new NavMeshPath();
                Assert.IsTrue(NavMesh.CalculatePath(from.position, to.position, filter, path));
                Assert.AreEqual(NavMeshPathStatus.PathComplete, path.status);
                Assert.Greater(path.corners.Length, 2);
                Assert.IsFalse(NavMesh.SamplePosition(new Vector3(0, 2, 0), out _, .3f, filter));
                for (var x = -15; x <= 15; x++)
                {
                    var expected = 2f + Mathf.Sin(x * .3f) * .7f;
                    Assert.IsTrue(NavMesh.SamplePosition(new Vector3(x, expected, 10), out var hit, .5f, filter));
                    Assert.Less(Mathf.Abs(hit.position.y - expected), .2f);
                }
                var agent = agentRoot.AddComponent<NavMeshAgent>();
                navigation.ConfigureAgent(agent);
                Assert.AreEqual(.5f, agent.radius);
                Assert.AreEqual(navigation.AgentTypeId, agent.agentTypeID);
                navigation.SetRegistered(false);
                yield return null;
                Assert.IsFalse(NavMesh.SamplePosition(from.position, out _, 1, filter));
                navigation.SetRegistered(true);
                yield return null;
                Assert.IsTrue(NavMesh.SamplePosition(from.position, out _, 1, filter));
                host.SetActive(false);
                Assert.IsFalse(navigation.IsRegistered);
                host.SetActive(true);
                Assert.IsTrue(navigation.IsRegistered);
                navigation.Dispose();
                yield return null;
                Assert.IsFalse(NavMesh.SamplePosition(from.position, out _, 1, filter));
            }
            finally { Object.DestroyImmediate(agentRoot); Object.DestroyImmediate(host); }
        }

        [UnityTest]
        public IEnumerator CancellationReleasesBuildSourcesAndAgentType()
        {
            var host = new GameObject("Cancelled navigation");
            var navigation = host.AddComponent<IslandNavigation>();
            using var cancellation = new CancellationTokenSource();
            var count = NavMesh.GetSettingsCount();
            try
            {
                var task = navigation.BuildAsync(Ground(), null, null, IslandPreparedCaves.Empty,
                    new IslandNavigationSettings { LogBuildStatistics = false }, cancellation.Token);
                cancellation.Cancel();
                while (!task.IsCompleted) yield return null;
                Assert.IsTrue(task.IsCanceled);
                Assert.IsFalse(navigation.IsRegistered);
                Assert.IsNull(navigation.Data);
                Assert.AreEqual(count, NavMesh.GetSettingsCount());
            }
            finally { Object.DestroyImmediate(host); }
        }

        [UnityTest]
        public IEnumerator SharedAgentTypesAndDisposalDuringBuildAreSafe()
        {
            var first = new GameObject("First navigation").AddComponent<IslandNavigation>();
            var second = new GameObject("Second navigation").AddComponent<IslandNavigation>();
            var cancelled = new GameObject("Disposed build").AddComponent<IslandNavigation>();
            var count = NavMesh.GetSettingsCount();
            try
            {
                var config = new IslandNavigationSettings { AgentRadius = .75f, LogBuildStatistics = false };
                var a = first.BuildAsync(Ground(), null, null, IslandPreparedCaves.Empty, config, CancellationToken.None);
                var b = second.BuildAsync(Ground(), null, null, IslandPreparedCaves.Empty, config, CancellationToken.None);
                while (!a.IsCompleted || !b.IsCompleted) yield return null;
                a.GetAwaiter().GetResult(); b.GetAwaiter().GetResult();
                Assert.AreEqual(first.AgentTypeId, second.AgentTypeId);
                Assert.AreEqual(count + 1, NavMesh.GetSettingsCount());
                first.Dispose();
                Assert.IsTrue(second.IsRegistered);
                Assert.AreEqual(count + 1, NavMesh.GetSettingsCount());
                second.Dispose();
                Assert.AreEqual(count, NavMesh.GetSettingsCount());
                var task = cancelled.BuildAsync(Ground(128, 512), null, null, IslandPreparedCaves.Empty,
                    config, CancellationToken.None);
                // Let the upload continuation start the native async operation.
                yield return null;
                cancelled.Dispose();
                while (!task.IsCompleted) yield return null;
                if (task.IsFaulted) task.GetAwaiter().GetResult();
                Assert.IsFalse(cancelled.IsReady);
                Assert.IsFalse(cancelled.IsRegistered);
                Assert.IsNull(cancelled.Data);
                Assert.AreEqual(count, NavMesh.GetSettingsCount());
            }
            finally
            {
                Object.DestroyImmediate(first.gameObject);
                Object.DestroyImmediate(second.gameObject);
                Object.DestroyImmediate(cancelled.gameObject);
            }
        }

        [Test]
        public void FactoryNavigationSettingsAreCapturedByRequestProfile()
        {
            var host = new GameObject("Factory navigation settings");
            try
            {
                var factory = host.AddComponent<GridIslandGenerationRequestFactory>();
                factory.Configure(42, true, 1f);
                factory.GenerationSettings.UseSnapshotCache = false;
                factory.NavigationSettings.AgentRadius = .8f;
                var request = factory.CreateIslandGenerationRequest(Vector2Int.zero);
                Assert.AreEqual(.8f, request.Navigation.AgentRadius);
                factory.NavigationSettings.AgentRadius = .6f;
                Assert.AreEqual(.8f, request.Navigation.AgentRadius);
                Assert.AreEqual(.8f, request.Profile.Clone().Navigation.AgentRadius);
            }
            finally { Object.DestroyImmediate(host); }
        }

        [UnityTest]
        public IEnumerator BackgroundBuildDoesNotRegisterDormantOrUnloadedIslands()
        {
            var host = new GameObject("Background navigation");
            host.SetActive(false);
            var navigation = host.AddComponent<IslandNavigation>();
            try
            {
                navigation.SetRegistered(false);
                navigation.StartBuild(Ground(), null, null, IslandPreparedCaves.Empty,
                    new IslandNavigationSettings { LogBuildStatistics = false });
                host.SetActive(true);
                while (!navigation.BuildCompletion.IsCompleted) yield return null;
                navigation.BuildCompletion.GetAwaiter().GetResult();
                Assert.IsTrue(navigation.IsReady);
                Assert.IsFalse(navigation.IsRegistered);
                navigation.SetRegistered(true);
                Assert.IsTrue(navigation.IsRegistered);
            }
            finally { Object.DestroyImmediate(host); }
            var unloaded = new GameObject("Unloaded before navigation starts");
            var cancelled = unloaded.AddComponent<IslandNavigation>();
            cancelled.StartBuild(Ground(), null, null, IslandPreparedCaves.Empty, new IslandNavigationSettings());
            var completion = cancelled.BuildCompletion;
            Object.DestroyImmediate(unloaded);
            while (!completion.IsCompleted) yield return null;
            completion.GetAwaiter().GetResult();
            Assert.IsFalse(cancelled.IsReady);
            Assert.IsFalse(cancelled.IsRegistered);
        }

        [UnityTest]
        public IEnumerator MovingAgentUsesHeightMeshForGroundContact()
        {
            yield return new EnterPlayMode();
            var host = new GameObject("Moving agent navigation");
            var navigation = host.AddComponent<IslandNavigation>();
            var body = new GameObject("Moving agent");
            body.SetActive(false);
            try
            {
                var task = navigation.BuildAsync(Ground(), null, null, IslandPreparedCaves.Empty,
                    new IslandNavigationSettings { LogBuildStatistics = false }, CancellationToken.None);
                while (!task.IsCompleted) yield return null;
                task.GetAwaiter().GetResult();
                var agent = body.AddComponent<NavMeshAgent>();
                navigation.ConfigureAgent(agent);
                body.transform.position = new Vector3(-10, 2f + Mathf.Sin(-3f) * .7f, 10);
                body.SetActive(true);
                agent.speed = 20; agent.acceleration = 1000;
                Assert.IsTrue(agent.SetDestination(new Vector3(10, 2, 10)));
                var start = Time.realtimeSinceStartup;
                var maximum = 0f;
                var samples = 0;
                while (Time.realtimeSinceStartup - start < 5f)
                {
                    yield return null;
                    var position = body.transform.position;
                    if (agent.pathPending) continue;
                    var expected = 2f + Mathf.Sin(position.x * .3f) * .7f;
                    maximum = Mathf.Max(maximum, Mathf.Abs(position.y - expected));
                    samples++;
                    if (position.x >= 9.8f) break;
                }
                Debug.Log($"Moving NavMeshAgent maximum ground height error: {maximum:F4} metres across {samples} frames.");
                Assert.Greater(body.transform.position.x, 9f);
                Assert.Greater(samples, 2);
                Assert.Less(maximum, .2f);
            }
            finally { Object.DestroyImmediate(body); Object.DestroyImmediate(host); }
            yield return new ExitPlayMode();
        }

        [UnityTest]
        public IEnumerator ChunkLinksDoNotBridgeRiverBedsOrCliffs()
        {
            for (var barrier = 0; barrier < 2; barrier++)
            {
                var host = new GameObject("Chunk barrier test");
                host.transform.SetPositionAndRotation(new Vector3(300, 5, -250), Quaternion.Euler(0, 35, 0));
                var navigation = host.AddComponent<IslandNavigation>();
                try
                {
                    var ground = Ground();
                    var material = new Color[ground.vertices.Length];
                    for (var i = 0; i < ground.vertices.Length; i++)
                    {
                        if (barrier == 0 && Mathf.Abs(ground.vertices[i].x) <= 1.1f) material[i] = Color.blue;
                        if (barrier == 1 && ground.vertices[i].x >= 0) ground.vertices[i].y += 10;
                    }
                    ground = new IslandPreparedMesh(ground.vertices, ground.normals, ground.triangles,
                        ground.uv, material, ground.environment);
                    var task = navigation.BuildAsync(ground, null, null, IslandPreparedCaves.Empty,
                        new IslandNavigationSettings { LogBuildStatistics = false }, CancellationToken.None);
                    while (!task.IsCompleted) yield return null;
                    task.GetAwaiter().GetResult();
                    var filter = new NavMeshQueryFilter { agentTypeID = navigation.AgentTypeId, areaMask = NavMesh.AllAreas };
                    Assert.IsTrue(NavMesh.SamplePosition(host.transform.TransformPoint(new Vector3(-10, 2, 0)), out var a, 2, filter));
                    Assert.IsTrue(NavMesh.SamplePosition(host.transform.TransformPoint(new Vector3(10, barrier == 0 ? 2 : 12, 0)), out var b, 2, filter));
                    var path = new NavMeshPath();
                    NavMesh.CalculatePath(a.position, b.position, filter, path);
                    Assert.AreNotEqual(NavMeshPathStatus.PathComplete, path.status, "A chunk connection bypassed an impassable barrier.");
                    Assert.Greater(navigation.Report.seamLinks, 0, "Other walkable seams should still connect.");
                }
                finally { Object.DestroyImmediate(host); }
            }
        }

        [UnityTest]
        public IEnumerator StackedSurfacesConnectAcrossSeamsWithoutJoiningFloors()
        {
            var host = new GameObject("Stacked navigation");
            var navigation = host.AddComponent<IslandNavigation>();
            try
            {
                var upper = Ground();
                for (var i = 0; i < upper.vertices.Length; i++) upper.vertices[i].y += 5f;
                var caves = new IslandPreparedCaves(new[] { new IslandPreparedCave(default,
                    new[] { upper }, 40f) }, default);
                var task = navigation.BuildAsync(Ground(), null, null, caves,
                    new IslandNavigationSettings { LogBuildStatistics = false }, CancellationToken.None);
                while (!task.IsCompleted) yield return null;
                task.GetAwaiter().GetResult();
                var filter = new NavMeshQueryFilter { agentTypeID = navigation.AgentTypeId, areaMask = NavMesh.AllAreas };
                var path = new NavMeshPath();
                var starts = new Vector3[2];
                for (var floor = 0; floor < 2; floor++)
                {
                    Assert.IsTrue(NavMesh.SamplePosition(new Vector3(-10, 2 + floor * 5, 10), out var a, 1, filter));
                    Assert.IsTrue(NavMesh.SamplePosition(new Vector3(10, 2 + floor * 5, 10), out var b, 1, filter));
                    starts[floor] = a.position;
                    NavMesh.CalculatePath(a.position, b.position, filter, path);
                    Assert.AreEqual(NavMeshPathStatus.PathComplete, path.status);
                }
                NavMesh.CalculatePath(starts[0], starts[1], filter, path);
                Assert.AreNotEqual(NavMeshPathStatus.PathComplete, path.status, "Border links connected separate cave levels.");
            }
            finally { Object.DestroyImmediate(host); }
        }

        [UnityTest]
        public IEnumerator CancellingBetweenChunksReleasesEveryCompletedChunk()
        {
            var count = NavMesh.GetSettingsCount();
            var host = new GameObject("Cancel partial navigation");
            var navigation = host.AddComponent<IslandNavigation>();
            using var cancellation = new CancellationTokenSource();
            try
            {
                var task = navigation.BuildAsync(Ground(100, 100), null, null, IslandPreparedCaves.Empty,
                    new IslandNavigationSettings { ChunkSize = 32, LogBuildStatistics = false }, cancellation.Token);
                while (navigation.Report.completedChunks == 0 && !task.IsCompleted) yield return null;
                Assert.Greater(navigation.Report.completedChunks, 0);
                Assert.Less(navigation.Report.completedChunks, navigation.Report.chunkCount);
                cancellation.Cancel();
                while (!task.IsCompleted) yield return null;
                Assert.IsTrue(task.IsCanceled);
                Assert.IsFalse(navigation.IsReady);
                Assert.IsFalse(navigation.IsRegistered);
                Assert.AreEqual(0, navigation.ChunkCount);
                Assert.AreEqual(count, NavMesh.GetSettingsCount());
            }
            finally { Object.DestroyImmediate(host); }
        }

        [Test]
        public void ObstaclesAndRiverAreasDoNotDependOnVisibleLods()
        {
            using var sources = new IslandNavigationSources();
            var ground = Ground(2, 4);
            var material = new Color[ground.vertices.Length];
            Array.Fill(material, Color.blue);
            sources.AddMesh(new IslandPreparedMesh(ground.vertices, ground.normals, ground.triangles,
                ground.uv, material, ground.environment), true);
            Assert.AreEqual(8, sources.triangleCount);
            Assert.AreEqual(1, sources.sources[0].area);
            sources.AddWood(new[] { new[] { new IslandPreparedTreeCollider(Vector3.zero, Vector3.up * 5, .3f) } }, false);
            sources.AddWood(new[] { new[] { new IslandPreparedTreeCollider(Vector3.zero, Vector3.right * 5, .3f) } }, true);
            sources.AddBoulders(new[] { new[] { new IslandPreparedBoulderCollider(Vector3.zero, 2) } });
            Assert.AreEqual(NavMeshBuildSourceShape.ModifierBox, sources.sources[1].shape);
            Assert.AreEqual(NavMeshBuildSourceShape.ModifierBox, sources.sources[2].shape);
            Assert.AreEqual(NavMeshBuildSourceShape.ModifierBox, sources.sources[3].shape);
            Assert.AreEqual(new Vector3(.6f, 5, .6f), sources.sources[2].size);
            sources.ExcludeSubmergedGround(2, .5f);
            Assert.AreEqual(NavMeshBuildSourceShape.ModifierBox, sources.sources[4].shape);
        }
    }
}
