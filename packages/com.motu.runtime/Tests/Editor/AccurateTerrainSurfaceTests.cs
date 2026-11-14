using Motu.Interop;
using Motu.Islands;
using Motu.World;
using NUnit.Framework;
using UnityEngine;

namespace Motu.Tests
{
    public sealed class AccurateTerrainSurfaceTests
    {
        [TestCase(0, 0, 1, 1, 1)]
        [TestCase(0, 73, 2, 3, .5f)]
        [TestCase(23, 41, 2, 3, .5f)]
        public void WorldVerticalLineAndGeometricNormalRespectTransform(float tilt, float yaw, float sx, float sy, float sz)
        {
            const float size = 2000;
            var matrix = Matrix4x4.TRS(new Vector3(130, 17, -90), Quaternion.Euler(tilt, yaw, 0), new Vector3(sx, sy, sz));
            var localSurface = new Vector3(120, 37, -80);
            var worldSurface = matrix.MultiplyPoint3x4(localSurface);
            var localNormal = new Vector3(-.2f, 1, .1f).normalized;
            var sample = new TerrainSurfaceNative.Sample
            {
                Position = new Vector3(localSurface.x / size + .5f, localSurface.z / size + .5f, localSurface.y / size),
                Normal = new Vector3(localNormal.x, localNormal.z, localNormal.y)
            };
            Assert.That(TerrainSurfaceCoordinates.TryToWorld(matrix, size, sample, out var height, out var normal), Is.True);
            Assert.That(height, Is.EqualTo(worldSurface.y).Within(.001f));
            // A geometric normal stays perpendicular to both transformed triangle tangents.
            var tangent = Vector3.Cross(localNormal, Vector3.forward).normalized;
            var bitangent = Vector3.Cross(localNormal, tangent).normalized;
            Assert.That(Mathf.Abs(Vector3.Dot(normal, matrix.MultiplyVector(tangent).normalized)), Is.LessThan(1e-5f));
            Assert.That(Mathf.Abs(Vector3.Dot(normal, matrix.MultiplyVector(bitangent).normalized)), Is.LessThan(1e-5f));
            foreach (float altitude in new[] { -250f, 300f })
            {
                Assert.That(TerrainSurfaceCoordinates.TryCreateLine(matrix.inverse, size, worldSurface + Vector3.up * altitude, out var origin, out var down), Is.True);
                // Moving this many world metres down must land on the same exact native point.
                Assert.That(Vector3.Distance(origin + down * altitude, sample.Position), Is.LessThan(1e-6f));
            }
        }

        [Test]
        public void MissingResidentDataAndInvalidPointsMissWithoutPreparingAnIsland()
        {
            var host = new GameObject("Read-only exact terrain query");
            host.SetActive(false);
            try
            {
                var world = host.AddComponent<IslandWorldManager>();
                var generator = host.AddComponent<IslandGenerator>();
                Assert.That(world.TryGetAccurateTerrainSurface(Vector3.zero, out _, out _), Is.False);
                Assert.That(generator.TryGetAccurateTerrainSurface(Vector3.zero, out _, out _), Is.False);
                Assert.That(world.TryGetAccurateTerrainSurface(new Vector3(float.NaN, 0, 0), out _, out _), Is.False);
                Assert.That(world.KnownIslandCount, Is.Zero);
                Assert.That(generator.HasRuntime, Is.False);
                Assert.That(TerrainSurfaceCoordinates.TryCreateLine(Matrix4x4.identity, 0, Vector3.zero, out _, out _), Is.False);
                Assert.That(TerrainSurfaceCoordinates.TryCreateLine(Matrix4x4.zero, 2000, Vector3.zero, out _, out _), Is.False);
            }
            finally { Object.DestroyImmediate(host); }
        }
    }
}
