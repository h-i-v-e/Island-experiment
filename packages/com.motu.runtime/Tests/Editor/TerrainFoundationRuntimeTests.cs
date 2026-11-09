using System.Collections;
using System.Reflection;
using Motu.Islands;
using Motu.Streaming;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class TerrainFoundationRuntimeTests
    {
        [UnityTest]
        public IEnumerator EnabledRuntimeComponentWaitsForTheRealCpuMapThenBuildsOnlyOnce()
        {
            yield return new EnterPlayMode();
            var sourceRoot = new GameObject("Unloaded foundation terrain");
            sourceRoot.SetActive(false);
            var source = sourceRoot.AddComponent<IslandGenerator>();
            var streamer = sourceRoot.AddComponent<TerrainTileStreamer>();
            streamer.worldSize = 64;
            typeof(IslandGenerator).GetField("terrainStreamer", BindingFlags.Instance | BindingFlags.NonPublic)
                .SetValue(source, streamer);
            var root = new GameObject("Automatically placed runtime building");
            try
            {
                var building = GameObject.CreatePrimitive(PrimitiveType.Cube);
                building.transform.SetParent(root.transform, false);
                building.transform.localPosition = Vector3.up * 2;
                building.transform.localScale = new Vector3(6, 4, 5);
                var foundation = root.AddComponent<TerrainFoundation>();
                foundation.TerrainSource = source;
                yield return null;
                yield return null;
                Assert.That(foundation.IsBuilt, Is.False);
                Assert.That(root.transform.position, Is.EqualTo(Vector3.zero));
                Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.WaitingForTerrain));
                streamer.colliderHeightMap = TerrainFootprintTests.MakeMap(64, 2, (x, z) => 12 + x * .2f);
                var deadline = Time.realtimeSinceStartup + 3;
                while (!foundation.IsBuilt && Time.realtimeSinceStartup < deadline) yield return null;
                Assert.That(foundation.IsBuilt, Is.True);
                Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.Ready));
                Assert.That(root.transform.position.y, Is.EqualTo(foundation.GroundSample.MaximumHeight).Within(.001f));
                var platform = foundation.Platform;
                root.SetActive(false); root.SetActive(true);
                yield return null;
                Assert.That(foundation.Platform, Is.SameAs(platform));
                Assert.That(root.GetComponentsInChildren<MeshRenderer>().Length, Is.EqualTo(2));
                Assert.That(source.Streaming.Target, Is.Null, "Foundation placement never installs a streaming target.");
            }
            finally
            {
                Object.DestroyImmediate(root);
                streamer.Dispose(); Object.DestroyImmediate(sourceRoot);
            }
            yield return new ExitPlayMode();
        }
    }
}
