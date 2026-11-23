using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using Motu.Islands;
using Motu.Streaming;
using Motu.Towns;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;

namespace Motu.Tests
{
    public sealed class WheatTileGeometryTests
    {
        [TestCase(1)]
        [TestCase(2)]
        public void RotatedCanopyFollowsBothTerrainTrianglesAndHasOnlyPerimeterWalls(int lod)
        {
            var bounds = new WheatTileBounds(0f, 0f, 10f, 10f);
            var field = new WheatField(new Vector2(5f, 5f), new Vector2(2.12f, 1.4f), 0.63f, 42);
            var data = WheatTileGeometry.Build(Terrain(bounds, RidgedGround), new[] { field }, lod, default, bounds);
            Assert.That(data.Plants, Is.Empty);
            var mesh = data.Canopy;
            float c = Mathf.Cos(field.Yaw), s = Mathf.Sin(field.Yaw);
            for (int i = 0; i < mesh.vertices.Length; i++)
            {
                var point = mesh.vertices[i];
                var uv = mesh.uv[i];
                Assert.That(Mathf.Abs(uv.x), Is.LessThanOrEqualTo(field.HalfExtents.x));
                Assert.That(Mathf.Abs(uv.y), Is.LessThanOrEqualTo(field.HalfExtents.y));
                Assert.That(point.x, Is.EqualTo(field.Centre.x + uv.x * c - uv.y * s).Within(1e-5f));
                Assert.That(point.z, Is.EqualTo(field.Centre.y + uv.x * s + uv.y * c).Within(1e-5f));
                Assert.That(point.y, Is.EqualTo(RidgedGround(point.x, point.z)
                    + mesh.material[i].r * WheatTileGeometry.CanopyHeight).Within(1e-5f));
            }

            float topArea = 0f, wallArea = 0f;
            for (int i = 0; i < mesh.triangles.Length; i += 3)
            {
                var a = mesh.vertices[mesh.triangles[i]];
                var b = mesh.vertices[mesh.triangles[i + 1]];
                var d = mesh.vertices[mesh.triangles[i + 2]];
                var cross = Vector3.Cross(b - a, d - a);
                if (mesh.normals[mesh.triangles[i]].y > 0f)
                {
                    Assert.That(cross.y, Is.GreaterThan(0f));
                    topArea += cross.y * 0.5f;
                }
                else
                {
                    var radial = new Vector3(a.x - field.Centre.x, 0f, a.z - field.Centre.y);
                    Assert.That(Vector3.Dot(radial, cross), Is.GreaterThan(0f));
                    wallArea += cross.magnitude * 0.5f;
                }
            }

            Assert.That(topArea, Is.EqualTo(4f * field.HalfExtents.x * field.HalfExtents.y).Within(1e-4f));
            Assert.That(wallArea, Is.EqualTo(4f * (field.HalfExtents.x + field.HalfExtents.y)
                * WheatTileGeometry.CanopyHeight).Within(1e-4f));
        }

        [Test]
        public void CanopyTopsMeetAcrossATileSeamWithoutMakingInteriorWalls()
        {
            var left = new WheatTileBounds(-6f, -6f, 0f, 6f);
            var right = new WheatTileBounds(0f, -6f, 6f, 6f);
            var field = new WheatField(new Vector2(0.4f, -0.3f), new Vector2(3.2f, 1.8f), 0.36f, 127);
            var a = WheatTileGeometry.Build(Terrain(left, Ground), new[] { field }, 2, default, left).Canopy;
            var b = WheatTileGeometry.Build(Terrain(right, Ground), new[] { field }, 1, default, right).Canopy;
            var seamA = Seam(a);
            var seamB = Seam(b);
            Assert.That(seamA.Length, Is.EqualTo(2));
            Assert.That(seamB.Length, Is.EqualTo(2));
            for (int i = 0; i < seamA.Length; i++)
                Assert.That(Vector3.Distance(seamA[i], seamB[i]), Is.LessThan(1e-5f));
        }

        [Test]
        public void ActualFieldBoundaryOnATileSeamOwnsItsWallOnce()
        {
            var left = new WheatTileBounds(-6f, -6f, 0f, 6f);
            var right = new WheatTileBounds(0f, -6f, 6f, 6f);
            var field = new WheatField(new Vector2(-2f, 0f), new Vector2(2f, 1f), 0f, 7);
            var a = WheatTileGeometry.Build(Terrain(left, Ground), new[] { field }, 2, default, left).Canopy;
            var b = WheatTileGeometry.Build(Terrain(right, Ground), new[] { field }, 2, default, right);
            Assert.That(b, Is.Null);
            int seamWallTriangles = 0;
            for (int i = 0; i < a.triangles.Length; i += 3)
                if (a.vertices[a.triangles[i]].x == 0f && a.vertices[a.triangles[i + 1]].x == 0f
                    && a.vertices[a.triangles[i + 2]].x == 0f) seamWallTriangles++;
            Assert.That(seamWallTriangles, Is.EqualTo(2));
        }

        [Test]
        public void PlantsUseTheHighestUpwardGroundAndOneTileOwnsSharedRoots()
        {
            const float edgeInset = 0.15f;
            var bounds = new WheatTileBounds(-6f, -6f, 6f, 6f);
            var field = new WheatField(new Vector2(0.4f, -0.3f), new Vector2(3.2f, 1.8f), 0.36f, 127);
            var fields = new[] { field };
            var combined = Terrain(bounds, Ground);
            var roots = WheatTileGeometry.Build(combined, fields, 0, default, bounds).Plants;
            var plantedArea = 4f * (field.HalfExtents.x - edgeInset) * (field.HalfExtents.y - edgeInset);
            Assert.That(roots.Length / plantedArea, Is.InRange(11f, 13f));

            var seamX = roots.OrderBy(p => p.Root.x).ElementAt(roots.Length / 2).Root.x;
            var left = new WheatTileBounds(-6f, -6f, seamX, 6f);
            var right = new WheatTileBounds(seamX, -6f, 6f, 6f);
            var west = WheatTileGeometry.Build(Terrain(left, Ground), fields, 0, default, left).Plants;
            var east = WheatTileGeometry.Build(Terrain(right, Ground), fields, 0, default, right).Plants;
            Assert.That(west.Any(p => p.Root.x == seamX), Is.False);
            Assert.That(east.Any(p => p.Root.x == seamX), Is.True);
            Assert.That(west.Concat(east).Select(p => Horizontal(p.Root)),
                Is.EquivalentTo(roots.Select(p => Horizontal(p.Root))));

            var lower = Terrain(bounds, (x, z) => Ground(x, z) - 1f);
            var ceiling = Terrain(bounds, (x, z) => Ground(x, z) + 4f);
            var overlapping = Mesh(combined.vertices.Concat(lower.vertices).Concat(ceiling.vertices).ToArray(),
                combined.triangles.Concat(lower.triangles.Select(i => i + 4))
                    .Concat(ceiling.triangles.Reverse().Select(i => i + 8)).ToArray());
            var plants = WheatTileGeometry.Build(overlapping, fields, 0, default, bounds).Plants;
            Assert.That(plants.Select(p => p.Root), Is.EqualTo(roots.Select(p => p.Root)));
            foreach (var plant in west.Concat(east))
                Assert.That(plant.Root.y, Is.EqualTo(Ground(plant.Root.x, plant.Root.z)).Within(1e-6f));
        }

        [TestCase(1, false)]
        [TestCase(2, true)]
        public void ConcaveRoadsideCropFollowsTerrainAndLeavesTheNotchOpen(int lod, bool reversed)
        {
            var left = new WheatTileBounds(-6f, -6f, 0f, 6f);
            var right = new WheatTileBounds(0f, -6f, 6f, 6f);
            var corners = new[] { new Vector2(-4f, -3f), new Vector2(-2f, -3f), new Vector2(0f, -3f),
                new Vector2(3f, -3f), new Vector2(3f, -1f), new Vector2(-1f, -1f), new Vector2(-1f, 1f),
                new Vector2(-1f, 3f), new Vector2(-4f, 3f), new Vector2(-4f, 0f) };
            if (reversed) Array.Reverse(corners, 1, corners.Length - 1);
            var outline = corners.SelectMany(point => new[] { 1000f + point.x, 1000f + point.y, 99f }).ToArray();
            var field = WheatFieldOutline.FromNative(outline, 1f, 127);
            var leftTerrain = Terrain(left, Ground);
            var rightTerrain = Terrain(right, Ground);
            var fields = new[] { field };
            var plants = WheatTileGeometry.Build(leftTerrain, fields, 0, default, left).Plants
                .Concat(WheatTileGeometry.Build(rightTerrain, fields, 0, default, right).Plants).ToArray();
            Assert.That(plants.Length, Is.GreaterThan(230));
            foreach (var plant in plants)
            {
                Assert.That(plant.Root.x > -1f && plant.Root.z > -1f, Is.False);
                Assert.That(plant.Root.y, Is.EqualTo(Ground(plant.Root.x, plant.Root.z)).Within(1e-5f));
            }

            var west = WheatTileGeometry.Build(leftTerrain, fields, lod, default, left).Canopy;
            var east = WheatTileGeometry.Build(rightTerrain, fields, lod, default, right).Canopy;
            float topArea = 0f, wallArea = 0f;
            foreach (var mesh in new[] { west, east })
            {
                for (int i = 0; i < mesh.triangles.Length; i += 3)
                {
                    var first = mesh.triangles[i];
                    var a = mesh.vertices[first];
                    var b = mesh.vertices[mesh.triangles[i + 1]];
                    var c = mesh.vertices[mesh.triangles[i + 2]];
                    var cross = Vector3.Cross(b - a, c - a);
                    if (mesh.normals[first].y > 0f)
                    {
                        var centre = (a + b + c) / 3f;
                        Assert.That(centre.x > -1f && centre.z > -1f, Is.False);
                        Assert.That(centre.y, Is.EqualTo(Ground(centre.x, centre.z)
                            + WheatTileGeometry.CanopyHeight).Within(1e-5f));
                        topArea += cross.y * .5f;
                    }
                    else wallArea += cross.magnitude * .5f;
                }
            }
            Assert.That(topArea, Is.EqualTo(26f).Within(1e-4f));
            Assert.That(wallArea, Is.EqualTo(26f * WheatTileGeometry.CanopyHeight).Within(1e-4f));
        }

        [Test]
        public void PreparationAddsRoadsideCropsAlongsideTownWheatAndLeavesPastureAlone()
        {
            var outline = new[] { 1000f, 1000f, 0f, 1004f, 1000f, 1f, 1004f, 1006f, 2f, 1000f, 1006f, 3f };
            var town = new TownPreparedData
            {
                wheat_fields = new[] { new TownTrackBlocker
                {
                    centre = new[] { 1100f, 1100f }, half_extents = new[] { 4f, 4f }
                } },
                roadsideFields = new TownRoadsideFieldPlan
                {
                    fields = new[]
                    {
                        new TownRoadsideField { id = 1, wheat = true, outline = outline },
                        new TownRoadsideField { id = 2, wheat = false }
                    },
                    windbreaks = new[] { new TownRoadsidePlanting { position = new[] { 1001.5f, 1002f, 1f }, clearance_radius = .8f } },
                    bushes = new[] { new TownRoadsidePlanting { position = new[] { 1002.5f, 1004f, 2f }, clearance_radius = 1f } },
                }
            };

            var preparation = IslandPreparedWheat.Prepare(town, 42, 1000f, default);
            Assert.That(preparation.Fields.Length, Is.EqualTo(2));
            Assert.That(preparation.Fields[0].Outline, Is.Null);
            var crop = preparation.Fields[1];
            Assert.That(crop.Centre, Is.EqualTo(new Vector2(1f, 1.5f)));
            Assert.That(crop.InwardDistance(0, 0), Is.EqualTo(1f));
            Assert.That(crop.InwardDistance(1.1, 0), Is.Zero);
            Assert.That(preparation.Fields[0].Exclusions, Is.Empty);
            Assert.That(crop.Exclusions.Select(exclusion => exclusion.Centre),
                Is.EqualTo(new[] { new Vector2(.75f, 1f), new Vector2(1.25f, 2f) }));
            var bounds = new WheatTileBounds(0f, 0f, 3f, 4f);
            var terrain = Terrain(bounds, Ground);
            var original = WheatFieldOutline.FromNative(outline, .5f, crop.Seed);
            var full = WheatTileGeometry.Build(terrain, new[] { original }, 0, default, bounds).Plants;
            var cleared = WheatTileGeometry.Build(terrain, new[] { crop }, 0, default, bounds).Plants;
            Assert.That(cleared.Length, Is.LessThan(full.Length));
            Assert.That(cleared.Select(plant => plant.Root),
                Is.EqualTo(full.Where(plant => !crop.Excludes(plant.Root)).Select(plant => plant.Root)));
            Assert.That(preparation.Mask.HasExclusions, Is.True);
        }

        [Test]
        public void Lod3AndTilesOutsidePlotsProduceNoGeometry()
        {
            var bounds = new WheatTileBounds(-6f, -6f, 6f, 6f);
            var terrain = Terrain(bounds, Ground);
            var field = new WheatField(Vector2.zero, Vector2.one, 0f, 1);
            var outside = new WheatField(new Vector2(20f, 20f), Vector2.one, 0f, 1);
            Assert.That(WheatTileGeometry.Build(terrain, new[] { field }, 3, default, bounds), Is.Null);
            Assert.That(WheatTileGeometry.Build(terrain, new[] { outside }, 0, default, bounds), Is.Null);
            Assert.Throws<OperationCanceledException>(() =>
                WheatTileGeometry.Build(terrain, new[] { field }, 2, new CancellationToken(true), bounds));
        }

        [Test]
        public void CanopyShaderIsAvailableInResources()
        {
            var shader = Resources.Load<Shader>("WheatCanopy");
            Assert.That(shader, Is.Not.Null);
            var material = new Material(shader);
            try
            {
                for (var pass = 0; pass < material.passCount; pass++) ShaderUtil.CompilePass(material, pass, true);
                Assert.That(ShaderUtil.ShaderHasError(shader), Is.False);
            }
            finally { UnityEngine.Object.DestroyImmediate(material); }
        }

        private static Vector3[] Seam(IslandPreparedMesh mesh)
        {
            var points = new List<Vector3>();
            for (int i = 0; i < mesh.triangles.Length; i += 3)
            {
                var indices = mesh.triangles.Skip(i).Take(3).ToArray();
                var onSeam = indices.Where(index => Mathf.Abs(mesh.vertices[index].x) < 1e-5f).ToArray();
                Assert.That(onSeam.Length, Is.LessThan(3));
                points.AddRange(onSeam.Where(index => mesh.normals[index].y > 0f).Select(index => mesh.vertices[index]));
            }
            return points.Distinct().OrderBy(point => point.z).ToArray();
        }

        private static float Ground(float x, float z) => 2f + 0.27f * x + 0.19f * z;
        private static float RidgedGround(float x, float z) => Ground(x, z) + 0.47f * Mathf.Max(0f, z - x);
        private static Vector2 Horizontal(Vector3 point) => new Vector2(point.x, point.z);

        private static IslandPreparedMesh Terrain(WheatTileBounds bounds, Func<float, float, float> height)
        {
            Vector3 Vertex(float x, float z) => new Vector3(x, height(x, z), z);
            return Mesh(new[] { Vertex(bounds.MinX, bounds.MinZ), Vertex(bounds.MinX, bounds.MaxZ),
                    Vertex(bounds.MaxX, bounds.MaxZ), Vertex(bounds.MaxX, bounds.MinZ) },
                new[] { 0, 1, 2, 0, 2, 3 });
        }

        private static IslandPreparedMesh Mesh(Vector3[] vertices, int[] triangles)
        {
            return new IslandPreparedMesh(vertices, Array.Empty<Vector3>(), triangles,
                Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
        }
    }
}
