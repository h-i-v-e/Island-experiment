using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using NUnit.Framework;
using UnityEngine;

namespace Motu.Tests
{
    public sealed class CoastalStripNativeIntegrationTests
    {
        [Test, Category("NativeIntegration"), Timeout(300000)]
        public void IndependentlyGeneratedSectionsJoinAtTheManagedRenderAndColliderSeams()
        {
            var first = IntPtr.Zero;
            var second = IntPtr.Zero;
            try
            {
                first = Generate(0);
                second = Generate(1);
                for (var lod = 0; lod <= 3; lod++)
                {
                    var a = CopyMesh(first, lod);
                    var b = CopyMesh(second, lod);
                    AssertMatching(Edge(a, 1), Edge(b, -1), $"LOD{lod} neighbouring sections");
                    AssertMatching(Edge(b, 1), Edge(a, -1), $"LOD{lod} closing periodic seam");
                }
                var size = new IslandGenerationSettings().WorldSizeMetres;
                var aCollider = TerrainPreparation.PrepareColliderHeightMap(first, size, 33);
                var bCollider = TerrainPreparation.PrepareColliderHeightMap(second, size, 33);
                Assert.That(aCollider.dimension, Is.EqualTo(bCollider.dimension));
                for (var y = 0; y < aCollider.dimension; y++)
                {
                    Assert.That(aCollider.WorldHeightAt(aCollider.dimension - 1, y),
                        Is.EqualTo(bCollider.WorldHeightAt(0, y)).Within(.001f), $"Collider neighbour row {y}");
                    Assert.That(bCollider.WorldHeightAt(bCollider.dimension - 1, y),
                        Is.EqualTo(aCollider.WorldHeightAt(0, y)).Within(.001f), $"Collider wrap row {y}");
                }
            }
            finally
            {
                if (second != IntPtr.Zero) MotuNative.ReleaseMotu(second);
                if (first != IntPtr.Zero) MotuNative.ReleaseMotu(first);
            }
        }

        private static IntPtr Generate(int section)
        {
            var generation = new IslandGenerationSettings
            {
                TerrainShape = IslandTerrainShape.CoastalStrip,
                CoastalSectionCount = 2,
                CoastalSeamSegments = 16,
                UseSnapshotCache = false,
            };
            var profile = new IslandGenerationProfile(generation, new IslandRiverSettings(),
                new IslandForestSettings(), new IslandReedSettings(), new IslandFernSettings(),
                new IslandRenderingSettings(), new IslandDebugSettings(), new IslandCaveSettings { Enabled = false });
            var request = new IslandGenerationRequest(42, new Vector2Int(section, 0), profile, default);
            var handle = IslandPreparationPipeline.LoadOrGenerateNative(request.RandomSeed, request.Options,
                request.ForestOptions, request.ReedOptions, request.FernOptions, null, 0,
                request.CaveOptions, request.CaveNetworkOptions, request.CaveWalkOptions,
                out var generated, request.HybridForestOptions, request.CoastalStripOptions);
            Assert.That(generated, Is.True, "The test must independently generate each section.");
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero),
                Marshal.PtrToStringUTF8(CaveNative.GetLastCaveGenerationError()));
            return handle;
        }

        private static IslandPreparedMesh CopyMesh(IntPtr handle, int lod)
        {
            MotuNative.ExportMesh export;
            if (lod == 3) MotuNative.CreateIslandLod3Mesh(handle, out export);
            else MotuNative.CreateMesh(handle, IntPtr.Zero, lod, 0, out export);
            try
            {
                Assert.That(export.handle, Is.Not.EqualTo(IntPtr.Zero));
                var mesh = IslandMeshInterop.CopyNavigationMeshData(export,
                    new IslandGenerationSettings().WorldSizeMetres);
                Assert.That(mesh, Is.Not.Null);
                return mesh;
            }
            finally { MotuNative.ReleaseMesh(ref export); }
        }

        private static SortedDictionary<float, float> Edge(IslandPreparedMesh mesh, int side)
        {
            var x = side * new IslandGenerationSettings().WorldSizeMetres * .5f;
            var edge = new SortedDictionary<float, float>();
            foreach (var vertex in mesh.vertices)
                if (Mathf.Abs(vertex.x - x) < .001f) edge[vertex.z] = vertex.y;
            Assert.That(edge.Count, Is.GreaterThan(2), "A coast section must retain its rendered seam vertices.");
            return edge;
        }

        private static void AssertMatching(SortedDictionary<float, float> a,
            SortedDictionary<float, float> b, string description)
        {
            if (a.Count != b.Count)
            {
                Debug.Log(description + " left edge (Z,height): "
                    + string.Join("; ", a.Select(point => $"({point.Key:R}, {point.Value:R})")));
                Debug.Log(description + " right edge (Z,height): "
                    + string.Join("; ", b.Select(point => $"({point.Key:R}, {point.Value:R})")));
            }
            Assert.That(a.Count, Is.EqualTo(b.Count), description + " vertex count");
            using var left = a.GetEnumerator();
            using var right = b.GetEnumerator();
            while (left.MoveNext() && right.MoveNext())
            {
                Assert.That(left.Current.Key, Is.EqualTo(right.Current.Key).Within(.001f), description + " position");
                Assert.That(left.Current.Value, Is.EqualTo(right.Current.Value).Within(.001f), description + " height");
            }
        }
    }
}
