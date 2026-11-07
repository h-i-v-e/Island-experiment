using Motu.Islands;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;
using System.Reflection;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalStripLodTests
    {
        [Test]
        public void MixedNearAndDistantCoastResidencyDoesNotMoveTheSharedSeam()
        {
            var parent = new GameObject("Coastal mixed LOD test");
            IslandRuntime runtime = null;
            try
            {
                runtime = IslandRuntime.Create(IslandDescriptor.Request(42, Vector2Int.zero, null), parent.transform);
                runtime.transform.position = Vector3.up * 12f;
                runtime.InstallLod3(Quad(), 400f, IslandTerrainShape.CoastalStrip);
                var farRoot = new GameObject("Adjacent disk-backed horizon");
                farRoot.transform.SetParent(parent.transform, false);
                farRoot.transform.position = new Vector3(2000f, 12f, 0f);
                var horizon = farRoot.AddComponent<IslandHorizonRenderer>();
                horizon.Initialize(Quad(), 400f, IslandTerrainShape.CoastalStrip);
                var nearMesh = runtime.GetComponentInChildren<MeshFilter>(true).transform;
                var farMesh = horizon.GetComponentInChildren<MeshFilter>(true).transform;
                foreach (var viewerX in new[] { 0f, 1500f, 3500f, -5000f })
                {
                    var viewer = new Vector3(viewerX, 12f, 0f);
                    runtime.SetViewPosition(viewer);
                    horizon.SetViewPosition(viewer, true, IslandRuntime.DefaultLod3DistanceMetres);
                    Assert.That(nearMesh.position.y, Is.EqualTo(12f).Within(.0001f));
                    Assert.That(farMesh.position.y, Is.EqualTo(12f).Within(.0001f));
                    Assert.That(nearMesh.TransformPoint(new Vector3(1000f, 8f, 0f)),
                        Is.EqualTo(farMesh.TransformPoint(new Vector3(-1000f, 8f, 0f))));
                }
            }
            finally
            {
                runtime?.Dispose();
                Object.DestroyImmediate(parent);
            }
        }

        [Test]
        public void OriginalIslandHorizonsRetainTheirDistanceSubmersion()
        {
            var host = new GameObject("Island horizon default regression");
            try
            {
                var horizon = host.AddComponent<IslandHorizonRenderer>();
                horizon.Initialize(Quad(), 400f);
                horizon.SetViewPosition(Vector3.right * 2500f, true, 1000f);
                Assert.That(horizon.GetComponentInChildren<MeshFilter>(true).transform.localPosition.y,
                    Is.EqualTo(-200f).Within(.001f));
            }
            finally { Object.DestroyImmediate(host); }
        }

        [Test]
        public void HighBackCornersRetainDetailedTerrainAndCollisionUntilLeavingTheCoastFootprint()
        {
            var parent = new GameObject("Coastal corner collision test");
            IslandRuntime runtime = null;
            try
            {
                runtime = IslandRuntime.Create(IslandDescriptor.Request(42, Vector2Int.zero, null), parent.transform);
                runtime.InstallLod3(Quad(), 400f, IslandTerrainShape.CoastalStrip);
                var terrain = new GameObject("Installed terrain collider fixture");
                terrain.transform.SetParent(runtime.transform, false);
                runtime.SetTerrainStreamer(terrain.AddComponent<TerrainTileStreamer>());
                var support = terrain.AddComponent<BoxCollider>();
                support.center = new Vector3(980f, 440f, 980f);
                support.size = new Vector3(20f, 2f, 20f);
                // Native preparation is exercised separately; this fixture isolates
                // the active runtime's LOD/collider residency decision.
                typeof(IslandRuntime).GetProperty("State", BindingFlags.Instance | BindingFlags.NonPublic)
                    .SetValue(runtime, IslandRuntimeState.Active);
                runtime.gameObject.SetActive(true);
                var viewer = new Vector3(980f, 442f, 980f);
                Assert.That(viewer.magnitude, Is.GreaterThan(IslandRuntime.DefaultLod3DistanceMetres));
                runtime.SetViewPosition(viewer);
                AssertSupport(support, true);
                Assert.That(runtime.IsLod3Visible, Is.False);
                runtime.SetViewPosition(new Vector3(3500f, 442f, 980f));
                AssertSupport(support, false);
                Assert.That(runtime.IsLod3Visible, Is.True);
                runtime.SetViewPosition(viewer);
                AssertSupport(support, true);
                Assert.That(runtime.IsLod3Visible, Is.False);
            }
            finally
            {
                runtime?.Dispose();
                Object.DestroyImmediate(parent);
            }
        }

        private static void AssertSupport(BoxCollider support, bool expected)
        {
            Physics.SyncTransforms();
            var ray = new Ray(new Vector3(980f, 460f, 980f), Vector3.down);
            Assert.That(support.gameObject.activeInHierarchy, Is.EqualTo(expected));
            var hitSupport = Physics.Raycast(ray, out var hit, 30f) && hit.collider == support;
            Assert.That(hitSupport, Is.EqualTo(expected));
        }

        [Test]
        public void CoastalResidencyContainsExtremeHighBackFacesButStillUnloadsFarAerialTravel()
        {
            var generation = new IslandGenerationSettings
            {
                TerrainShape = IslandTerrainShape.CoastalStrip,
                MaximumHeightMetres = 1000f,
                CoastalSeamRidgeHeightMetres = 1000f,
                ContinentalNoiseStrength = 0f,
                DetailNoiseStrength = 0f,
            };
            Assert.That(IslandWorldManager.CoastalDetailDistance(generation, new Vector3(1000f, 2000f, 1000f)), Is.Zero,
                "Standing on the maximum-height seam ridge must retain the detail runtime.");
            Assert.That(IslandWorldManager.CoastalDetailDistance(generation, new Vector3(1100f, 2000f, 1000f)),
                Is.EqualTo(100f).Within(.001f));
            Assert.That(IslandWorldManager.CoastalDetailDistance(generation, new Vector3(0f, 6000f, 0f)),
                Is.GreaterThan(1800f), "A distant aerial viewer must still release detailed coast runtimes.");
        }

        [Test]
        public void StrongFineNoiseRetainsDeepBayTerrainWithoutDisablingVerticalUnloading()
        {
            var generation = new IslandGenerationSettings
            {
                TerrainShape = IslandTerrainShape.CoastalStrip,
                MaximumHeightMetres = 1000f,
                ContinentalNoiseStrength = 0f,
                DetailNoiseStrength = 4f,
                LandMassOffset = -2f,
                SeabedDepthDistanceRatio = 1f,
            };
            Assert.That(IslandWorldManager.CoastalDetailDistance(generation, new Vector3(980f, -6000f, 980f)), Is.Zero,
                "The stronger fine field and final seabed drop must fit inside coastal residency bounds.");
            Assert.That(IslandWorldManager.CoastalDetailDistance(generation, new Vector3(980f, -10000f, 980f)),
                Is.GreaterThan(1800f), "A viewer far beneath the bay must still release detailed terrain.");
        }

        private static IslandPreparedMesh Quad() => new IslandPreparedMesh(
            new[] { new Vector3(-1000f, 8f, -1000f), new Vector3(1000f, 8f, -1000f),
                new Vector3(-1000f, 8f, 1000f), new Vector3(1000f, 8f, 1000f) },
            new[] { Vector3.up, Vector3.up, Vector3.up, Vector3.up },
            new[] { 0, 2, 1, 1, 2, 3 },
            new Vector2[4], new Color[4], new Vector2[4]);
    }
}
