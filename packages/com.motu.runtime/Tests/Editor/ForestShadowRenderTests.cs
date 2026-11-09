using System;
using System.Collections;
using System.IO;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ForestShadowRenderTests
    {
        [UnityTest]
        public IEnumerator ActualFarTreeBeyondTheDetailedBudgetShadowsVisibleGround() => VerifyFarCaster(false);

        [UnityTest]
        public IEnumerator ActualOffscreenFarTreeShadowsVisibleGround() => VerifyFarCaster(true);

        private static IEnumerator VerifyFarCaster(bool offscreen)
        {
            using var fixture = new SceneFixture();
            var groundMaterial = new Material(Shader.Find("Universal Render Pipeline/Lit"));
            try
            {
                groundMaterial.SetColor("_BaseColor", Color.gray);
                groundMaterial.SetFloat("_Smoothness", 0);
                var ground = fixture.Cube("Live shadow receiver", new Vector3(0, -.5f, 0), new Vector3(80, 1, 80));
                ground.GetComponent<MeshRenderer>().sharedMaterial = groundMaterial;
                fixture.Camera.orthographicSize = 40;
                fixture.Camera.transform.position = Vector3.up * 40;
                fixture.Sun.transform.rotation = Quaternion.Euler(45, offscreen ? 270 : 0, 0);
                var root = new GameObject("Instanced caster forest") { layer = 31 };
                root.transform.SetParent(ground.transform.parent, false);
                var wood = TreePart(new Vector3(.7f, 16, .7f), Vector3.up * 8);
                var crown = TreePart(new Vector3(8, 4, 8), Vector3.up * 16);
                var prototype = new[] { wood, crown, wood, crown, wood, crown, null, null };
                var trees = new HybridForestNative.Instance[97];
                for (var i = 0; i < 96; ++i)
                    trees[i] = new HybridForestNative.Instance { anchor = new Vector3(.499f, .5f, 0), scale = .0001f };
                var position = offscreen ? new Vector3(48, 0, 0) : new Vector3(-15, 0, 10);
                trees[96] = new HybridForestNative.Instance { anchor = new Vector3(.5f + position.x / 1000, .5f + position.z / 1000, 0), scale = 1 };
                var forest = root.AddComponent<InstancedForestRenderer>();
                forest.Initialize(root.transform, new HybridForestData(new[] { prototype }, trees, 1, 1000));
                // Keep the light fixed and capture the reference with casters
                // outside both the view and the live shadow range.
                root.transform.position = Vector3.right * 10000;
                yield return null;
                var unshadowed = fixture.Capture(offscreen ? "offscreen-tree-reference" : "far-tree-reference");
                root.transform.position = Vector3.zero;
                // Graphics.RenderMeshInstanced submissions last for one frame.
                // Flush them before comparing a different caster configuration.
                yield return null;
                var shadowed = fixture.Capture(offscreen ? "offscreen-tree-caster" : "far-tree-caster");
                Assert.That(forest.SubmittedDetailedCrowns, Is.EqualTo(96));
                Assert.That(forest.SubmittedFarShadowTrees, Is.EqualTo(1), "The 97th tree must participate in the actual shadow pass.");
                Assert.That(forest.SubmittedShadowOnlyTrees, Is.EqualTo(offscreen ? 1 : 0));
                var ray = fixture.Sun.transform.forward;
                var receiver = position + ray * (16 / -ray.y); receiver.y = 0;
                var uv = fixture.Camera.WorldToViewportPoint(receiver);
                var pixel = Mathf.FloorToInt(uv.y * 256) * 256 + Mathf.FloorToInt(uv.x * 256);
                Assert.That(unshadowed[pixel].grayscale - shadowed[pixel].grayscale, Is.GreaterThan(.1f),
                    "Visible ground must receive the tree's live shadow without a baked mask.");
            }
            finally { Object.DestroyImmediate(groundMaterial); }
        }

        private static IslandPreparedMesh TreePart(Vector3 size, Vector3 centre)
        {
            var cube = Resources.GetBuiltinResource<Mesh>("Cube.fbx");
            var vertices = cube.vertices;
            for (var i = 0; i < vertices.Length; ++i) vertices[i] = Vector3.Scale(vertices[i], size) + centre;
            return new IslandPreparedMesh(vertices, cube.normals, cube.triangles, cube.uv,
                Array.Empty<Color>(), Array.Empty<Vector2>());
        }

        [TestCase("")]
        [TestCase("MOTU_TERRAIN_LOD1")]
        [TestCase("MOTU_TERRAIN_LOD2")]
        public void ActualTerrainKeepsDistantCanopyShadowsAndFollowsIslandRecentering(string keyword)
        {
            using var fixture = new SceneFixture();
            var material = new Material(Shader.Find("Motu/Terrain Unified"));
            var albedo = Layers(new Color(.5f, .5f, .5f, 1));
            var normal = Layers(new Color(.5f, .5f, 1, .5f));
            var maskLayers = Layers(new Color(.5f, 1, 0, 1));
            using var mask = new ForestShadowMask(ForestShadowTests.Forest(80,
                ForestShadowTests.Tree(.3f, .55f, 2), ForestShadowTests.Tree(.65f, .35f, 2)), material);
            try
            {
                if (keyword.Length != 0) material.EnableKeyword(keyword);
                material.SetTexture("_TerrainAlbedoArray", albedo);
                material.SetTexture("_TerrainNormalArray", normal);
                material.SetTexture("_TerrainMaskArray", maskLayers);
                material.SetFloat("_WorldNormalWeight", 0);
                material.SetFloat("_OcclusionStrength", 0);
                material.SetVector("_TerrainParallaxDepthsA", Vector4.zero);
                material.SetVector("_TerrainParallaxDepthsB", Vector4.zero);
                material.SetFloat("_SnowLine", 1000);
                var ground = fixture.Cube("Terrain", new Vector3(0, -.5f, 0), new Vector3(80, 1, 80));
                ground.GetComponent<MeshRenderer>().sharedMaterial = material;
                material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                fixture.Camera.orthographicSize = 40;
                fixture.Camera.transform.position = new Vector3(0, 15, 0);
                mask.SetVisible(false);
                var clearNear = fixture.Capture();
                mask.SetVisible(true);
                Assert.That(Difference(clearNear, fixture.Capture()), Is.LessThan(.001f), "Inside live shadow coverage the bake must not darken unshadowed ground.");

                var blocker = GameObject.CreatePrimitive(PrimitiveType.Cylinder);
                blocker.transform.SetParent(ground.transform.parent);
                blocker.transform.position = new Vector3(-10, 5, 6);
                blocker.transform.localScale = new Vector3(16, 4, 16);
                blocker.layer = 31;
                blocker.GetComponent<MeshRenderer>().shadowCastingMode = ShadowCastingMode.ShadowsOnly;
                fixture.Camera.transform.position = new Vector3(0, 40, 0);
                var centre = fixture.Camera.WorldToViewportPoint(new Vector3(-10, 0, 6));
                var pixel = Mathf.FloorToInt(centre.y * 256) * 256 + Mathf.FloorToInt(centre.x * 256);
                foreach (var height in new[] { 40f, 90f, 97f, 160f })
                {
                    fixture.Camera.transform.position = new Vector3(0, height, 0);
                    var handoff = fixture.Capture();
                    Assert.That(handoff[pixel].grayscale, Is.LessThan(clearNear[pixel].grayscale * .4f),
                        "The canopy centre must remain shadowed through the live-to-baked handoff at height " + height);
                }

                fixture.Camera.transform.position = new Vector3(0, 160, 0);
                mask.SetVisible(false);
                var clear = fixture.Capture();
                mask.SetVisible(true);
                var distant = fixture.Capture("ground-" + (keyword.Length == 0 ? "lod0" : keyword.ToLowerInvariant()));
                Assert.That(Difference(clear, distant) / Mean(clear), Is.GreaterThan(.04f), "Canopy shadows must be visible beyond the live shadow range.");
                Assert.That(Mean(clear), Is.GreaterThan(.02f), "The reference ground must actually be lit.");
                fixture.Sun.intensity = 0;
                mask.SetVisible(false);
                var ambient = fixture.Capture();
                mask.SetVisible(true);
                Assert.That(Difference(ambient, fixture.Capture()), Is.LessThan(.001f), "The canopy bake must preserve ambient ground lighting.");
                fixture.Sun.intensity = 1.5f;

                // The same island-local bake must move/rotate with the island,
                // including large floating-origin offsets, without rebaking.
                var island = Matrix4x4.TRS(new Vector3(8000, 0, -6000), Quaternion.Euler(0, 90, 0), Vector3.one);
                ground.transform.position = island.MultiplyPoint3x4(new Vector3(0, -.5f, 0));
                ground.transform.rotation = Quaternion.Euler(0, 90, 0);
                material.SetMatrix("_IslandWorldToLocal", island.inverse);
                fixture.Camera.transform.position = island.MultiplyPoint3x4(new Vector3(0, 160, 0));
                fixture.Camera.transform.rotation = Quaternion.Euler(0, 90, 0) * Quaternion.Euler(90, 0, 0);
                var recentered = fixture.Capture();
                mask.SetVisible(false);
                var recenteredClear = fixture.Capture();
                // Fine terrain's normal/specular shading can change with its
                // orientation. Compare the actual shadow footprint, not its albedo.
                var mismatches = 0;
                for (var i = 0; i < clear.Length; ++i)
                    if (IsShadow(clear[i], distant[i]) != IsShadow(recenteredClear[i], recentered[i])) ++mismatches;
                Assert.That(mismatches, Is.LessThan(clear.Length / 200), "The mask must remain attached to transformed island terrain.");
            }
            finally
            {
                mask.Dispose(); Object.DestroyImmediate(material);
                Object.DestroyImmediate(albedo); Object.DestroyImmediate(normal); Object.DestroyImmediate(maskLayers);
            }
        }

        [Test]
        public void ActualDistantTreeShaderKeepsFoliageLitWhileWoodReceivesOnlyAmbient()
        {
            using var fixture = new SceneFixture();
            using var bark = new PlanetForestBark();
            var material = new Material(Shader.Find("Motu/Planet Forest"));
            var noise = new Texture3D(1, 1, 1, TextureFormat.RGBA32, false);
            var woodMesh = Object.Instantiate(Resources.GetBuiltinResource<Mesh>("Cube.fbx"));
            var leafMesh = Object.Instantiate(woodMesh);
            try
            {
                noise.SetPixel(0, 0, 0, Color.gray); noise.Apply();
                material.SetTexture("_ForestNoise", noise);
                material.SetFloat("_ForestKind", 3);
                bark.Bind(material);
                SetSurface(woodMesh, 0); SetSurface(leafMesh, 1);
                var wood = fixture.Cube("Distant wood", new Vector3(-2, 0, 0), Vector3.one * 2);
                wood.GetComponent<MeshFilter>().sharedMesh = woodMesh;
                wood.GetComponent<MeshRenderer>().sharedMaterial = material;
                wood.GetComponent<MeshRenderer>().shadowCastingMode = ShadowCastingMode.Off;
                var leaves = fixture.Cube("Distant foliage", new Vector3(2, 0, 0), Vector3.one * 2);
                leaves.GetComponent<MeshFilter>().sharedMesh = leafMesh;
                leaves.GetComponent<MeshRenderer>().sharedMaterial = material;
                leaves.GetComponent<MeshRenderer>().shadowCastingMode = ShadowCastingMode.Off;
                fixture.Camera.orthographicSize = 4;
                fixture.Camera.transform.position = new Vector3(0, 10, 0);
                material.SetFloat("_MotuForestAmbientWood", 0);
                var lit = fixture.Capture();
                material.SetFloat("_MotuForestAmbientWood", 1);
                var shaded = fixture.Capture("distant-wood-ambient");
                Assert.That(HalfMean(shaded, false), Is.LessThan(HalfMean(lit, false) * .7f), "Unshadowed direct light must no longer light the wood.");
                Assert.That(HalfMean(shaded, true), Is.EqualTo(HalfMean(lit, true)).Within(.002f), "The combined mesh's foliage must retain direct light.");
                var previous = RenderSettings.ambientLight;
                RenderSettings.ambientLight = Color.black;
                var dark = fixture.Capture();
                RenderSettings.ambientLight = previous;
                Assert.That(HalfMean(dark, false), Is.LessThan(HalfMean(shaded, false)), "Wood must still respond to ambient conditions.");
            }
            finally { Object.DestroyImmediate(material); Object.DestroyImmediate(noise); Object.DestroyImmediate(woodMesh); Object.DestroyImmediate(leafMesh); }
        }

        private static void SetSurface(Mesh mesh, float foliage)
        {
            var colours = new Color[mesh.vertexCount];
            Array.Fill(colours, new Color(.5f, foliage, 1, 1));
            mesh.colors = colours;
        }

        private static Texture2DArray Layers(Color colour)
        {
            var array = new Texture2DArray(1, 1, 6, TextureFormat.RGBA32, false, true);
            for (var i = 0; i < 6; ++i) array.SetPixels(new[] { colour }, i);
            array.Apply(); return array;
        }

        private static float Mean(Color[] pixels)
        {
            float sum = 0; foreach (var pixel in pixels) sum += pixel.grayscale;
            return sum / pixels.Length;
        }

        private static float Difference(Color[] first, Color[] second)
        {
            float sum = 0;
            for (var i = 0; i < first.Length; ++i) sum += Mathf.Abs(first[i].grayscale - second[i].grayscale);
            return sum / first.Length;
        }

        private static bool IsShadow(Color clear, Color shadow) => clear.grayscale - shadow.grayscale > clear.grayscale * .25f;

        private static float HalfMean(Color[] pixels, bool right)
        {
            float sum = 0;
            for (var y = 0; y < 256; ++y)
                for (var x = right ? 128 : 0; x < (right ? 256 : 128); ++x) sum += pixels[y * 256 + x].grayscale;
            return sum / (128 * 256);
        }

        private sealed class SceneFixture : IDisposable
        {
            private readonly EnvironmentHostState previous = new EnvironmentHostState();
            private readonly GameObject root = new GameObject("Distant forest shadow validation");
            private readonly UniversalRenderPipelineAsset pipeline;
            private readonly float shadowDistance;
            internal readonly Camera Camera;
            internal readonly Light Sun;

            internal SceneFixture()
            {
                // Render requests can start with an authored scene still open.
                // Its lights must not turn an ambient-only probe into a moon/
                // additional-light probe when this fixture switches its sun off.
                foreach (var light in Object.FindObjectsByType<Light>())
                {
                    previous.BorrowLight(light);
                    light.enabled = false;
                }
                pipeline = GraphicsSettings.currentRenderPipeline as UniversalRenderPipelineAsset;
                Assert.That(pipeline, Is.Not.Null);
                shadowDistance = pipeline.shadowDistance; pipeline.shadowDistance = 100;
                RenderSettings.fog = false;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = new Color(.12f, .12f, .12f);
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_MotuCloudLightActive", 0);
                Shader.SetGlobalFloat("_MotuCloudSecondaryLightActive", 0);
                var sun = new GameObject("Sun").AddComponent<Light>(); sun.transform.SetParent(root.transform);
                Sun = sun;
                sun.type = LightType.Directional; sun.intensity = 1.5f; sun.shadows = LightShadows.Hard;
                sun.transform.rotation = Quaternion.Euler(90, 0, 0); RenderSettings.sun = sun;
                Camera = new GameObject("Shadow camera").AddComponent<Camera>(); Camera.transform.SetParent(root.transform);
                Camera.enabled = false; Camera.orthographic = true; Camera.transform.rotation = Quaternion.Euler(90, 0, 0);
                Camera.aspect = 1; // Match the square target when projecting receiver sample positions.
                Camera.clearFlags = CameraClearFlags.SolidColor; Camera.backgroundColor = Color.black;
                Camera.cullingMask = 1 << 31; Camera.nearClipPlane = .1f; Camera.farClipPlane = 500;
                Camera.allowHDR = false; Camera.allowMSAA = false;
                Camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
            }

            internal GameObject Cube(string name, Vector3 position, Vector3 scale)
            {
                var item = GameObject.CreatePrimitive(PrimitiveType.Cube);
                item.name = name; item.transform.SetParent(root.transform);
                item.transform.position = position; item.transform.localScale = scale; item.layer = 31;
                return item;
            }

            internal Color[] Capture(string name = null)
            {
                var target = new RenderTexture(256, 256, 24);
                var image = new Texture2D(256, 256, TextureFormat.RGBA32, false);
                var oldTarget = RenderTexture.active;
                try
                {
                    RenderPipeline.SubmitRenderRequest(Camera, new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                    RenderTexture.active = target; image.ReadPixels(new Rect(0, 0, 256, 256), 0, 0); image.Apply();
                    if (name != null)
                    {
                        var directory = Path.Combine(Path.GetTempPath(), "motu-forest-shadows"); Directory.CreateDirectory(directory);
                        File.WriteAllBytes(Path.Combine(directory, name + ".png"), image.EncodeToPNG());
                    }
                    return image.GetPixels();
                }
                finally { RenderTexture.active = oldTarget; Object.DestroyImmediate(target); Object.DestroyImmediate(image); }
            }

            public void Dispose()
            {
                pipeline.shadowDistance = shadowDistance;
                Object.DestroyImmediate(root); previous.Restore();
            }
        }
    }
}
