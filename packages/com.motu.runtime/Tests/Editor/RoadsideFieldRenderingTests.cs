using System.IO;
using System.Linq;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;

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

        [Test]
        public void WindbreakBushesShareTileMeshesAndLeaveBoundaryCollisionUnchanged()
        {
            var host = new GameObject("Field windbreak test");
            try
            {
                var data = Field();
                data.roadsideFields.bushes = new[]
                {
                    Bush(1011f, 1014f, 1.6f),
                    Bush(1012.55f, 1014.17f, 1.5f),
                    Bush(1035f, 1014f, 1.6f),
                };
                var town = TownRenderer.Install(host.transform, data, 2000f);
                var bushes = town.GetComponentsInChildren<MeshRenderer>()
                    .Where(renderer => renderer.name == "Windbreak bushes").ToArray();
                Assert.That(bushes, Has.Length.EqualTo(2));
                var paired = bushes.Single(renderer => renderer.bounds.size.x > 3f);
                var single = bushes.Single(renderer => renderer != paired);
                Assert.That(Mesh(paired).GetIndexCount(0), Is.EqualTo(2 * Mesh(single).GetIndexCount(0)));
                Assert.That(single.bounds.min.y, Is.EqualTo(1.96f).Within(.001f));
                Assert.That(single.bounds.max.y, Is.InRange(3.999f, 6.001f));
                Assert.That(paired.bounds.max.y, Is.EqualTo(single.bounds.max.y).Within(.001f));
                Assert.That(single.bounds.size.x, Is.InRange(1.5f, 2.5f));
                Assert.That(single.bounds.size.z, Is.InRange(1.5f, 2.5f));
                Assert.That(single.sharedMaterial, Is.SameAs(paired.sharedMaterial));
                Assert.That(single.sharedMaterial.GetTexture("_ForestNoise"), Is.TypeOf<Texture3D>());

                var group = paired.GetComponentInParent<LODGroup>();
                var lods = group.GetLODs();
                Assert.That(lods, Has.Length.EqualTo(3));
                var branches = lods[0].renderers.Single(renderer => renderer.name == "Windbreak branches");
                Assert.That(branches.sharedMaterial, Is.SameAs(paired.sharedMaterial));
                Assert.That(branches.sharedMaterial.GetTexture("_TwigCard").name, Is.EqualTo("TreeBranchFan"));
                Assert.That(Mesh(branches).GetIndexCount(0), Is.EqualTo(2 * 8 * 6));
                Assert.That(branches.bounds.max.y, Is.LessThan(paired.bounds.max.y));
                Assert.That(lods[0].renderers.Contains(paired), Is.True);
                Assert.That(lods[1].renderers.Contains(paired), Is.True);
                Assert.That(lods[1].renderers.Contains(branches), Is.False);
                Assert.That(lods[2].renderers.Select(renderer => renderer.name),
                    Is.EquivalentTo(new[] { "Stone walls", "Timber gates" }));
                Assert.That(town.GetComponentsInChildren<Collider>(), Has.Length.EqualTo(2));
                AssertBushRenders(paired, branches, group, host.transform);
            }
            finally { Object.DestroyImmediate(host); }

            static TownRoadsidePlanting Bush(float x, float z, float scale) =>
                new TownRoadsidePlanting { position = new[] { x, z, 2f }, yaw = .4f, scale = scale };
        }

        private static void AssertBushRenders(MeshRenderer bush, Renderer branches, LODGroup group, Transform host)
        {
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null) return;
            const int layer = 30;
            bush.gameObject.layer = layer;
            branches.gameObject.layer = layer;
            var camera = new GameObject("Windbreak review camera").AddComponent<Camera>();
            camera.transform.SetParent(host, false);
            camera.transform.position = bush.bounds.center + new Vector3(1.6f, -.95f, -2.4f);
            camera.transform.LookAt(bush.bounds.center);
            camera.cullingMask = 1 << layer;
            camera.clearFlags = CameraClearFlags.SolidColor;
            camera.backgroundColor = new Color(.38f, .42f, .46f);
            camera.orthographic = true;
            camera.orthographicSize = bush.bounds.size.y * .6f;
            camera.enabled = false;
            var previous = RenderTexture.active;
            var target = new RenderTexture(384, 256, 24);
            var image = new Texture2D(384, 256, TextureFormat.RGB24, false);
            try
            {
                camera.targetTexture = target;
                var nearby = Render(0);
                var directory = System.Environment.GetEnvironmentVariable("MOTU_TOWN_RENDER_DIR");
                if (!string.IsNullOrEmpty(directory))
                    File.WriteAllBytes(Path.Combine(directory, "windbreak-bush.png"), image.EncodeToPNG());
                var farther = Render(1);
                Assert.That(nearby.Count(IsGreen), Is.GreaterThan(1800));
                Assert.That(farther.Count(IsGreen), Is.GreaterThan(1800));
                Assert.That(nearby.Count(IsWood), Is.GreaterThan(farther.Count(IsWood) + 10),
                    "Bare branches should be visible through gaps in the canopy only at LOD0.");
            }
            finally
            {
                camera.targetTexture = null;
                RenderTexture.active = previous;
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
            }

            Color32[] Render(int lod)
            {
                group.ForceLOD(lod);
                camera.Render();
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, target.width, target.height), 0, 0);
                return image.GetPixels32();
            }

            static bool IsGreen(Color32 pixel) => pixel.g > pixel.r && pixel.g > pixel.b;
            static bool IsWood(Color32 pixel) => pixel.r > pixel.g && pixel.g > pixel.b;
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
