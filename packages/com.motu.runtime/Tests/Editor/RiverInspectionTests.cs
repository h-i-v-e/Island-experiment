using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class RiverInspectionTests
    {
        [Test]
        public void InspectionSurvivesConsumedUploadDataAndFollowsTheTileTransform()
        {
            var owner = new GameObject("River inspection lifecycle");
            owner.SetActive(false);
            try
            {
                var streamer = owner.AddComponent<TerrainTileStreamer>();
                var tiles = new[] { new IslandPreparedMesh(
                    new[] { new Vector3(0, 2, 0), new Vector3(3, 2, 0), new Vector3(0, 2, 3) },
                    new Vector3[3], new[] { 0, 1, 2 }, new Vector2[3], new Color[3], new Vector2[3]) };
                streamer.CacheRiverInspectionData(tiles);
                tiles[0] = null; // Feature upload transfers these arrays to a Unity mesh.
                Assert.That(streamer.GeneratedRiverTileCount, Is.EqualTo(1));
                Assert.That(streamer.TryGetRiverInspectionPoint(out var first), Is.True);
                Assert.That(Vector3.Distance(first, new Vector3(1, 2.025f, 1)), Is.LessThan(.0001f));
                owner.transform.SetPositionAndRotation(new Vector3(90, 10, -40), Quaternion.Euler(0, 90, 0));
                Assert.That(streamer.TryGetRiverInspectionPoint(out var moved), Is.True);
                Assert.That(Vector3.Distance(moved, owner.transform.TransformPoint(first)), Is.LessThan(.0001f));
                streamer.CacheRiverInspectionData(null);
                Assert.That(streamer.GeneratedRiverTileCount, Is.Zero);
                Assert.That(streamer.TryGetRiverInspectionPoint(out _), Is.False);
            }
            finally { Object.DestroyImmediate(owner); }
        }
    }
}
