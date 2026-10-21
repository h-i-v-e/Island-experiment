using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class TerrainSurfaceAtlasTests
    {
        const BindingFlags Private = BindingFlags.Instance | BindingFlags.NonPublic;
        const float WorldSize = 2000f;

        [TestCase(TextureFormat.RGB24, 13, 7, 3)]
        [TestCase(TextureFormat.R8, 9, 5, 3)]
        [TestCase(TextureFormat.RGB24, 2, 1, 2)]
        [TestCase(TextureFormat.R8, 1, 1, 1)]
        [TestCase(TextureFormat.RGB24, 2048, 1, 3)]
        [TestCase(TextureFormat.R8, 1024, 1, 3)]
        [TestCase(TextureFormat.RGB24, 1024, 2048, 3)]
        [TestCase(TextureFormat.R8, 1024, 2048, 3)]
        [TestCase(TextureFormat.RGB24, 2048, 1024, 3)]
        [TestCase(TextureFormat.R8, 2048, 1024, 3)]
        public void RectangularAtlasesKeepLinearChannelsAndBoundedMipFootprints(
            TextureFormat format, int width, int height, int mipCount)
        {
            var channels = format == TextureFormat.RGB24 ? 3 : 1;
            var pixels = new byte[width * height * channels];
            for (var pixel = 0; pixel < width * height; pixel++)
                for (var channel = 0; channel < channels; channel++)
                    pixels[pixel * channels + channel] = (byte)(37 + channel * 83);
            var texture = IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Atlas upload validation", width, height, format, pixels);
            try
            {
                Assert.That(texture.width, Is.EqualTo(width));
                Assert.That(texture.height, Is.EqualTo(height));
                Assert.That(texture.mipmapCount, Is.EqualTo(mipCount));
                Assert.That(texture.filterMode, Is.EqualTo(FilterMode.Trilinear));
                Assert.That(texture.wrapModeU, Is.EqualTo(TextureWrapMode.Clamp));
                Assert.That(texture.wrapModeV, Is.EqualTo(TextureWrapMode.Clamp));
                Assert.That(texture.anisoLevel, Is.EqualTo(1));
                Assert.IsFalse(texture.isReadable, "CPU texture copies must retire after upload.");
                var pixel = ReadPixel(texture);
                Assert.That(pixel.r, Is.EqualTo(pixels[0] / 255f).Within(.006f),
                    "Normal/AO values must survive GPU upload without an sRGB conversion.");
                if (channels == 3)
                {
                    Assert.That(pixel.g, Is.EqualTo(pixels[1] / 255f).Within(.006f));
                    Assert.That(pixel.b, Is.EqualTo(pixels[2] / 255f).Within(.006f));
                }
            }
            finally { Object.DestroyImmediate(texture); }
        }

        [Test]
        public void AtlasUploadRejectsMismatchedAndUnsupportedPixelBuffers()
        {
            Assert.Throws<ArgumentException>(() => IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Invalid atlas", 7, 3, TextureFormat.RGB24, new byte[7 * 3]));
            Assert.Throws<ArgumentException>(() => IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Invalid atlas", 7, 3, TextureFormat.RGBA32, new byte[7 * 3 * 4]));
            Assert.Throws<ArgumentOutOfRangeException>(() => IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Invalid atlas", 0, 3, TextureFormat.R8, Array.Empty<byte>()));
        }

        [Test]
        public void Lod1ShaderKeepsSlopedMeshNormalForBlankAtlasTexelsButUsesValidNormals()
        {
            using var ground = new TerrainSurfaceMapTests.Ground();
            var material = ground.Material;
            material.EnableKeyword("MOTU_TERRAIN_LOD1");
            material.SetFloat("_TerrainDebugView", 4);
            material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
            material.SetVector("_TerrainNormalStrengthsA", Vector4.zero);
            material.SetVector("_TerrainNormalStrengthsB", Vector4.zero);
            material.SetVector("_TerrainParallaxDepthsA", Vector4.zero);
            material.SetVector("_TerrainParallaxDepthsB", Vector4.zero);
            foreach (var strength in new[]
            {
                "_CliffNormalStrength", "_DirtNormalStrength", "_GrassNormalStrength",
                "_SandNormalStrength", "_SnowNormalStrength",
            }) material.SetFloat(strength, 0);
            var mesh = TerrainSurfaceMapTests.Ground.Quad(10, new Color(1, 0, 0, 0), Vector2.zero, -1, 1);
            var vertices = mesh.vertices;
            for (var index = 0; index < vertices.Length; index++)
            {
                vertices[index].x *= .001f;
                vertices[index].y = 10 + vertices[index].x * .5f;
            }
            var geometric = new Vector3(-.5f, 1, 0).normalized;
            mesh.vertices = vertices;
            mesh.normals = new[] { geometric, geometric, geometric, geometric };
            mesh.uv = new[] { Vector2.zero, Vector2.up, Vector2.one, Vector2.right };
            mesh.RecalculateBounds();
            var surface = new GameObject("Sloped atlas normal probe");
            surface.transform.SetParent(ground.Root.transform, false);
            surface.layer = 31;
            surface.AddComponent<MeshFilter>().sharedMesh = mesh;
            surface.AddComponent<MeshRenderer>().sharedMaterial = material;
            var albedo = new Texture2DArray(1, 1, 6, TextureFormat.RGBA32, false, true);
            var recipeNormals = new Texture2DArray(1, 1, 6, TextureFormat.RGBA32, false, true);
            var masks = new Texture2DArray(1, 1, 6, TextureFormat.RGBA32, false, true);
            var blank = IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Blank normal probe", 1, 1, TextureFormat.RGB24, new byte[] { 128, 128, 128 });
            var valid = IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Valid normal probe", 1, 1, TextureFormat.RGB24, new byte[] { 230, 166, 64 });
            var target = RenderTexture.GetTemporary(64, 64, 24, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                for (var layer = 0; layer < 6; layer++)
                {
                    albedo.SetPixels(new[] { Color.white }, layer);
                    recipeNormals.SetPixels(new[] { new Color(.5f, .5f, 1, 1) }, layer);
                    masks.SetPixels(new[] { new Color(.5f, 1, 0, 1) }, layer);
                }
                albedo.Apply(); recipeNormals.Apply(); masks.Apply();
                material.SetTexture("_TerrainAlbedoArray", albedo);
                material.SetTexture("_TerrainNormalArray", recipeNormals);
                material.SetTexture("_TerrainMaskArray", masks);
                var cameraObject = new GameObject("Terrain normal probe camera");
                cameraObject.transform.SetParent(ground.Root.transform, false);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.orthographic = true;
                camera.orthographicSize = 1.6f;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.magenta;
                camera.allowMSAA = false;
                camera.targetTexture = target;
                camera.transform.position = new Vector3(0, 13, -4);
                camera.transform.LookAt(new Vector3(0, 10, 0));
                Vector3 Capture(Texture normal, float weight)
                {
                    material.SetTexture("_WorldNormal", normal);
                    material.SetFloat("_WorldNormalWeight", weight);
                    UrpTestCamera.Render(camera);
                    RenderTexture.active = target;
                    readback.ReadPixels(new Rect(32, 32, 1, 1), 0, 0);
                    readback.Apply();
                    var pixel = readback.GetPixel(0, 0);
                    return new Vector3(pixel.r, pixel.g, pixel.b);
                }
                var baseline = Capture(blank, 0);
                Assert.That(Vector3.Distance(baseline, geometric * .5f + Vector3.one * .5f), Is.LessThan(.025f),
                    "The actual terrain shader must draw the sloped mesh normal in debug view 4.");
                var ignoredBlank = Capture(blank, 1);
                Assert.That(Vector3.Distance(ignoredBlank, baseline), Is.LessThan(.005f),
                    "RGB128 has no reliable direction; enabling the atlas must retain the geometric normal.");
                var sampled = Capture(valid, 1);
                var decoded = (new Vector3(230, 64, 166) / 127.5f - Vector3.one).normalized;
                Assert.That(Vector3.Distance(sampled, decoded * .5f + Vector3.one * .5f), Is.LessThan(.025f),
                    "Valid signed island XYZ normals must still reach the shader with Unity's Y/Z conversion.");
                Assert.That(Vector3.Distance(sampled, baseline), Is.GreaterThan(.2f),
                    "The guard must not disable valid atlas normals.");
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(readback);
                Object.DestroyImmediate(blank); Object.DestroyImmediate(valid);
                Object.DestroyImmediate(albedo); Object.DestroyImmediate(recipeNormals); Object.DestroyImmediate(masks);
                Object.DestroyImmediate(mesh);
                RenderTexture.ReleaseTemporary(target);
            }
        }

        [Test]
        public void LimitedMipGenerationKeepsOddOuterRowsAndColumns()
        {
            var pixels = new byte[5 * 3];
            for (var y = 0; y < 3; y++)
                for (var x = 0; x < 5; x++)
                    if (x == 4 || y == 2) pixels[y * 5 + x] = 255;
            var texture = IslandMaterialFactory.CreateSurfaceAtlasTexture(
                "Odd atlas mip validation", 5, 3, TextureFormat.R8, pixels);
            try
            {
                // 5x3 -> 2x1 averages every source texel, including the odd
                // right/bottom edges; 2x1 -> 1x1 rounds to byte 114.
                Assert.That(ReadPixel(texture, 2).r, Is.EqualTo(114f / 255f).Within(.01f));
            }
            finally { Object.DestroyImmediate(texture); }
        }

        [Test]
        public void CoarseMaterialsBindIndependentMapsAndRuntimeOwnsAllSixTextures()
        {
            var host = new GameObject("Terrain surface atlas bindings");
            host.SetActive(false);
            var generator = host.AddComponent<IslandGenerator>();
            var shader = Shader.Find("Motu/Terrain Unified");
            Assert.IsNotNull(shader);
            Assert.IsFalse(ShaderUtil.ShaderHasError(shader));
            var materials = new[] { new Material(shader), new Material(shader), new Material(shader) };
            var runtime = IslandRuntime.Create(IslandDescriptor.Request(17, Vector2Int.zero, null), host.transform);
            try
            {
                SetField(generator, "islandRuntime", runtime);
                SetField(generator, "terrainMaterial", materials[0]);
                SetField(generator, "terrainLod1Material", materials[1]);
                SetField(generator, "terrainLod2Material", materials[2]);
                foreach (var material in materials) runtime.OwnMaterial(material);
                var shared = Maps(8, 8, 128, 251);
                var coarse = new[] { Maps(13, 7, 64, 193), Maps(9, 5, 192, 127) };
                var upload = typeof(IslandGenerator).GetMethod("CreateSurfaceTextures", Private);
                Assert.IsNotNull(upload);
                upload.Invoke(generator, new object[] { shared, coarse });
                var textures = new List<Texture>();
                for (var level = 0; level < materials.Length; level++)
                {
                    var maps = level == 0 ? shared : coarse[level - 1];
                    foreach (var property in new[] { "_WorldNormal", "_Occlusion" })
                    {
                        var texture = materials[level].GetTexture(property) as Texture2D;
                        Assert.IsNotNull(texture, $"LOD{level} {property}");
                        CollectionAssert.DoesNotContain(textures, texture, "Every LOD owns a distinct texture pair.");
                        textures.Add(texture);
                        Assert.That(texture.width, Is.EqualTo(maps.width));
                        Assert.That(texture.height, Is.EqualTo(maps.height));
                        Assert.That(texture.mipmapCount, Is.EqualTo(level == 0 ? 4 : 3),
                            "LOD0 retains its full chain; coarse chart gutters limit the atlas chain.");
                        var pixel = ReadPixel(texture);
                        var expected = property == "_WorldNormal" ? maps.normalRgb[0] : maps.occlusion[0];
                        Assert.That(pixel.r, Is.EqualTo(expected / 255f).Within(.006f));
                    }
                }
                runtime.Dispose();
                runtime.Dispose();
                foreach (var texture in textures)
                    Assert.IsTrue(texture == null, "Runtime disposal must release every atlas exactly once.");
            }
            finally
            {
                runtime.Dispose();
                Object.DestroyImmediate(host);
                foreach (var material in materials) if (material != null) Object.DestroyImmediate(material);
            }
        }

        [Test]
        public void InvalidNativeAtlasRequestsReturnEmptyReleasableExports()
        {
            NativeAbi.Validate();
            foreach (var lod in new[] { 0, 1, 2, 3 })
            {
                Assert.That(MotuNative.CreateTerrainSurfaceAtlas(IntPtr.Zero, lod, 2048, out var maps), Is.Zero);
                Assert.That(maps.handle, Is.EqualTo(IntPtr.Zero));
                Assert.That(maps.normalRgb, Is.EqualTo(IntPtr.Zero));
                MotuNative.ReleaseTerrainSurfaceAtlas(ref maps);
                MotuNative.ReleaseTerrainSurfaceAtlas(ref maps);
                var area = new MotuNative.ExportArea(0, 0, 1, 1);
                MotuNative.CreateTerrainAtlasMeshGrid(IntPtr.Zero, ref area, lod, 2, 0, out var grid);
                Assert.That(grid.handle, Is.EqualTo(IntPtr.Zero));
                Assert.That(grid.length, Is.Zero);
                MotuNative.ReleaseMeshGrid(ref grid);
            }
            var error = Assert.Throws<InvalidOperationException>(() =>
                TerrainPreparation.PrepareSurfaceAtlas(IntPtr.Zero, 1, 2048));
            StringAssert.Contains("invalid island handle", error.Message);
        }

        [Test, Category("NativeIntegration"), Timeout(600000)]
        public void EmptyOffshoreIslandsPrepareNeutralAtlasesAndEmptyTerrainBatches()
        {
            var fixture = Environment.GetEnvironmentVariable("MOTU_TERRAIN_ATLAS_EMPTY_FIXTURE");
            if (string.IsNullOrEmpty(fixture))
                Assert.Ignore("Set MOTU_TERRAIN_ATLAS_EMPTY_FIXTURE to a fully submerged .motusnapshot file.");
            Assert.IsTrue(File.Exists(fixture), fixture);
            var handle = MotuNative.LoadMotuSnapshot(fixture, out var status);
            Assert.That(status, Is.Zero);
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero));
            var activeHandles = NativeIslandHandle.ActiveCount;
            using var owner = new NativeIslandHandle(handle);
            var managed = new IslandPreparedSurfaceMaps[2];
            try
            {
                var area = new MotuNative.ExportArea(0, 0, 1, 1);
                for (var index = 0; index < managed.Length; index++)
                {
                    var lod = index + 1;
                    var dimension = index == 0 ? 2048 : 1024;
                    var maps = TerrainPreparation.PrepareSurfaceAtlas(owner.Value, lod, dimension);
                    managed[index] = maps;
                    Assert.That(maps.width, Is.EqualTo(dimension));
                    Assert.That(maps.height, Is.EqualTo(1), "Empty render surfaces must not allocate a square blank atlas.");
                    Assert.That(maps.normalRgb.Length, Is.EqualTo(dimension * 3));
                    Assert.That(maps.occlusion.Length, Is.EqualTo(dimension));
                    for (var pixel = 0; pixel < dimension; pixel++)
                    {
                        Assert.That(maps.normalRgb[pixel * 3], Is.EqualTo(127));
                        Assert.That(maps.normalRgb[pixel * 3 + 1], Is.EqualTo(127));
                        Assert.That(maps.normalRgb[pixel * 3 + 2], Is.EqualTo(255));
                        Assert.That(maps.occlusion[pixel], Is.EqualTo(255));
                    }
                    // The streaming worker must accept a valid batch of empty
                    // cells even when all four coarse edge clamps are requested.
                    var tiles = TerrainGridWorker.Prepare(owner.Value, area, lod, 2, 15,
                        WorldSize, CancellationToken.None);
                    Assert.That(tiles.Length, Is.EqualTo(4));
                    Assert.IsTrue(Array.TrueForAll(tiles, tile => tile == null));
                    MotuNative.CreateTerrainSurfaceAtlas(owner.Value, lod, dimension, out var export);
                    try { Assert.That(export.handle, Is.Not.EqualTo(IntPtr.Zero)); }
                    finally { MotuNative.ReleaseTerrainSurfaceAtlas(ref export); }
                    MotuNative.ReleaseTerrainSurfaceAtlas(ref export);
                    Assert.That(export.handle, Is.EqualTo(IntPtr.Zero));
                    Assert.That(export.normalRgb, Is.EqualTo(IntPtr.Zero));
                    Assert.That(export.occlusion, Is.EqualTo(IntPtr.Zero));
                }
                var overview = TerrainTileStreamer.PrepareOverviewTiles(owner.Value, WorldSize);
                Assert.That(overview.Length, Is.GreaterThan(0));
                Assert.IsTrue(Array.TrueForAll(overview, tile => tile == null));
            }
            finally { owner.Dispose(); }
            Assert.That(NativeIslandHandle.ActiveCount, Is.EqualTo(activeHandles));
            Assert.That(managed[0].normalRgb[2], Is.EqualTo(255), "Managed copies survive native ownership release.");
            Assert.That(managed[1].occlusion[0], Is.EqualTo(255));
        }

        [Test, Category("NativeIntegration"), Timeout(600000)]
        public void MultipageLod2AtlasReachesManagedTilesAndGpuAsOneRectangularTexture()
        {
            var fixture = Environment.GetEnvironmentVariable("MOTU_TERRAIN_ATLAS_MULTIPAGE_FIXTURE");
            if (string.IsNullOrEmpty(fixture))
                Assert.Ignore("Set MOTU_TERRAIN_ATLAS_MULTIPAGE_FIXTURE to a disconnected-LOD2 .motusnapshot file.");
            Assert.IsTrue(File.Exists(fixture), fixture);
            var handle = MotuNative.LoadMotuSnapshot(fixture, out var status);
            Assert.That(status, Is.Zero);
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero));
            using var owner = new NativeIslandHandle(handle);
            Texture2D normal = null;
            Texture2D occlusion = null;
            try
            {
                const int pageDimension = 1024;
                var maps = TerrainPreparation.PrepareSurfaceAtlas(owner.Value, 2, pageDimension);
                Assert.IsTrue(maps.width > pageDimension || maps.height > pageDimension,
                    "This fixture must exercise several canonical 1024px pages.");
                Assert.That(maps.normalRgb.Length, Is.EqualTo(checked(maps.width * maps.height * 3)));
                Assert.That(maps.occlusion.Length, Is.EqualTo(checked(maps.width * maps.height)));
                var tiles = TerrainGridWorker.Prepare(owner.Value,
                    new MotuNative.ExportArea(0, 0, 1, 1), 2, 2, 0, WorldSize, CancellationToken.None);
                Assert.That(tiles.Length, Is.EqualTo(4));
                var pages = new HashSet<Vector2Int>();
                var vertices = 0;
                var triangles = 0;
                var minimumUv = Vector2.one;
                var maximumUv = Vector2.zero;
                foreach (var tile in tiles)
                {
                    if (tile == null) continue;
                    vertices += tile.vertices.Length;
                    triangles += tile.triangles.Length / 3;
                    Assert.That(tile.uv.Length, Is.EqualTo(tile.vertices.Length));
                    Assert.That(tile.material.Length, Is.EqualTo(tile.vertices.Length));
                    Assert.That(tile.environment.Length, Is.EqualTo(tile.vertices.Length));
                    foreach (var uv in tile.uv)
                    {
                        Assert.That(uv.x, Is.InRange(0f, 1f));
                        Assert.That(uv.y, Is.InRange(0f, 1f));
                        minimumUv = Vector2.Min(minimumUv, uv);
                        maximumUv = Vector2.Max(maximumUv, uv);
                        // Recover page coordinates from actual texture extents,
                        // so this covers both horizontal and vertical layouts.
                        pages.Add(new Vector2Int(
                            Mathf.Min((int)(uv.x * maps.width / pageDimension), (maps.width - 1) / pageDimension),
                            Mathf.Min((int)(uv.y * maps.height / pageDimension), (maps.height - 1) / pageDimension)));
                    }
                }
                Assert.That(vertices, Is.GreaterThan(0));
                Assert.That(triangles, Is.GreaterThan(0));
                Assert.That(pages.Count, Is.GreaterThan(1), "The managed grid must retain vertices from multiple atlas pages.");
                Assert.IsTrue((minimumUv.x < .5f && maximumUv.x > .5f)
                    || (minimumUv.y < .5f && maximumUv.y > .5f),
                    "Combined UVs must span the packed texture instead of aliasing every page onto the first.");
                normal = IslandMaterialFactory.CreateSurfaceAtlasTexture(
                    "Multipage normal validation", maps.width, maps.height, TextureFormat.RGB24, maps.normalRgb);
                occlusion = IslandMaterialFactory.CreateSurfaceAtlasTexture(
                    "Multipage occlusion validation", maps.width, maps.height, TextureFormat.R8, maps.occlusion);
                foreach (var texture in new[] { normal, occlusion })
                {
                    Assert.That(texture.width, Is.EqualTo(maps.width));
                    Assert.That(texture.height, Is.EqualTo(maps.height));
                    Assert.That(texture.mipmapCount, Is.EqualTo(3));
                    Assert.IsFalse(texture.isReadable);
                }
            }
            finally
            {
                Object.DestroyImmediate(normal);
                Object.DestroyImmediate(occlusion);
            }
        }

        [Test, Category("NativeIntegration"), Timeout(600000)]
        public void NativeCoarseAtlasesPreserveLod0ChannelsAndOutliveTheIslandHandle()
        {
            var fixture = Environment.GetEnvironmentVariable("MOTU_TERRAIN_ATLAS_FIXTURE");
            if (string.IsNullOrEmpty(fixture))
                Assert.Ignore("Set MOTU_TERRAIN_ATLAS_FIXTURE to a small generation-bench .motusnapshot file.");
            Assert.IsTrue(File.Exists(fixture), fixture);
            var handle = MotuNative.LoadMotuSnapshot(fixture, out var status);
            Assert.That(status, Is.Zero);
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero));
            var maps = new MotuNative.ExportSurfaceMaps[2];
            var grids = new MotuNative.ExportMeshGrid[2];
            var baseline = new IslandPreparedMesh[2][];
            var normalBytes = new byte[2][];
            try
            {
                var original = CopyWholeMesh(handle, 0);
                var originalMaps = TerrainPreparation.PrepareSurfaceMaps(handle, 32);
                var area = new MotuNative.ExportArea(0, 0, 1, 1);
                for (var index = 0; index < 2; index++)
                {
                    var lod = index + 1;
                    var dimension = index == 0 ? 2048 : 1024;
                    Assert.That(MotuNative.CreateTerrainSurfaceAtlas(handle, lod, dimension, out maps[index]), Is.EqualTo(1));
                    Assert.That(maps[index].width, Is.InRange(1, 8192));
                    Assert.That(maps[index].height, Is.InRange(1, 8192));
                    normalBytes[index] = new byte[maps[index].width * maps[index].height * 3];
                    Marshal.Copy(maps[index].normalRgb, normalBytes[index], 0, normalBytes[index].Length);
                    MotuNative.CreateMeshGrid(handle, ref area, lod, 2, 0, out var sourceGrid);
                    try { baseline[index] = CopyGrid(sourceGrid, lod); }
                    finally { MotuNative.ReleaseMeshGrid(ref sourceGrid); }
                    MotuNative.CreateTerrainAtlasMeshGrid(handle, ref area, lod, 2, 0, out grids[index]);
                    Assert.That(grids[index].length, Is.EqualTo(4));
                    Assert.That(grids[index].handle, Is.Not.EqualTo(IntPtr.Zero));
                }
                var unchanged = CopyWholeMesh(handle, 0);
                CollectionAssert.AreEqual(original.vertices, unchanged.vertices);
                CollectionAssert.AreEqual(original.normals, unchanged.normals);
                CollectionAssert.AreEqual(original.triangles, unchanged.triangles);
                CollectionAssert.AreEqual(original.uv, unchanged.uv);
                CollectionAssert.AreEqual(original.material, unchanged.material);
                CollectionAssert.AreEqual(original.environment, unchanged.environment);
                var unchangedMaps = TerrainPreparation.PrepareSurfaceMaps(handle, 32);
                CollectionAssert.AreEqual(originalMaps.normalRgb, unchangedMaps.normalRgb);
                CollectionAssert.AreEqual(originalMaps.occlusion, unchangedMaps.occlusion);
                MotuNative.ReleaseMotu(handle);
                handle = IntPtr.Zero;
                for (var index = 0; index < 2; index++)
                {
                    var retained = new byte[normalBytes[index].Length];
                    Marshal.Copy(maps[index].normalRgb, retained, 0, retained.Length);
                    CollectionAssert.AreEqual(normalBytes[index], retained,
                        "Atlas exports retain their bytes after the island handle is released.");
                    var atlas = CopyGrid(grids[index], index + 1);
                    ValidateAtlasChannels(baseline[index], atlas);
                    ValidateNormalEncoding(original, atlas, maps[index], retained);
                }
            }
            finally
            {
                foreach (var index in new[] { 0, 1 })
                {
                    MotuNative.ReleaseMeshGrid(ref grids[index]);
                    MotuNative.ReleaseTerrainSurfaceAtlas(ref maps[index]);
                    Assert.That(maps[index].handle, Is.EqualTo(IntPtr.Zero));
                }
                if (handle != IntPtr.Zero) MotuNative.ReleaseMotu(handle);
            }
        }

        static IslandPreparedSurfaceMaps Maps(int width, int height, byte normal, byte occlusion)
        {
            var normals = new byte[width * height * 3];
            var ao = new byte[width * height];
            for (var pixel = 0; pixel < ao.Length; pixel++)
            {
                normals[pixel * 3] = normal;
                normals[pixel * 3 + 1] = 128;
                normals[pixel * 3 + 2] = 240;
                ao[pixel] = occlusion;
            }
            return new IslandPreparedSurfaceMaps(width, height, normals, ao);
        }

        static void SetField(IslandGenerator generator, string name, object value) =>
            typeof(IslandGenerator).GetField(name, Private).SetValue(generator, value);

        static Color ReadPixel(Texture texture, int mip = 0)
        {
            var previous = RenderTexture.active;
            var width = Mathf.Max(texture.width >> mip, 1);
            var height = Mathf.Max(texture.height >> mip, 1);
            var target = RenderTexture.GetTemporary(width, height, 0,
                RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var readback = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            try
            {
                Graphics.Blit(texture, target);
                RenderTexture.active = target;
                readback.ReadPixels(new Rect(width / 2, height / 2, 1, 1), 0, 0);
                readback.Apply();
                return readback.GetPixel(0, 0);
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(target);
                Object.DestroyImmediate(readback);
            }
        }

        static IslandPreparedMesh CopyWholeMesh(IntPtr handle, int lod)
        {
            MotuNative.CreateMesh(handle, IntPtr.Zero, lod, 0, out var mesh);
            try { return IslandMeshInterop.CopyTerrainMeshData(mesh, lod, WorldSize); }
            finally { MotuNative.ReleaseMesh(ref mesh); }
        }

        static IslandPreparedMesh[] CopyGrid(MotuNative.ExportMeshGrid grid, int lod)
        {
            var result = new List<IslandPreparedMesh>();
            var stride = Marshal.SizeOf<MotuNative.ExportMesh>();
            for (var index = 0; index < grid.length; index++)
            {
                var mesh = Marshal.PtrToStructure<MotuNative.ExportMesh>(IntPtr.Add(grid.data, index * stride));
                if (mesh.vertices.length > 0 && mesh.triangles.length > 0)
                    result.Add(IslandMeshInterop.CopyTerrainMeshData(mesh, lod, WorldSize));
            }
            return result.ToArray();
        }

        static Vector3Int PositionKey(Vector3 value) => Vector3Int.RoundToInt(value * 1000f);

        static void ValidateAtlasChannels(IslandPreparedMesh[] baseline, IslandPreparedMesh[] atlas)
        {
            var channels = new Dictionary<Vector3Int, (Color material, Vector2 environment)>();
            foreach (var mesh in baseline)
                for (var index = 0; index < mesh.vertices.Length; index++)
                    channels[PositionKey(mesh.vertices[index])] = (mesh.material[index], mesh.environment[index]);
            var changedUv = 0;
            var matched = 0;
            var vertices = 0;
            foreach (var mesh in atlas)
                for (var index = 0; index < mesh.vertices.Length; index++)
                {
                    vertices++;
                    var point = mesh.vertices[index];
                    var uv = mesh.uv[index];
                    Assert.That(uv.x, Is.InRange(0f, 1f));
                    Assert.That(uv.y, Is.InRange(0f, 1f));
                    var planar = new Vector2(point.x / WorldSize + .5f, point.z / WorldSize + .5f);
                    if ((uv - planar).sqrMagnitude > .0001f) changedUv++;
                    if (!channels.TryGetValue(PositionKey(point), out var expected)) continue;
                    matched++;
                    Assert.That(((Vector4)mesh.material[index] - (Vector4)expected.material).magnitude, Is.LessThan(.002f));
                    Assert.That((mesh.environment[index] - expected.environment).magnitude, Is.LessThan(.002f),
                        "Materials and environment must be sampled through island coordinates, never atlas UVs.");
                }
            Assert.That(changedUv, Is.GreaterThan(vertices / 2));
            Assert.That(matched, Is.GreaterThan(vertices * .8f));
        }

        static void ValidateNormalEncoding(IslandPreparedMesh high, IslandPreparedMesh[] atlas,
            MotuNative.ExportSurfaceMaps maps, byte[] rgb)
        {
            var tested = 0;
            var sloped = 0;
            foreach (var mesh in atlas)
                for (var triangle = 0; triangle < mesh.triangles.Length && (tested < 12 || sloped < 4); triangle += 3)
                {
                    var a = mesh.triangles[triangle]; var b = mesh.triangles[triangle + 1]; var c = mesh.triangles[triangle + 2];
                    var uv = (mesh.uv[a] + mesh.uv[b] + mesh.uv[c]) / 3f;
                    var x = Mathf.Clamp((int)(uv.x * maps.width), 0, maps.width - 1);
                    var y = Mathf.Clamp((int)(uv.y * maps.height), 0, maps.height - 1);
                    var centre = new Vector2((x + .5f) / maps.width, (y + .5f) / maps.height);
                    if (!Barycentric(centre, mesh.uv[a], mesh.uv[b], mesh.uv[c], out var weights)) continue;
                    var point = mesh.vertices[a] * weights.x + mesh.vertices[b] * weights.y + mesh.vertices[c] * weights.z;
                    // Floor clipping creates extra interpolated normals below
                    // sea level; compare unchanged source-face interiors here.
                    if (point.y <= 1f) continue;
                    if (!SampleNormal(high, point, out var expected, out var support)) continue;
                    if (tested >= 12 && expected.y > .98f) continue;
                    var pixel = (y * maps.width + x) * 3;
                    // Native normal RGB is signed island XYZ; Unity swaps Y/Z exactly once.
                    var normal = new Vector3(rgb[pixel], rgb[pixel + 2], rgb[pixel + 1]) / 127.5f - Vector3.one;
                    Assert.That(normal.magnitude, Is.EqualTo(1f).Within(.02f));
                    Assert.That(Vector3.Dot(normal.normalized, expected), Is.GreaterThan(.98f),
                        $"Atlas {maps.width}x{maps.height}, triangle {triangle / 3}, texel ({x},{y}), "
                        + $"world ({point.x:R},{point.y:R},{point.z:R}), UV ({centre.x:R},{centre.y:R}), "
                        + $"weights {weights.ToString("F8")}, decoded {normal.ToString("F8")}, expected {expected.ToString("F8")}; {support}");
                    tested++;
                    if (expected.y <= .98f) sloped++;
                }
            Assert.That(tested, Is.GreaterThanOrEqualTo(8), "Sample enough chart interiors to verify signed XYZ encoding.");
            Assert.That(sloped, Is.GreaterThanOrEqualTo(4), "Include slopes so swapping the normal axes cannot pass accidentally.");
        }

        static bool SampleNormal(IslandPreparedMesh mesh, Vector3 point, out Vector3 normal, out string support)
        {
            // This independent reference uses clipped render vertices converted
            // to metres. Keep its faces large enough to limit export rounding;
            // native atlas sampling tests separately cover sub-metre source
            // faces in their original coordinates with double precision.
            const float minimumProjectedArea = 1.1920928955078125e-7f * WorldSize * WorldSize;
            for (var index = 0; index < mesh.triangles.Length; index += 3)
            {
                var a = mesh.triangles[index]; var b = mesh.triangles[index + 1]; var c = mesh.triangles[index + 2];
                var first = mesh.vertices[b] - mesh.vertices[a];
                var second = mesh.vertices[c] - mesh.vertices[a];
                var projectedArea = Mathf.Abs(first.x * second.z - first.z * second.x);
                if (projectedArea <= minimumProjectedArea) continue;
                if (!Barycentric(new Vector2(point.x, point.z), new Vector2(mesh.vertices[a].x, mesh.vertices[a].z),
                    new Vector2(mesh.vertices[b].x, mesh.vertices[b].z), new Vector2(mesh.vertices[c].x, mesh.vertices[c].z), out var weights)) continue;
                normal = (mesh.normals[a] * weights.x + mesh.normals[b] * weights.y + mesh.normals[c] * weights.z).normalized;
                support = $"render LOD0 triangle {index / 3}, projected cross area {projectedArea:R}, "
                    + $"corners {mesh.vertices[a].ToString("F8")}, {mesh.vertices[b].ToString("F8")}, {mesh.vertices[c].ToString("F8")}";
                return true;
            }
            normal = Vector3.zero;
            support = null;
            return false;
        }

        static bool Barycentric(Vector2 point, Vector2 a, Vector2 b, Vector2 c, out Vector3 weights)
        {
            float Cross(Vector2 first, Vector2 second) => first.x * second.y - first.y * second.x;
            var area = Cross(b - a, c - a);
            if (Mathf.Abs(area) < 1e-12f) { weights = Vector3.zero; return false; }
            var u = Cross(b - point, c - point) / area;
            var v = Cross(c - point, a - point) / area;
            weights = new Vector3(u, v, 1 - u - v);
            return weights.x >= -.00001f && weights.y >= -.00001f && weights.z >= -.00001f;
        }
    }
}
