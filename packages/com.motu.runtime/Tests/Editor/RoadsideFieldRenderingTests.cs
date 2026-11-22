using System.Linq;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;

namespace Motu.Tests
{
    public sealed class RoadsideFieldRenderingTests
    {
        [Test]
        public void SingleWallMeshAndClosedGateHaveMatchingCollision()
        {
            var host = new GameObject("Roadside field test");
            try
            {
                var town = TownRenderer.Install(host.transform, Field(), 2000f);
                Assert.That(town.RoadsideFieldCount, Is.EqualTo(1));
                var groups = town.GetComponentsInChildren<LODGroup>();
                Assert.That(groups, Has.Length.EqualTo(1));
                var lods = groups[0].GetLODs();
                Assert.That(lods, Has.Length.EqualTo(1));
                Assert.That(lods[0].renderers, Has.Length.EqualTo(2));
                var walls = lods[0].renderers.Single(renderer => renderer.name == "Stone walls");
                Assert.That(Mesh(walls).GetIndexCount(0), Is.EqualTo(96),
                    "Four wall spans have only two sides, a cap, and the four end faces.");
                Assert.That(lods[0].renderers.Any(renderer => renderer.name == "Timber gates"), Is.True);

                Assert.That(town.RoadsideWallColliders, Has.Count.EqualTo(1));
                var wall = town.RoadsideWallColliders[0];
                var gate = town.GetComponentsInChildren<BoxCollider>().Single();
                Assert.That(gate.size.z, Is.LessThan(2.1f));
                Physics.SyncTransforms();
                Assert.That(wall.Raycast(new Ray(new Vector3(11f, 2.55f, 10f), Vector3.forward), out _, 4f), Is.True);
                Assert.That(wall.Raycast(new Ray(new Vector3(15f, 2.55f, 10f), Vector3.forward), out _, 4f), Is.False);
                Assert.That(gate.Raycast(new Ray(new Vector3(15f, 2.55f, 10f), Vector3.forward), out _, 4f), Is.True);
            }
            finally { Object.DestroyImmediate(host); }
        }

        [Test]
        public void SeparateWallRunsShareTheCornerAndDividerAcrossTiles()
        {
            var host = new GameObject("Field wall junction test");
            try
            {
                var data = Field();
                data.roadsideFields.walls = new[]
                {
                    new TownRoadsideWall { points = Run(-6f, 4f, 0f, 4f) },
                    new TownRoadsideWall { points = Run(.0004f, 4f, 0f, 9f) },
                    new TownRoadsideWall { points = Run(5f, 7f, 0f, 7f) },
                };
                data.roadsideFields.gates = System.Array.Empty<TownRoadsideGate>();
                var town = TownRenderer.Install(host.transform, data, 2000f);
                Assert.That(town.RoadsideWallColliders.Count, Is.GreaterThan(1));
                Assert.That(town.RoadsideWallColliders.Sum(wall => (int)wall.sharedMesh.GetIndexCount(0)),
                    Is.EqualTo(99), "Four spans, three outer ends, and one three-way roof join.");

                Physics.SyncTransforms();
                bool HasRoof(float x, float z) => town.RoadsideWallColliders.Any(wall =>
                    wall.Raycast(new Ray(new Vector3(x, 5f, z), Vector3.down), out var hit, 3f)
                    && hit.normal.y > .98f && Mathf.Abs(hit.point.y - (Ground(x, z) + 1.02f)) < .025f);
                Assert.That(HasRoof(.14f, 3.84f), Is.True, "The outer corner must be filled, even at the tile seam.");
                Assert.That(HasRoof(.10f, 7.10f), Is.True, "A divider ending midway along a wall must share its roof.");
            }
            finally { Object.DestroyImmediate(host); }

            static float Ground(float x, float z) => 2f + x * .02f + z * .03f;
            static float[] Run(float ax, float az, float bx, float bz) =>
                new[] { 1000f + ax, 1000f + az, Ground(ax, az), 1000f + bx, 1000f + bz, Ground(bx, bz) };
        }

        private static Mesh Mesh(Renderer renderer) => renderer.GetComponent<MeshFilter>().sharedMesh;

        private static TownPreparedData Field() => new TownPreparedData
        {
            roadsideFields = new TownRoadsideFieldPlan
            {
                fields = new[]
                {
                    new TownRoadsideField { id = 1, outline = new[] { 1010f, 1012f, 2f, 1020f, 1012f, 2f,
                        1020f, 1020f, 2f, 1010f, 1020f, 2f }, wheat = true },
                },
                walls = new[]
                {
                    new TownRoadsideWall { points = new[] { 1010f, 1012f, 2f, 1011.7f, 1012f, 2f, 1014.08f, 1012f, 2f } },
                    new TownRoadsideWall { points = new[] { 1015.92f, 1012f, 2f, 1018f, 1012f, 2f, 1020f, 1012f, 2f } },
                },
                gates = new[]
                {
                    new TownRoadsideGate { field_id = 1, left = new[] { 1014.08f, 1012f, 2f }, right = new[] { 1015.92f, 1012f, 2f } },
                },
            },
        };
    }
}
