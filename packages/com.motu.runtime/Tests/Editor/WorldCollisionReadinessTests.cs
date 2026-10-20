using System;
using Motu.Islands;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WorldCollisionReadinessTests
    {
        private sealed class IslandSource : IIslandGenerationRequestFactory
        {
            public IslandGenerationRequest CreateIslandGenerationRequest(Vector2Int cell) =>
                throw new InvalidOperationException("Readiness must never generate or stream terrain.");
            public bool HasIsland(Vector2Int cell) => cell != Vector2Int.zero;
        }

        [Test]
        public void KnownOpenSeaIsReadyWithoutAResidentIslandOrCollider()
        {
            var root = new GameObject("Open sea readiness");
            root.SetActive(false);
            try
            {
                var world = root.AddComponent<IslandWorldManager>();
                world.IslandGenerationRequestFactory = new IslandSource();
                Assert.That(world.HasActiveTerrainColliderAt(Vector3.zero), Is.False);
                Assert.That(world.IsTerrainCollisionReadyAt(Vector3.zero), Is.True);
                Assert.That(world.KnownIslandCount, Is.Zero);
                Assert.That(world.LoadedIslandCount, Is.Zero);
            }
            finally { Object.DestroyImmediate(root); }
        }

        [Test]
        public void MissingIslandCollisionAndUnknownWorldRemainUnsafe()
        {
            var root = new GameObject("Unloaded island readiness");
            root.SetActive(false);
            try
            {
                var world = root.AddComponent<IslandWorldManager>();
                Assert.That(world.IsTerrainCollisionReadyAt(Vector3.zero), Is.False);
                world.IslandGenerationRequestFactory = new IslandSource();
                Assert.That(world.IsTerrainCollisionReadyAt(new Vector3(1001, 0, 0)), Is.False);
                Assert.That(world.IsTerrainCollisionReadyAt(new Vector3(-1001, 0, 0)), Is.False);
                Assert.That(world.KnownIslandCount, Is.Zero);
            }
            finally { Object.DestroyImmediate(root); }
        }
    }
}
