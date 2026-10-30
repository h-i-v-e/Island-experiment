using Motu.Streaming;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class TerrainAreaPinTests
    {
        [Test]
        public void FootprintUsesAuthoredHullGeometryBeforePhysicsSync()
        {
            var root = new GameObject("Rotated scaled hull pin");
            try
            {
                var shape = root.AddComponent<BoxCollider>(); shape.center = new Vector3(1, 0, -2);
                shape.size = new Vector3(4, 2, 10);
                var pin = root.AddComponent<TerrainAreaPin>(); pin.Configure(new Collider[] { shape });
                root.transform.SetPositionAndRotation(new Vector3(12, 7, 30), Quaternion.Euler(0, 90, 0));
                root.transform.localScale = new Vector3(2, 3, 4);
                Assert.IsTrue(pin.TryGetWorldBounds(out var bounds));
                Assert.That(Vector3.Distance(bounds.center, new Vector3(4, 7, 28)), Is.LessThan(.001f));
                Assert.That(Vector3.Distance(bounds.size, new Vector3(44, 10, 12)), Is.LessThan(.001f));
            }
            finally { Object.DestroyImmediate(root); }
        }

        [Test]
        public void DisabledAndDestroyedOwnersCannotLeaveActivePins()
        {
            var root = new GameObject("Owner lifetime pin");
            try
            {
                var owner = root.AddComponent<Light>();
                var shape = root.AddComponent<BoxCollider>();
                var pin = root.AddComponent<TerrainAreaPin>(); pin.Configure(new Collider[] { shape }, true, owner);
                Assert.IsTrue(pin.TryGetWorldBounds(out _));
                owner.enabled = false; Assert.IsFalse(pin.TryGetWorldBounds(out _));
                owner.enabled = true; Assert.IsTrue(pin.TryGetWorldBounds(out _));
                Object.DestroyImmediate(owner); Assert.IsFalse(pin.TryGetWorldBounds(out _));
            }
            finally { Object.DestroyImmediate(root); }
        }
    }
}
