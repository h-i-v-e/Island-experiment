using System;
using System.Reflection;
using Motu.Islands;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CylindricalCoastOrientationTests
    {
        private const BindingFlags PrivateInstance = BindingFlags.Instance | BindingFlags.NonPublic;

        [TestCase(0, 0u)]
        [TestCase(1, 2u)]
        [TestCase(2, 1u)]
        [TestCase(3, 0u)]
        [TestCase(-1, 1u)]
        [TestCase(int.MinValue, 2u)]
        public void SouthCoastReversesSectionOrderWithoutChangingWorldPlacement(int x, uint index)
        {
            var request = Request(new Vector2Int(x, -2));
            Assert.That(request.CoastalStripOptions.Value.sectionIndex, Is.EqualTo(index));
            Assert.That(request.Descriptor.LogicalXMetres, Is.EqualTo(x * 2000d));
            Assert.That(request.WorldRotation * Vector3.forward, Is.EqualTo(Vector3.back));
            Assert.That(request.WorldRotation * Vector3.right, Is.EqualTo(Vector3.left));
        }

        [TestCase(0)]
        [TestCase(7)]
        public void SouthCoastMaterialFieldsMeetInTheRotatedLocalFrame(int westX)
        {
            var west = Request(new Vector2Int(westX, -2));
            var east = Request(new Vector2Int(westX + 1, -2));
            var shared = new Vector3(westX * 2000f + 1000f, 12f, -3987f);
            var westMatrix = Matrix4x4.TRS(new Vector3(westX * 2000f, 0f, -4000f), west.WorldRotation, Vector3.one);
            var eastMatrix = Matrix4x4.TRS(new Vector3((westX + 1) * 2000f, 0f, -4000f), east.WorldRotation, Vector3.one);
            var westField = westMatrix.inverse.MultiplyPoint3x4(shared)
                + (Vector3)IslandGenerator.TerrainFieldOffset(west.Descriptor, west.Generation.TerrainShape, true);
            var eastField = eastMatrix.inverse.MultiplyPoint3x4(shared)
                + (Vector3)IslandGenerator.TerrainFieldOffset(east.Descriptor, east.Generation.TerrainShape, true);
            Assert.That(westField, Is.EqualTo(eastField));
        }

        [Test]
        public void WorldInstallationPreservesSouthFacingHorizonsDetailsAndForcedCacheRequests()
        {
            var host = new GameObject("South coast world installation");
            host.SetActive(false);
            try
            {
                var manager = host.AddComponent<IslandWorldManager>();
                var environment = host.AddComponent<WorldEnvironmentController>();
                typeof(IslandWorldManager).GetField("worldEnvironment", PrivateInstance).SetValue(manager, environment);
                var request = Request(new Vector2Int(1, -2));
                var entryType = typeof(IslandWorldManager).GetNestedType("IslandRuntimeEntry", BindingFlags.NonPublic);
                var entry = Activator.CreateInstance(entryType, PrivateInstance, null, new object[] { request }, null);
                var installedRequest = (IslandGenerationRequest)entryType.GetProperty("GenerationRequest", PrivateInstance)
                    .GetValue(entry);
                Assert.That(installedRequest.SnapshotPath, Is.Not.Null);
                Assert.That(installedRequest.CoastalInlandSouth, Is.True);
                Assert.That(installedRequest.CoastalStripOptions, Is.EqualTo(request.CoastalStripOptions));

                var generator = (IslandGenerator)typeof(IslandWorldManager).GetMethod("EnsureGenerator", PrivateInstance)
                    .Invoke(manager, new[] { entry });
                typeof(IslandWorldManager).GetMethod("InstallHorizon", PrivateInstance)
                    .Invoke(manager, new object[] { entry, Quad() });
                var horizon = host.GetComponentInChildren<IslandHorizonRenderer>(true);
                Assert.That(generator.transform.forward, Is.EqualTo(Vector3.back));
                Assert.That(horizon.transform.forward, Is.EqualTo(Vector3.back));
                Assert.That(generator.transform.position, Is.EqualTo(new Vector3(2000f, 0f, -4000f)));
                Assert.That(horizon.transform.position, Is.EqualTo(generator.transform.position));
            }
            finally { Object.DestroyImmediate(host); }
        }

        private static IslandGenerationRequest Request(Vector2Int cell)
        {
            var settings = new IslandGenerationSettings
            {
                TerrainShape = IslandTerrainShape.CoastalStrip,
                CoastalSectionCount = 3,
                UseSnapshotCache = false,
            };
            var profile = new IslandGenerationProfile(settings, new IslandRiverSettings(), new IslandForestSettings(),
                new IslandReedSettings(), new IslandFernSettings(), new IslandRenderingSettings(), null);
            return IslandGenerationRequest.FromOwnedProfile(42, cell, profile, default, coastalInlandSouth: true);
        }

        private static IslandPreparedMesh Quad() => new IslandPreparedMesh(
            new[] { new Vector3(-1000f, 0f, -1000f), new Vector3(1000f, 0f, -1000f),
                new Vector3(-1000f, 10f, 1000f), new Vector3(1000f, 10f, 1000f) },
            new[] { Vector3.up, Vector3.up, Vector3.up, Vector3.up }, new[] { 0, 2, 1, 1, 2, 3 },
            new Vector2[4], new Color[4], new Vector2[4]);
    }
}
