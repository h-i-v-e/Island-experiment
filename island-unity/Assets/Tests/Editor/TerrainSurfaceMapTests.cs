using System;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class TerrainSurfaceMapTests
    {
        internal sealed class Ground : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Footstep surface fixture");
            internal readonly Material Material = new Material(Shader.Find("Motu/Terrain Unified"));
            readonly Texture3D noise = new Texture3D(2, 2, 2, TextureFormat.RGBA32, false);
            readonly Texture2D grassPatches = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            internal Ground()
            {
                var pixels = new Color[8]; for (int i = 0; i < 8; i++) pixels[i] = new Color(.5f, .5f, .5f, 1);
                noise.SetPixels(pixels); noise.Apply();
                Material.SetTexture("_CliffNoise3D", noise);
                // The same patch value is bare on poor soil and grassy on rich
                // soil, exercising the shared terrain/query/grass response.
                grassPatches.SetPixels(new[] { Color.gray * .4f, Color.gray * .4f,
                    Color.gray * .4f, Color.gray * .4f });
                grassPatches.Apply();
                Material.SetTexture("_GrassPatchNoise", grassPatches);
                Material.SetFloat("_SnowLine", 1000);
            }
            internal static Mesh Quad(float height, Color material, Vector2 environment, float south = -1000, float north = 1000)
            {
                var mesh = new Mesh();
                mesh.vertices = new[] { new Vector3(-1000, height, south), new Vector3(-1000, height, north),
                    new Vector3(1000, height, north), new Vector3(1000, height, south) };
                mesh.normals = new[] { Vector3.up, Vector3.up, Vector3.up, Vector3.up };
                mesh.colors = new[] { material, material, material, material };
                mesh.uv2 = new[] { environment, environment, environment, environment };
                mesh.triangles = new[] { 0, 1, 2, 0, 2, 3 };
                return mesh;
            }
            public void Dispose() { Object.DestroyImmediate(Root); Object.DestroyImmediate(Material); Object.DestroyImmediate(noise); Object.DestroyImmediate(grassPatches); }
        }

        [TestCase(TerrainSurfaceKind.Dirt)]
        [TestCase(TerrainSurfaceKind.Grass)]
        [TestCase(TerrainSurfaceKind.Stone)]
        [TestCase(TerrainSurfaceKind.Sand)]
        [TestCase(TerrainSurfaceKind.Gravel)]
        [TestCase(TerrainSurfaceKind.Mud)]
        [TestCase(TerrainSurfaceKind.Snow)]
        public void BakedGroundUsesTheTerrainShaderSurfaceCoverage(TerrainSurfaceKind kind)
        {
            using var ground = new Ground();
            Color attributes = Color.clear; Vector2 environment = Vector2.zero;
            float height = kind == TerrainSurfaceKind.Snow ? 1500 : kind == TerrainSurfaceKind.Sand ? 1 : 10;
            switch (kind)
            {
                case TerrainSurfaceKind.Grass: attributes.g = 1; break;
                case TerrainSurfaceKind.Stone: attributes.r = 1; break;
                case TerrainSurfaceKind.Sand: attributes.g = attributes.a = 1; break;
                case TerrainSurfaceKind.Gravel: environment.y = 1; break;
                case TerrainSurfaceKind.Mud: attributes.b = 1; break;
            }
            var mesh = Ground.Quad(height, attributes, environment);
            try
            {
                using var map = new TerrainSurfaceMap(ground.Root.transform, 2000, ground.Material);
                Assert.IsFalse(map.TryGetSample(Vector3.zero, out _));
                map.Bake(() => Graphics.DrawMeshNow(mesh, Matrix4x4.identity));
                AsyncGPUReadback.WaitAllRequests();
                Assert.IsTrue(map.TryGetSample(Vector3.zero, out var sample));
                Assert.That(sample.Kind, Is.EqualTo(kind)); Assert.That(sample.GroundHeight, Is.EqualTo(height).Within(.01f));
                Assert.IsFalse(ShaderUtil.ShaderHasError(Resources.Load<Shader>("TerrainSurfaceQuery")));
            }
            finally { Object.DestroyImmediate(mesh); }
        }

        [Test]
        public void TerrainRoadProfileClassifiesTheGravelShoulderAndPavedCore()
        {
            using var ground = new Ground();
            var mesh = Ground.Quad(10, new Color(0, 1, 0, 0), Vector2.zero);
            try
            {
                mesh.SetUVs(1, new[] { Vector3.zero, Vector3.zero, Vector3.forward, Vector3.forward });
                using var map = new TerrainSurfaceMap(ground.Root.transform, 2000, ground.Material);
                map.Bake(() => Graphics.DrawMeshNow(mesh, Matrix4x4.identity));
                AsyncGPUReadback.WaitAllRequests();
                Assert.That(map.TryGetSample(Vector3.left * 750f, out var grass), Is.True);
                Assert.That(map.TryGetSample(Vector3.zero, out var shoulder), Is.True);
                Assert.That(map.TryGetSample(Vector3.right * 750f, out var road), Is.True);
                Assert.That(grass.Kind, Is.EqualTo(TerrainSurfaceKind.Grass));
                Assert.That(shoulder.Kind, Is.EqualTo(TerrainSurfaceKind.Gravel));
                Assert.That(road.Kind, Is.EqualTo(TerrainSurfaceKind.Gravel));
            }
            finally { Object.DestroyImmediate(mesh); }
        }

        [Test]
        public void VillageCartTrackClassifiesTwoRutsWithGrassBetweenAndOutside()
        {
            using var ground = new Ground();
            var offsets = new[] { -4f, -1.4f, -1f, -.6f, .6f, 1f, 1.4f, 4f };
            var vertices = new Vector3[offsets.Length * 2];
            var normals = new Vector3[vertices.Length];
            var materials = new Color[vertices.Length];
            var environment = new Vector2[vertices.Length];
            var rutCoverage = new float[vertices.Length];
            var triangles = new int[(offsets.Length - 1) * 6];
            for (var index = 0; index < offsets.Length; index++)
            {
                var vertex = index * 2;
                vertices[vertex] = new Vector3(offsets[index], 10, -4);
                vertices[vertex + 1] = new Vector3(offsets[index], 10, 4);
                normals[vertex] = normals[vertex + 1] = Vector3.up;
                materials[vertex] = materials[vertex + 1] = new Color(0, 1, 0, 0);
                rutCoverage[vertex] = rutCoverage[vertex + 1] = Mathf.Abs(offsets[index]) == 1f ? 1f : 0f;
                if (index == offsets.Length - 1) break;
                var triangle = index * 6;
                triangles[triangle] = vertex;
                triangles[triangle + 1] = vertex + 1;
                triangles[triangle + 2] = vertex + 3;
                triangles[triangle + 3] = vertex;
                triangles[triangle + 4] = vertex + 3;
                triangles[triangle + 5] = vertex + 2;
            }

            var prepared = new IslandPreparedMesh(vertices, normals, triangles, Array.Empty<Vector2>(),
                materials, environment, cartTrackCoverage: rutCoverage);
            var mesh = IslandMeshInterop.CreateTerrainMesh(prepared, 0);
            try
            {
                using var map = new TerrainSurfaceMap(ground.Root.transform, 8, ground.Material);
                map.Bake(() => Graphics.DrawMeshNow(mesh, Matrix4x4.identity));
                AsyncGPUReadback.WaitAllRequests();
                foreach (var x in new[] { -2f, -1f, 0f, 1f, 2f })
                {
                    Assert.That(map.TryGetSample(Vector3.right * x, out var sample), Is.True);
                    Assert.That(sample.Kind, Is.EqualTo(Mathf.Abs(x) == 1f
                        ? TerrainSurfaceKind.Gravel
                        : TerrainSurfaceKind.Grass), $"Surface at {x} m");
                }
            }
            finally { Object.DestroyImmediate(mesh); }
        }

        [Test]
        public void MapCoordinatesFollowTranslatedRotatedIslandsAndRejectUncoveredSpace()
        {
            using var ground = new Ground();
            var south = Ground.Quad(10, Color.clear, Vector2.zero, -1000, 0);
            var north = Ground.Quad(40, new Color(0, 1, 0, 0), Vector2.zero, 0, 1000);
            try
            {
                using var map = new TerrainSurfaceMap(ground.Root.transform, 2000, ground.Material);
                map.Bake(() => { Graphics.DrawMeshNow(south, Matrix4x4.identity); Graphics.DrawMeshNow(north, Matrix4x4.identity); });
                AsyncGPUReadback.WaitAllRequests();
                ground.Root.transform.SetPositionAndRotation(new Vector3(4000, 20, -2000), Quaternion.Euler(0, 74, 0));
                foreach (float z in new[] { -500f, 500f })
                {
                    var point = ground.Root.transform.TransformPoint(new Vector3(123, 0, z));
                    Assert.IsTrue(map.TryGetSample(point, out var sample));
                    Assert.That(sample.Kind, Is.EqualTo(z < 0 ? TerrainSurfaceKind.Dirt : TerrainSurfaceKind.Grass));
                    Assert.That(sample.GroundHeight, Is.EqualTo(z < 0 ? 30 : 60).Within(.01f));
                    Assert.That(sample.SeaLevel, Is.EqualTo(20));
                }
                Assert.IsFalse(map.TryGetSample(ground.Root.transform.TransformPoint(Vector3.right * 1100), out _));
                map.Dispose(); Assert.IsFalse(map.TryGetSample(ground.Root.transform.position, out _));
            }
            finally { Object.DestroyImmediate(south); Object.DestroyImmediate(north); }
        }

        [Test]
        public void SharedTerrainNormalTextureControlsExposedRockAsItDoesInTheRenderer()
        {
            using var ground = new Ground();
            var normal = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var mesh = Ground.Quad(10, new Color(0, 1, 0, 0), Vector2.zero);
            try
            {
                // World-normal texture stores X,Z,Y; this encodes a vertical rock face.
                normal.SetPixel(0, 0, new Color(1, .5f, .5f, 1)); normal.Apply();
                ground.Material.SetTexture("_WorldNormal", normal);
                using var map = new TerrainSurfaceMap(ground.Root.transform, 2000, ground.Material);
                map.Bake(() => Graphics.DrawMeshNow(mesh, Matrix4x4.identity));
                AsyncGPUReadback.WaitAllRequests();
                Assert.IsTrue(map.TryGetSample(Vector3.zero, out var sample));
                Assert.That(sample.Kind, Is.EqualTo(TerrainSurfaceKind.Stone),
                    "Rendered steep rock must override the loose grass attributes on the mesh.");
            }
            finally { Object.DestroyImmediate(mesh); Object.DestroyImmediate(normal); }
        }

        [Test]
        public void DisposingDuringReadbackDoesNotLeaveAQueryableMap()
        {
            using var ground = new Ground();
            var mesh = Ground.Quad(10, Color.clear, Vector2.zero);
            try
            {
                using var map = new TerrainSurfaceMap(ground.Root.transform, 2000, ground.Material);
                map.Bake(() => Graphics.DrawMeshNow(mesh, Matrix4x4.identity));
                map.Dispose(); AsyncGPUReadback.WaitAllRequests();
                Assert.IsFalse(map.TryGetSample(Vector3.zero, out _));
            }
            finally { Object.DestroyImmediate(mesh); }
        }
    }
}
