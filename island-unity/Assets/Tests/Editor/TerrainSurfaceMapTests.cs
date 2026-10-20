using System;
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
            internal Ground()
            {
                var pixels = new Color[8]; for (int i = 0; i < 8; i++) pixels[i] = new Color(.5f, .5f, .5f, 1);
                noise.SetPixels(pixels); noise.Apply();
                Material.SetTexture("_CliffNoise3D", noise);
                Material.SetTexture("_GrassPatchNoise", Texture2D.whiteTexture);
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
            public void Dispose() { Object.DestroyImmediate(Root); Object.DestroyImmediate(Material); Object.DestroyImmediate(noise); }
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
