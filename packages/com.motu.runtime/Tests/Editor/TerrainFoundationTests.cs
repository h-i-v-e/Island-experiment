using System;
using System.Collections;
using System.IO;
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
    public sealed class TerrainFoundationTests
    {
        private sealed class PreparedTerrain : ITerrainFootprintQuery, IDisposable
        {
            private readonly GameObject root = new GameObject("Foundation CPU terrain");
            private readonly TerrainTileStreamer streamer;
            internal TerrainFootprint LastFootprint;

            internal PreparedTerrain(Func<float, float, float> height, int tileSamples = 2)
            {
                root.SetActive(false);
                streamer = root.AddComponent<TerrainTileStreamer>();
                streamer.worldSize = 64;
                streamer.colliderHeightMap = TerrainFootprintTests.MakeMap(64, tileSamples, height);
            }

            public TerrainFootprintQueryStatus QueryTerrainFootprint(TerrainFootprint footprint,
                out TerrainFootprintSample sample, int maximumCellTests = 262144)
            {
                LastFootprint = footprint;
                return streamer.QueryTerrainFootprint(footprint, out sample, maximumCellTests);
            }

            public void Dispose() { streamer.Dispose(); Object.DestroyImmediate(root); }
        }

        private GameObject root;
        private TerrainFoundation foundation;

        [SetUp]
        public void SetUp()
        {
            root = new GameObject("Runtime building group") { layer = 31 };
            foundation = root.AddComponent<TerrainFoundation>();
        }

        [TearDown]
        public void TearDown() => Object.DestroyImmediate(root);

        private GameObject Building(Vector3 centre, Vector3 size)
        {
            var building = GameObject.CreatePrimitive(PrimitiveType.Cube);
            building.name = "Building"; building.layer = 31;
            building.transform.SetParent(root.transform, false);
            building.transform.localPosition = centre;
            building.transform.localScale = size;
            return building;
        }

        [Test]
        public void UsesTheEntireTerrainRangeAndPlacesTheGroupOnTheHighestPoint()
        {
            var building = Building(new Vector3(0, 2, 0), new Vector3(8, 4, 6));
            using var terrain = new PreparedTerrain((x, z) => 12 + x * .2f + z * .1f);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            var sample = foundation.GroundSample;
            var bounds = foundation.Platform.GetComponent<Renderer>().bounds;
            Assert.That(bounds.max.y, Is.EqualTo(sample.MaximumHeight).Within(.0001f));
            Assert.That(bounds.min.y, Is.LessThanOrEqualTo(sample.MinimumHeight));
            Assert.That(building.GetComponent<Renderer>().bounds.min.y, Is.EqualTo(bounds.max.y).Within(.0001f));
            Assert.That(bounds.size.x, Is.EqualTo(9.6f).Within(.0001f));
            Assert.That(bounds.size.z, Is.EqualTo(7.2f).Within(.0001f));
            Assert.That(sample.HeightRange, Is.GreaterThan(2));
            Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.Ready));
        }

        [TestCase(0f, 1f, 1f, 1.5f)]
        [TestCase(37f, 2f, .7f, 1.5f)]
        [TestCase(123f, -2f, .7f, 1.5f)]
        [TestCase(70f, .01f, .01f, .01f)]
        public void RotatedScaledGroupsFitTheirFullFootprintAndHaveALevelCollider(float yaw, float scaleX, float scaleZ, float scaleY)
        {
            root.transform.SetPositionAndRotation(new Vector3(8, 100, -5), Quaternion.Euler(0, yaw, 0));
            root.transform.localScale = new Vector3(scaleX, scaleY, scaleZ);
            Building(new Vector3(-2, 1, 0), new Vector3(4, 2, 4));
            Building(new Vector3(4, 2, 1), new Vector3(2, 4, 6));
            using var terrain = new PreparedTerrain((x, z) => 12 + x * .1f);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            Assert.That(foundation.Footprint.Size.x, Is.GreaterThanOrEqualTo(9 * Mathf.Abs(scaleX) + .5f - .001f));
            Assert.That(foundation.Footprint.Size.y, Is.GreaterThanOrEqualTo(6 * scaleZ + .5f - .001f));
            Assert.That(foundation.Footprint.Size.x / 2.4f, Is.EqualTo(Mathf.Round(foundation.Footprint.Size.x / 2.4f)).Within(.0001f));
            Assert.That(foundation.Footprint.Size.y / 2.4f, Is.EqualTo(Mathf.Round(foundation.Footprint.Size.y / 2.4f)).Within(.0001f));
            AssertLevelTopAndCollider();
        }

        [Test]
        public void TiltedPropsUnderAScaledParentStillGetAWorldLevelFoundation()
        {
            var parent = new GameObject("Scaled placement parent");
            try
            {
                parent.transform.localScale = new Vector3(2, 1, .7f);
                root.transform.SetParent(parent.transform, false);
                root.transform.localRotation = Quaternion.Euler(8, 37, 3);
                Building(new Vector3(0, 2, 0), new Vector3(4, 4, 3));
                using var terrain = new PreparedTerrain((x, z) => 12 + x * .1f);
                Assert.That(foundation.TryBuild(terrain), Is.True);
                AssertLevelTopAndCollider();
            }
            finally { root.transform.SetParent(null, true); Object.DestroyImmediate(parent); }
        }

        private void AssertLevelTopAndCollider()
        {
            var filter = foundation.Platform.GetComponent<MeshFilter>();
            var vertices = filter.sharedMesh.vertices;
            for (var i = 0; i < 4; i++)
                Assert.That(filter.transform.TransformPoint(vertices[i]).y,
                    Is.EqualTo(foundation.GroundSample.MaximumHeight).Within(.001f));
            Physics.SyncTransforms();
            var centre = foundation.Footprint.Center;
            var ray = new Ray(new Vector3(centre.x, foundation.GroundSample.MaximumHeight + 5, centre.y), Vector3.down);
            Assert.That(foundation.Platform.GetComponent<MeshCollider>().Raycast(ray, out var hit, 10), Is.True);
            Assert.That(hit.point.y, Is.EqualTo(foundation.GroundSample.MaximumHeight).Within(.001f));
            Assert.That(Vector3.Dot(hit.normal, Vector3.up), Is.GreaterThan(.999f));
        }

        [Test]
        public void RebuildReusesTheBaseAndCannotIncludeItInTheBuildingBounds()
        {
            Building(Vector3.up * 2, new Vector3(6, 4, 5));
            using var terrain = new PreparedTerrain((x, z) => 12 + x * .3f);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            var platform = foundation.Platform;
            var mesh = platform.GetComponent<MeshFilter>().sharedMesh;
            var position = root.transform.position;
            var footprint = foundation.Footprint;
            for (var i = 0; i < 4; i++) Assert.That(foundation.TryBuild(terrain), Is.True);
            Assert.That(foundation.Platform, Is.SameAs(platform));
            Assert.That(platform.GetComponent<MeshFilter>().sharedMesh, Is.SameAs(mesh));
            Assert.That(root.transform.position, Is.EqualTo(position));
            Assert.That(foundation.Footprint.Size, Is.EqualTo(footprint.Size));
            Assert.That(root.GetComponentsInChildren<Renderer>().Length, Is.EqualTo(2));
        }

        [Test]
        public void InteriorTerrainSpikeDeterminesTheTopEvenWhenTheCornersAreLow()
        {
            Building(Vector3.up * 2, new Vector3(8, 4, 8));
            using var terrain = new PreparedTerrain((x, z) => x == 0 && z == 0 ? 19 : 10);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            Assert.That(foundation.Platform.GetComponent<Renderer>().bounds.max.y, Is.EqualTo(19).Within(.001f));
            Assert.That(root.transform.position.y, Is.EqualTo(19).Within(.001f));
        }

        [Test]
        public void FlatGroundHasAMinimumThicknessWithoutRaisingTheTop()
        {
            Building(Vector3.up * 2, new Vector3(8, 4, 8));
            using var terrain = new PreparedTerrain((x, z) => 10);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            var bounds = foundation.Platform.GetComponent<Renderer>().bounds;
            Assert.That(bounds.max.y, Is.EqualTo(10).Within(.001f));
            Assert.That(bounds.size.y, Is.EqualTo(1.2f).Within(.001f));
        }

        [Test]
        public void IncompleteAndSubmergedTerrainNeverMoveTheBuildingOrInventABase()
        {
            Building(Vector3.up * 2, new Vector3(8, 4, 8));
            root.transform.position = Vector3.right * 31;
            var before = root.transform.position;
            using var terrain = new PreparedTerrain((x, z) => 10);
            Assert.That(foundation.TryBuild(terrain), Is.False);
            Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.WaitingForTerrain));
            Assert.That(foundation.Platform, Is.Null);
            Assert.That(root.transform.position, Is.EqualTo(before));
            root.transform.position = Vector3.zero;
            using var submerged = new PreparedTerrain((x, z) => -3);
            Assert.That(foundation.TryBuild(submerged), Is.False);
            Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.UnsuitableTerrain));
            Assert.That(root.transform.position, Is.EqualTo(Vector3.zero));
        }

        [Test]
        public void ColliderOnlyPropsAndOptionalPositioningAreSupported()
        {
            var box = root.AddComponent<BoxCollider>(); box.center = Vector3.up * 2; box.size = new Vector3(6, 4, 5);
            foundation.AlignObjectToPlatform = false;
            using var terrain = new PreparedTerrain((x, z) => 10);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            Assert.That(root.transform.position, Is.EqualTo(Vector3.zero));
            Assert.That(foundation.Footprint.Size.x, Is.EqualTo(7.2f).Within(.001f));
            Assert.That(foundation.Footprint.Size.y, Is.EqualTo(7.2f).Within(.001f));
        }

        [TestCase(2.4f, 1.2f)]
        [TestCase(1.5f, 1f)]
        [TestCase(1.2f, 1.8f)]
        public void EveryFaceHasWholeTextureRepeatsAndExtraDepthGoesUnderground(float width, float height)
        {
            Building(Vector3.up * 2, new Vector3(8, 4, 6));
            foundation.TextureTileSizeMetres = new Vector2(width, height);
            using var terrain = new PreparedTerrain((x, z) => 12 + x * .2f);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            Assert.That(terrain.LastFootprint.Size, Is.EqualTo(foundation.Footprint.Size));
            var mesh = foundation.Platform.GetComponent<MeshFilter>().sharedMesh;
            foreach (var uv in mesh.uv)
            {
                Assert.That(uv.x, Is.EqualTo(Mathf.Round(uv.x)).Within(.0001f));
                Assert.That(uv.y, Is.EqualTo(Mathf.Round(uv.y)).Within(.0001f));
            }
            var bounds = foundation.Platform.GetComponent<Renderer>().bounds;
            Assert.That(bounds.max.y, Is.EqualTo(foundation.GroundSample.MaximumHeight).Within(.001f));
            Assert.That(bounds.min.y, Is.LessThanOrEqualTo(foundation.GroundSample.MinimumHeight));
            Assert.That(foundation.Footprint.Size.x, Is.GreaterThanOrEqualTo(8.5f));
            Assert.That(foundation.Footprint.Size.y, Is.GreaterThanOrEqualTo(6.5f));
        }

        [Test]
        public void RoundedFootprintIsProbedBeforeDecidingThePlatformTop()
        {
            Building(Vector3.up * 2, new Vector3(7, 4, 4));
            using var terrain = new PreparedTerrain((x, z) => x == 4.5f && z == 0 ? 23 : 10, 3);
            terrain.QueryTerrainFootprint(new TerrainFootprint(Vector2.zero, new Vector2(7.5f, 4.5f)), out var unrounded);
            Assert.That(unrounded.MaximumHeight, Is.EqualTo(10), "The peak starts outside the original padded footprint.");
            Assert.That(foundation.TryBuild(terrain), Is.True);
            Assert.That(foundation.Footprint.Size.x, Is.EqualTo(9.6f).Within(.001f));
            Assert.That(foundation.Platform.GetComponent<Renderer>().bounds.max.y, Is.EqualTo(23).Within(.001f));
        }

        [Test]
        public void TextureScaleIsMeasuredInWorldMetresEvenOnScaledObjects()
        {
            root.transform.localScale = new Vector3(2, 3, .5f);
            Building(Vector3.up * 2, new Vector3(4, 4, 4));
            using var terrain = new PreparedTerrain((x, z) => 10 + x * .2f);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            var filter = foundation.Platform.GetComponent<MeshFilter>();
            var mesh = filter.sharedMesh; var vertices = mesh.vertices; var uv = mesh.uv;
            for (var face = 0; face < 6; face++)
            {
                var i = face * 4;
                var horizontal = Vector3.Distance(filter.transform.TransformPoint(vertices[i]), filter.transform.TransformPoint(vertices[i + 1]));
                var vertical = Vector3.Distance(filter.transform.TransformPoint(vertices[i + 1]), filter.transform.TransformPoint(vertices[i + 2]));
                Assert.That((uv[i + 1].x - uv[i].x) * foundation.TextureTileSizeMetres.x, Is.EqualTo(horizontal).Within(.001f));
                Assert.That((uv[i + 2].y - uv[i + 1].y) * foundation.TextureTileSizeMetres.y, Is.EqualTo(vertical).Within(.001f));
            }
            var material = foundation.Platform.GetComponent<Renderer>().sharedMaterial;
            Assert.That(material, Is.SameAs(Resources.Load<Material>("Motu/FoundationStonework")));
            Assert.That(material.GetTexture("_BaseMap"), Is.Not.Null);
            Assert.That(material.GetTexture("_BumpMap"), Is.Not.Null);
            Assert.That(material.GetTexture("_OcclusionMap"), Is.Not.Null);
        }

        [Test]
        public void MissingBoundsOrInvalidScaleCannotBuild()
        {
            using var terrain = new PreparedTerrain((x, z) => 10);
            Assert.That(foundation.TryBuild(terrain), Is.False);
            Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.MissingBounds));
            Building(Vector3.up, Vector3.one * 2);
            root.transform.localScale = new Vector3(0, 1, 1);
            Assert.That(foundation.TryBuild(terrain), Is.False);
            Assert.That(foundation.Status, Is.EqualTo(TerrainFoundationStatus.InvalidConfiguration));
            Assert.That(foundation.Platform, Is.Null);
        }

        [UnityTest]
        public IEnumerator RendersTheBakedStoneworkOnALevelBaseBelowSeveralBuildings()
        {
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset)) Assert.Ignore("URP is required.");
            Building(new Vector3(-2, 2, 0), new Vector3(3, 4, 4));
            Building(new Vector3(2, 1.5f, 1), new Vector3(3, 3, 3));
            using var terrain = new PreparedTerrain((x, z) => 12 + x * .3f + z * .1f);
            Assert.That(foundation.TryBuild(terrain), Is.True);
            var cameraObject = new GameObject("Foundation acceptance camera");
            var lightObject = new GameObject("Foundation acceptance sun");
            var lights = Object.FindObjectsByType<Light>();
            var enabledLights = new bool[lights.Length];
            for (var i = 0; i < lights.Length; i++) { enabledLights[i] = lights[i].enabled; lights[i].enabled = false; }
            var ambientMode = RenderSettings.ambientMode;
            var ambientLight = RenderSettings.ambientLight;
            RenderSettings.ambientMode = AmbientMode.Flat;
            RenderSettings.ambientLight = new Color(.18f, .18f, .18f);
            var groundObject = new GameObject("Sloped terrain acceptance surface") { layer = 31 };
            var groundMesh = new Mesh();
            var groundMaterial = new Material(Shader.Find("Universal Render Pipeline/Lit"));
            groundMaterial.SetColor("_BaseColor", new Color(.15f, .23f, .11f));
            groundMaterial.SetFloat("_Smoothness", 0);
            groundMesh.vertices = new[] { new Vector3(-16, 5.6f, -16), new Vector3(-16, 8.8f, 16),
                new Vector3(16, 18.4f, 16), new Vector3(16, 15.2f, -16) };
            groundMesh.triangles = new[] { 0, 1, 2, 0, 2, 3 }; groundMesh.RecalculateNormals();
            groundObject.AddComponent<MeshFilter>().sharedMesh = groundMesh;
            groundObject.AddComponent<MeshRenderer>().sharedMaterial = groundMaterial;
            var target = new RenderTexture(960, 640, 24);
            var image = new Texture2D(960, 640, TextureFormat.RGB24, false);
            var previous = RenderTexture.active;
            try
            {
                var light = lightObject.AddComponent<Light>(); light.type = LightType.Directional; light.intensity = .9f;
                light.transform.rotation = Quaternion.Euler(42, -35, 0);
                var camera = cameraObject.AddComponent<Camera>(); camera.enabled = false;
                camera.cullingMask = 1 << 31; camera.targetTexture = target;
                camera.clearFlags = CameraClearFlags.SolidColor; camera.backgroundColor = new Color(.22f, .3f, .36f);
                camera.transform.position = new Vector3(-12, 22, -16);
                camera.transform.LookAt(new Vector3(0, foundation.GroundSample.MaximumHeight, 0));
                camera.fieldOfView = 35;
                camera.GetUniversalAdditionalCameraData().renderPostProcessing = false;
                yield return null;
                RenderPipeline.SubmitRenderRequest(camera, new UniversalRenderPipeline.SingleCameraRequest { destination = target });
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, 960, 640), 0, 0); image.Apply();
                var directory = Path.Combine(Path.GetTempPath(), "motu-terrain-foundation"); Directory.CreateDirectory(directory);
                File.WriteAllBytes(Path.Combine(directory, "foundation.png"), image.EncodeToPNG());
                var pixels = image.GetPixels32(); var changed = 0;
                for (var i = 1; i < pixels.Length; i++) if (!pixels[i].Equals(pixels[0])) changed++;
                Assert.That(changed, Is.GreaterThan(5000), "The actual camera must draw the foundation and its buildings.");
            }
            finally
            {
                RenderTexture.active = previous;
                RenderSettings.ambientMode = ambientMode; RenderSettings.ambientLight = ambientLight;
                for (var i = 0; i < lights.Length; i++) if (lights[i] != null) lights[i].enabled = enabledLights[i];
                Object.DestroyImmediate(cameraObject); Object.DestroyImmediate(lightObject);
                Object.DestroyImmediate(groundObject); Object.DestroyImmediate(groundMesh); Object.DestroyImmediate(groundMaterial);
                Object.DestroyImmediate(target); Object.DestroyImmediate(image);
            }
        }
    }
}
