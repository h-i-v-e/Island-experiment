using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class TerrainCollisionReadinessTests
    {
        [Test]
        public void ReadinessTracksColliderAndParentActivationWithoutStreaming()
        {
            var root=new GameObject("Terrain readiness fixture");
            var tileObject=new GameObject("Collision tile");tileObject.transform.SetParent(root.transform);
            try
            {
                var streamer=root.AddComponent<TerrainTileStreamer>();streamer.worldSize=2000;
                var collider=tileObject.AddComponent<TerrainCollider>();
                var key=new Vector2Int(32,32);
                streamer.colliderTiles.Add(key,new TerrainTileStreamer.ColliderTile(tileObject,null,collider));
                Assert.IsTrue(streamer.HasActiveColliderAt(Vector3.zero));
                collider.enabled=false;Assert.IsFalse(streamer.HasActiveColliderAt(Vector3.zero));
                collider.enabled=true;root.SetActive(false);Assert.IsFalse(streamer.HasActiveColliderAt(Vector3.zero));
                root.SetActive(true);Assert.IsTrue(streamer.HasActiveColliderAt(Vector3.zero));
                Assert.IsFalse(streamer.HasActiveColliderAt(new Vector3(50,0,0)));
                Assert.IsFalse(streamer.HasActiveColliderAt(new Vector3(1001,0,0)));
                Assert.That(streamer.colliderTiles.Count,Is.EqualTo(1));
                Assert.That(streamer.requestedLod1,Is.EqualTo(new Vector2Int(-1,-1)));
                Object.DestroyImmediate(tileObject);
                Assert.IsFalse(streamer.HasActiveColliderAt(Vector3.zero));
                streamer.colliderTiles.Clear();
            }
            finally { Object.DestroyImmediate(root); }
        }

        [Test]
        public void ReadinessUsesIslandLocalCoordinates()
        {
            var root=new GameObject("Translated terrain readiness");
            try
            {
                root.transform.SetPositionAndRotation(new Vector3(4000,10,-6000),Quaternion.Euler(0,90,0));
                var streamer=root.AddComponent<TerrainTileStreamer>();streamer.worldSize=2000;
                var collider=root.AddComponent<TerrainCollider>();
                streamer.colliderTiles.Add(new Vector2Int(32,32),new TerrainTileStreamer.ColliderTile(root,null,collider));
                Assert.IsTrue(streamer.HasActiveColliderAt(root.transform.TransformPoint(new Vector3(10,200,10))));
                Assert.IsFalse(streamer.HasActiveColliderAt(Vector3.zero));
                streamer.colliderTiles.Clear();
            }
            finally { Object.DestroyImmediate(root); }
        }
    }
}
