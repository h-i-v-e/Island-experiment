using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class CoastalStripRenderingTests
    {
        private const float WorldSize = 2000f;
        private const float PositionTolerance = .002f;
        private const float NormalTolerance = .0001f;

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator ExportedClampedSectionsKeepMixedLodSeamsAndRenderWithTheTerrainMaterial()
        {
            RequireCameraRendering();

            using var scene = new CoastalScene();
            var first = IntPtr.Zero;
            var second = IntPtr.Zero;
            var firstRequest = CreateRequest(0);
            var secondRequest = CreateRequest(1);
            try
            {
                first = Generate(firstRequest);
                second = Generate(secondRequest);
                var west = new IslandPreparedMesh[3][];
                var east = new IslandPreparedMesh[3][];
                for (var lod = 0; lod < 3; lod++)
                {
                    west[lod] = CopyClampedGrid(first, lod);
                    east[lod] = CopyClampedGrid(second, lod);
                }
                for (var westLod = 0; westLod < 3; westLod++)
                for (var eastLod = 0; eastLod < 3; eastLod++)
                {
                    AssertMatching(Edge(west[westLod], 1), Edge(east[eastLod], -1),
                        $"Neighbour LOD{westLod}/LOD{eastLod}");
                    AssertMatching(Edge(east[eastLod], 1), Edge(west[westLod], -1),
                        $"Closing period LOD{eastLod}/LOD{westLod}");
                }

                AssertReducedDistantMesh(west);
                AssertReducedDistantMesh(east);

                scene.AddSection(west[0], firstRequest, 0);
                scene.AddSection(east[2], secondRequest, 2);
                yield return null; // Let the renderer register the newly created meshes.
                scene.Capture("coastal-strip-mixed-lod0-lod2.png", false, false);
                scene.Capture("coastal-strip-mixed-lod0-lod2-seam.png", true, false);
                scene.Capture("coastal-strip-mixed-lod0-lod2-normals.png", true, true);
                scene.Capture("coastal-strip-bays-overhead.png", false, false, topDown: true);
            }
            finally
            {
                if (second != IntPtr.Zero) MotuNative.ReleaseMotu(second);
                if (first != IntPtr.Zero) MotuNative.ReleaseMotu(first);
            }
        }

        [UnityTest, Category("NativeIntegration"), Timeout(300000)]
        public IEnumerator RotatedBottomCoastKeepsThreeSectionMixedLodSeamsAndRendersFacingTheSea()
        {
            RequireCameraRendering();
            using var scene = new CoastalScene();
            var handles = new IntPtr[3];
            var requests = new IslandGenerationRequest[handles.Length];
            var sections = new IslandPreparedMesh[handles.Length][][];
            try
            {
                for (var section = 0; section < handles.Length; section++)
                {
                    var request = CreateRequest(section, handles.Length, true);
                    requests[section] = request;
                    Assert.That(request.CoastalInlandSouth, Is.True);
                    Assert.That(request.WorldRotation * Vector3.forward, Is.EqualTo(Vector3.back),
                        "The bottom coast must face its sea towards world +Z.");
                    handles[section] = Generate(request);
                    sections[section] = new IslandPreparedMesh[3][];
                    for (var lod = 0; lod < 3; lod++)
                        sections[section][lod] = CopyClampedGrid(handles[section], lod);
                    AssertReducedDistantMesh(sections[section]);
                }

                // Three sections expose an incorrect forward phase: with a two-section
                // period, incrementing and decrementing the native index are equivalent.
                for (var section = 0; section < handles.Length; section++)
                for (var leftLod = 0; leftLod < 3; leftLod++)
                for (var rightLod = 0; rightLod < 3; rightLod++)
                {
                    var next = (section + 1) % handles.Length;
                    AssertMatching(Edge(sections[section][leftLod], 1, requests[section].WorldRotation),
                        Edge(sections[next][rightLod], -1, requests[next].WorldRotation),
                        $"Rotated bottom {section}->{next}, LOD{leftLod}/LOD{rightLod}");
                }

                scene.AddSection(sections[0][0], requests[0], 0);
                scene.AddSection(sections[1][2], requests[1], 2);
                yield return null;
                scene.Capture("coastal-strip-bottom-mixed-lod0-lod2.png", false, false, true);
                scene.Capture("coastal-strip-bottom-mixed-lod0-lod2-seam.png", true, false, true);
                scene.Capture("coastal-strip-bottom-mixed-lod0-lod2-normals.png", true, true, true);
                scene.Capture("coastal-strip-bottom-bays-overhead.png", false, false, true, topDown: true);
            }
            finally
            {
                foreach (var handle in handles)
                    if (handle != IntPtr.Zero) MotuNative.ReleaseMotu(handle);
            }
        }

        private static void AssertReducedDistantMesh(IslandPreparedMesh[][] lods)
        {
            long CountIndices(IslandPreparedMesh[] tiles)
            {
                long count = 0;
                foreach (var tile in tiles) count += tile.triangles.Length;
                return count;
            }

            Assert.That(CountIndices(lods[2]), Is.LessThan(CountIndices(lods[0]) / 2),
                "Preserving the join collar must still allow distant terrain to simplify.");
        }

        private static void RequireCameraRendering()
        {
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
                Assert.Ignore("Coastal camera validation needs a graphics device; run without -nographics.");
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset))
                Assert.Ignore("Coastal camera validation needs an active URP pipeline.");
        }

        private static IslandGenerationRequest CreateRequest(int section, int count = 2, bool inlandSouth = false)
        {
            var settings = new IslandGenerationSettings
            {
                TerrainShape = IslandTerrainShape.CoastalStrip,
                CoastalSectionCount = count,
                CoastalSeamSegments = 16,
                UseSnapshotCache = false,
            };
            var profile = new IslandGenerationProfile(settings, new IslandRiverSettings(),
                new IslandForestSettings(), new IslandReedSettings(), new IslandFernSettings(),
                new IslandRenderingSettings(), new IslandDebugSettings(), new IslandCaveSettings { Enabled = false });
            var cell = new Vector2Int(section, inlandSouth ? -2 : 0);
            return IslandGenerationRequest.FromOwnedProfile(42, cell, profile, default,
                coastalInlandSouth: inlandSouth);
        }

        private static IntPtr Generate(IslandGenerationRequest request)
        {
            var handle = IslandPreparationPipeline.LoadOrGenerateNative(request.RandomSeed, request.Options,
                request.ForestOptions, request.ReedOptions, request.FernOptions, null, 0,
                request.CaveOptions, request.CaveNetworkOptions, request.CaveWalkOptions,
                out var generated, request.HybridForestOptions, request.CoastalStripOptions);
            Assert.That(handle, Is.Not.EqualTo(IntPtr.Zero),
                Marshal.PtrToStringUTF8(CaveNative.GetLastCaveGenerationError()));
            Assert.That(generated, Is.True);
            return handle;
        }

        private static IslandPreparedMesh[] CopyClampedGrid(IntPtr handle, int lod)
        {
            var area = new MotuNative.ExportArea(0, 0, 1, 1);
            MotuNative.CreateMeshGrid(handle, ref area, lod, 2, 15, out var grid);
            try
            {
                Assert.That(grid.handle, Is.Not.EqualTo(IntPtr.Zero));
                Assert.That(grid.length, Is.EqualTo(4));
                var meshes = new List<IslandPreparedMesh>();
                var stride = Marshal.SizeOf<MotuNative.ExportMesh>();
                for (var tile = 0; tile < grid.length; tile++)
                {
                    var source = Marshal.PtrToStructure<MotuNative.ExportMesh>(IntPtr.Add(grid.data, tile * stride));
                    Assert.That(source.handle, Is.Not.EqualTo(IntPtr.Zero));
                    // Fully submerged slices can be empty after the ten-metre render-floor clip.
                    if (source.triangles.length == 0) continue;
                    var mesh = IslandMeshInterop.CopyTerrainMeshData(source, lod, WorldSize);
                    Assert.That(mesh.normals.Length, Is.EqualTo(mesh.vertices.Length));
                    meshes.Add(mesh);
                }
                Assert.That(meshes.Count, Is.GreaterThan(0));
                return meshes.ToArray();
            }
            finally { MotuNative.ReleaseMeshGrid(ref grid); }
        }

        private static List<(Vector3 position, Vector3 normal)> Edge(IslandPreparedMesh[] meshes, int side,
            Quaternion? rotation = null)
        {
            var worldRotation = rotation ?? Quaternion.identity;
            var points = new List<(Vector3 position, Vector3 normal)>();
            foreach (var mesh in meshes)
            for (var index = 0; index < mesh.vertices.Length; index++)
            {
                var position = worldRotation * mesh.vertices[index];
                var normal = worldRotation * mesh.normals[index];
                if (Mathf.Abs(position.x - side * WorldSize * .5f) > PositionTolerance) continue;
                position.x = 0;
                var duplicate = points.FindIndex(value =>
                    Vector3.Distance(value.position, position) <= PositionTolerance);
                if (duplicate >= 0)
                {
                    Assert.That(Vector3.Distance(points[duplicate].normal, normal),
                        Is.LessThan(NormalTolerance), "Grid slices must preserve the same exported seam normal.");
                    continue;
                }
                points.Add((position, normal));
            }
            points.Sort((a, b) => a.position.z.CompareTo(b.position.z));
            Assert.That(points.Count, Is.GreaterThan(2));
            return points;
        }

        private static void AssertMatching(List<(Vector3 position, Vector3 normal)> a,
            List<(Vector3 position, Vector3 normal)> b, string description)
        {
            Assert.That(a.Count, Is.EqualTo(b.Count), description + " unique exported seam vertices");
            for (var index = 0; index < a.Count; index++)
            {
                Assert.That(Vector3.Distance(a[index].position, b[index].position),
                    Is.LessThan(PositionTolerance), description + $" position {index}");
                Assert.That(Vector3.Distance(a[index].normal, b[index].normal),
                    Is.LessThan(NormalTolerance), description + $" normal {index}: {a[index].position:R} {a[index].normal:R} vs {b[index].position:R} {b[index].normal:R}");
            }
        }

        private sealed class CoastalScene : IDisposable
        {
            private const int Layer = 30;
            private const int SeaLayer = 29;
            private readonly List<Object> owned = new List<Object>();
            private readonly List<Material> materials = new List<Material>();
            private readonly GameObject root;
            private readonly Camera camera;
            private readonly RenderTexture target;
            private readonly Texture2D image;
            private readonly Texture2DArray albedo, normals, masks;
            private readonly Texture3D noise;
            private readonly GameObject sea;
            private float rowZ;
            private readonly RenderTexture oldTarget = RenderTexture.active;
            private readonly Color oldAmbient = RenderSettings.ambientLight;
            private readonly AmbientMode oldAmbientMode = RenderSettings.ambientMode;
            private readonly bool oldFog = RenderSettings.fog;
            private readonly Light oldSun = RenderSettings.sun;
            private readonly float oldCloudEnabled = Shader.GetGlobalFloat("_MotuCloudEnabled");
            private readonly float oldCloudNight = Shader.GetGlobalFloat("_MotuCloudNightStrength");
            private readonly Color background = new Color(.08f, .14f, .22f, 1);

            internal CoastalScene()
            {
                root = Own(new GameObject("Exported coastal strip render validation"));
                camera = Child("Coastal review camera").AddComponent<Camera>();
                camera.enabled = false;
                camera.orthographic = true;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = background;
                camera.cullingMask = 1 << Layer;
                camera.nearClipPlane = 1;
                camera.farClipPlane = 12000;
                camera.allowHDR = false;
                camera.allowMSAA = false;
                camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                target = Own(new RenderTexture(1200, 800, 24));
                image = Own(new Texture2D(target.width, target.height, TextureFormat.RGB24, false));
                camera.targetTexture = target;
                var sun = Child("Coastal review sun").AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 1.5f;
                sun.shadows = LightShadows.None;
                sun.cullingMask = 1 << Layer;
                sun.transform.rotation = Quaternion.Euler(45, -30, 0);
                RenderSettings.sun = sun;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = new Color(.6f, .6f, .6f);
                RenderSettings.fog = false;
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_MotuCloudNightStrength", 0);
                sea = CreateReferenceSea();
                albedo = LayerArray(false, false);
                normals = LayerArray(true, false);
                masks = LayerArray(false, true);
                noise = Own(new Texture3D(8, 8, 8, TextureFormat.RGBA32, false)
                    { wrapMode = TextureWrapMode.Repeat, filterMode = FilterMode.Bilinear });
                var pixels = new Color[8 * 8 * 8];
                for (var z = 0; z < 8; z++)
                for (var y = 0; y < 8; y++)
                for (var x = 0; x < 8; x++)
                    pixels[(z * 8 + y) * 8 + x] = new Color(.5f, .5f, .5f, 1);
                noise.SetPixels(pixels);
                noise.Apply();
            }

            internal void AddSection(IslandPreparedMesh[] meshes, IslandGenerationRequest request, int lod)
            {
                var island = Child($"Section {request.IslandGridPosition.x}, LOD{lod}");
                rowZ = request.IslandGridPosition.y * WorldSize;
                sea.transform.position = new Vector3(1000f, 0f, rowZ);
                island.transform.SetLocalPositionAndRotation(
                    new Vector3(request.IslandGridPosition.x * WorldSize, 0, rowZ),
                    request.WorldRotation);
                var shader = Shader.Find("Motu/Terrain Unified");
                Assert.That(shader, Is.Not.Null);
                var material = Own(new Material(shader));
                materials.Add(material);
                if (lod == 1) material.EnableKeyword("MOTU_TERRAIN_LOD1");
                if (lod == 2) material.EnableKeyword("MOTU_TERRAIN_LOD2");
                material.SetMatrix("_IslandWorldToLocal", island.transform.worldToLocalMatrix);
                material.SetVector("_MotuTerrainFieldOffset", IslandGenerator.TerrainFieldOffset(
                    request.Descriptor, request.Generation.TerrainShape, request.CoastalInlandSouth));
                material.SetVector("_MotuTerrainSeam", IslandGenerator.TerrainSeamSettings(request.Generation));
                material.SetTexture("_TerrainAlbedoArray", albedo);
                material.SetTexture("_TerrainNormalArray", normals);
                material.SetTexture("_TerrainMaskArray", masks);
                material.SetTexture("_CliffNoise3D", noise);
                material.SetFloat("_WorldNormalWeight", 0);
                material.SetFloat("_OcclusionStrength", 0);
                material.SetFloat("_SnowLine", 350);
                foreach (var data in meshes)
                {
                    var tile = Child("Clamped native terrain slice", island.transform);
                    tile.AddComponent<MeshFilter>().sharedMesh = Own(IslandMeshInterop.CreateTerrainMesh(data, lod));
                    tile.AddComponent<MeshRenderer>().sharedMaterial = material;
                }
            }

            internal void Capture(string fileName, bool close, bool normalView, bool inlandSouth = false,
                bool topDown = false)
            {
                foreach (var material in materials) material.SetFloat("_TerrainDebugView", normalView ? 4 : 0);
                camera.orthographicSize = close ? 650 : 1650;
                var direction = inlandSouth ? -1f : 1f;
                camera.transform.position = close
                    ? new Vector3(1000, 1300, rowZ - 1300 * direction)
                    : new Vector3(1000, 3000, rowZ - 3200 * direction);
                camera.transform.LookAt(new Vector3(1000, 80, rowZ + (close ? 500 : 200) * direction));
                if (topDown)
                    camera.transform.SetPositionAndRotation(new Vector3(1000, 4500, rowZ), Quaternion.Euler(90, 0, 0));
                camera.cullingMask = 1 << SeaLayer;
                RenderToImage();
                var clearPixels = image.GetPixels32();
                camera.cullingMask = (1 << Layer) | (1 << SeaLayer);
                RenderToImage();
                var directory = Path.Combine(Path.DirectorySeparatorChar == '/' ? "/tmp" : Path.GetTempPath(),
                    "coastal-strip-review");
                Directory.CreateDirectory(directory);
                var path = Path.Combine(directory, fileName);
                File.WriteAllBytes(path, image.EncodeToPNG());
                TestContext.Progress.WriteLine("Coastal camera render: " + path);
                var visible = 0;
                var magenta = 0;
                var rendered = image.GetPixels32();
                for (var index = 0; index < rendered.Length; index++)
                {
                    var color = (Color)rendered[index];
                    var clear = (Color)clearPixels[index];
                    if (Vector3.Distance(new Vector3(color.r, color.g, color.b),
                        new Vector3(clear.r, clear.g, clear.b)) > .08f) visible++;
                    if (color.r > .9f && color.g < .1f && color.b > .9f) magenta++;
                }
                Assert.That(visible, Is.GreaterThan(image.width * image.height / 10),
                    "The actual camera must draw exported terrain beyond the water-only reference.");
                Assert.That(magenta, Is.LessThan(image.width * image.height / 100),
                    "The real terrain material must compile and render without the error shader.");
                Assert.That(ShaderUtil.ShaderHasError(materials[0].shader), Is.False);
            }

            private void RenderToImage()
            {
                RenderPipeline.SubmitRenderRequest(camera,
                    new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0);
                image.Apply();
            }

            private GameObject CreateReferenceSea()
            {
                var water = Child("Flat sea-level reference");
                water.layer = SeaLayer;
                var shader = Shader.Find("Universal Render Pipeline/Unlit");
                Assert.That(shader, Is.Not.Null);
                var material = Own(new Material(shader));
                material.SetColor("_BaseColor", new Color(.05f, .2f, .3f));
                var mesh = Own(new Mesh
                {
                    name = "Sea-level reference plane",
                    vertices = new[] { new Vector3(-5000, 0, -3000), new Vector3(5000, 0, -3000),
                        new Vector3(-5000, 0, 3000), new Vector3(5000, 0, 3000) },
                    triangles = new[] { 0, 2, 1, 1, 2, 3 },
                });
                mesh.RecalculateBounds();
                water.AddComponent<MeshFilter>().sharedMesh = mesh;
                water.AddComponent<MeshRenderer>().sharedMaterial = material;
                return water;
            }

            private Texture2DArray LayerArray(bool normal, bool mask)
            {
                var texture = Own(new Texture2DArray(2, 2, 6, TextureFormat.RGBA32, false, normal || mask)
                    { wrapMode = TextureWrapMode.Repeat, filterMode = FilterMode.Bilinear });
                var colours = new[]
                {
                    new Color(.19f, .12f, .06f), new Color(.12f, .16f, .07f), new Color(.35f, .36f, .31f),
                    new Color(.24f, .26f, .23f), new Color(.64f, .58f, .38f), new Color(.32f, .31f, .28f),
                };
                for (var layer = 0; layer < 6; layer++)
                {
                    var color = normal ? new Color(.5f, .5f, 1, 1)
                        : mask ? new Color(.5f, 1, 0, 1) : colours[layer];
                    texture.SetPixels(new[] { color, color, color, color }, layer);
                }
                texture.Apply();
                return texture;
            }

            private T Own<T>(T value) where T : Object { owned.Add(value); return value; }

            private GameObject Child(string name, Transform parent = null)
            {
                var child = new GameObject(name) { layer = Layer };
                child.transform.SetParent(parent != null ? parent : root.transform, false);
                return child;
            }

            public void Dispose()
            {
                camera.targetTexture = null;
                RenderTexture.active = oldTarget;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.ambientMode = oldAmbientMode;
                RenderSettings.fog = oldFog;
                RenderSettings.sun = oldSun;
                Shader.SetGlobalFloat("_MotuCloudEnabled", oldCloudEnabled);
                Shader.SetGlobalFloat("_MotuCloudNightStrength", oldCloudNight);
                target.Release();
                for (var index = owned.Count - 1; index >= 0; index--) Object.DestroyImmediate(owned[index]);
            }
        }
    }
}
