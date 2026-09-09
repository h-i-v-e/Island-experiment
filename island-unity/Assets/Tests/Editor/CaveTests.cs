using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using Motu.Interop;
using Motu.Rendering;
using Motu.Islands;
using Motu.Settings;
using Motu.Streaming;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class CaveTests
    {
        [Serializable] private sealed class PackedMesh { public float[] vertices, normals; public int[] triangles; public bool collisionOnly; }
        [Serializable] private sealed class Fixture
        {
            public float[] minimum, maximum, entrance, chamber;
            public PackedMesh[] chunks;
            public PackedMesh terrain;
        }

        [Test]
        public void SettingsCopyAndNativeLayoutAreStable()
        {
            Assert.AreEqual(132, Marshal.SizeOf<CaveNative.Options>());
            Assert.AreEqual(64, Marshal.SizeOf<CaveNative.Info>());
            Assert.AreEqual(28, Marshal.SizeOf<CaveNative.Stats>());
            Assert.AreEqual(CaveNative.AlgorithmRevision, CaveNative.CaveAlgorithmRevision(),
                "The native cave plugin must match the revision used in snapshot cache keys.");
            var original = new IslandCaveSettings { Enabled = true, EntranceWidth = 5f };
            var copy = original.Copy();
            copy.EntranceWidth = 8f;
            Assert.AreEqual(5f, original.EntranceWidth);
            Assert.AreEqual(8f, copy.ToNative().entranceWidth);
            Assert.AreEqual(1u, copy.ToNative().enabled);
            using var a = new MemoryStream();
            using var b = new MemoryStream();
            using (var writer = new BinaryWriter(a, System.Text.Encoding.UTF8, true)) original.ToNative().Write(writer);
            using (var writer = new BinaryWriter(b, System.Text.Encoding.UTF8, true)) copy.ToNative().Write(writer);
            Assert.AreEqual(132, a.Length);
            CollectionAssert.AreNotEqual(a.ToArray(), b.ToArray());
        }

        [Test]
        public void NativeCaveHasWalkableEntranceRoofAndBoundedInteriorQueries()
        {
            var fixturePath = Environment.GetEnvironmentVariable("MOTU_CAVE_FIXTURE_OUTPUT");
            if (string.IsNullOrEmpty(fixturePath))
                Assert.Ignore("Generate the Rust cave fixture and set MOTU_CAVE_FIXTURE_OUTPUT to run native geometry physics checks.");
            Assert.IsTrue(File.Exists(fixturePath), fixturePath);
            var fixture = JsonUtility.FromJson<Fixture>(File.ReadAllText(fixturePath));
            var root = new GameObject("Native cave physics fixture");
            var dataObjects = new List<TerrainData>();
            var extraMeshes = new List<Mesh>();
            var terrainMaterial = new Material(Shader.Find("Motu/Cave Surface"));
            try
            {
                var prepared = Prepare(fixture);
                var caves = root.AddComponent<CaveStreamer>();
                caves.InitializeAsync(prepared, terrainMaterial, CancellationToken.None,
                    new UnityFrameBudget(100000)).GetAwaiter().GetResult();
                Assert.AreEqual(1, caves.CaveCount);
                for (var y = 31; y <= 32; y++) for (var x = 31; x <= 33; x++)
                {
                    var data = new TerrainData { heightmapResolution = 129, size = new Vector3(31.25f, 64, 31.25f) };
                    dataObjects.Add(data);
                    var heights = new float[129, 129];
                    for (var v = 0; v < 129; v++) for (var u = 0; u < 129; u++)
                    {
                        var px = (x + u / 128f) * 31.25f - 1000f;
                        heights[v, u] = (10f + Mathf.Clamp01(px / 4f) * 30f) / 64f;
                    }
                    data.SetHeights(0, 0, heights);

                    var tile = new GameObject("Hidden terrain collider");
                    tile.transform.SetParent(root.transform, false);
                    tile.transform.localPosition = new Vector3(x * 31.25f - 1000, 0, y * 31.25f - 1000);
                    tile.AddComponent<TerrainCollider>().terrainData = data;
                }
                Physics.SyncTransforms();
                Assert.IsTrue(caves.TryFindGround(new Vector3(12, 10, 0), 2, 3, out var floor));
                Assert.That(floor.y, Is.EqualTo(10).Within(.15f));
                Assert.IsFalse(caves.TryFindGround(new Vector3(12, 40, 0), 2, 3, out _), "Caves must not duplicate the terrain roof.");
                Assert.IsTrue(caves.TryGetChamber(0, out var chamber));
                Assert.That(chamber.y, Is.EqualTo(10).Within(.15f));

                var player = new GameObject("Walking capsule");
                player.transform.SetParent(root.transform, false);
                player.transform.localPosition = new Vector3(-2, 10.05f, 0);
                var controller = player.AddComponent<CharacterController>();
                controller.height = 1.6f; controller.radius = .3f;
                controller.center = Vector3.up * .8f;
                controller.stepOffset = .35f; controller.skinWidth = .05f;
                Physics.SyncTransforms();
                using var collision = new CaveCollisionScope(controller);
                for (var i = 0; i < 240; i++)
                {
                    collision.Update(player.transform.position, true);
                    controller.Move(new Vector3(.05f, -.03f, 0));
                }
                Assert.IsTrue(collision.IsInside);
                foreach (var exterior in root.GetComponentsInChildren<TerrainCollider>()) Assert.IsFalse(exterior.enabled);
                var streamed = new GameObject("New exterior collider");
                streamed.transform.SetParent(root.transform, false);
                var streamedCollider = streamed.AddComponent<BoxCollider>();
                CaveCollisionScope.RegisterExteriorCollider(streamedCollider);
                Assert.IsFalse(streamedCollider.enabled);
                var alreadyDisabled = new GameObject("Already disabled collider");
                alreadyDisabled.transform.SetParent(root.transform, false);
                var disabledCollider = alreadyDisabled.AddComponent<BoxCollider>();
                disabledCollider.enabled = false;
                Assert.That(player.transform.position.x, Is.GreaterThan(9f), "Entrance wall blocked the player.");
                Assert.That(player.transform.position.y, Is.InRange(9.8f, 10.3f), "Floor gap or heightfield ejection.");
                collision.Update(player.transform.position, false); // Flight/teleport restores exterior collision.
                Assert.IsFalse(collision.IsInside);
                Assert.IsTrue(streamedCollider.enabled);
                Assert.IsFalse(disabledCollider.enabled);
                foreach (var exterior in root.GetComponentsInChildren<TerrainCollider>()) Assert.IsTrue(exterior.enabled);
                collision.Update(player.transform.position, true);
                Assert.IsTrue(collision.IsInside);
                for (var i=0; i<290; i++)
                {
                    collision.Update(player.transform.position, true);
                    controller.Move(new Vector3(-.05f,-.03f,0));
                }
                collision.Update(player.transform.position, true);
                Assert.IsFalse(collision.IsInside, "Walking out must restore exterior collision.");
                Assert.That(player.transform.position.y, Is.InRange(9.8f,10.3f));
                Object.DestroyImmediate(streamed);
                Object.DestroyImmediate(alreadyDisabled);
                controller.enabled = false;
                player.transform.position = new Vector3(10, 40.05f, 0);
                controller.enabled = true;
                for (var i = 0; i < 100; i++)
                {
                    collision.Update(player.transform.position, true);
                    Assert.IsFalse(collision.IsInside, "Walking above a cave must retain terrain collision.");
                    controller.Move(new Vector3(.05f, -.03f, 0));
                }
                Assert.That(player.transform.position.y, Is.InRange(39.8f, 40.3f), "Original terrain roof is not walkable.");

                var screenshotDirectory = Environment.GetEnvironmentVariable("MOTU_CAVE_SCREENSHOT_DIR");
                if (!string.IsNullOrEmpty(screenshotDirectory))
                {
                    var surface = new GameObject("Surrounding terrain");
                    surface.transform.SetParent(root.transform, false);
                    var terrainMesh = IslandMeshInterop.CreateGeneratedMesh(Unpack(fixture.terrain));
                    extraMeshes.Add(terrainMesh);
                    surface.AddComponent<MeshFilter>().sharedMesh = terrainMesh;
                    surface.AddComponent<MeshRenderer>().sharedMaterial = terrainMaterial;
                    Capture(root.transform, screenshotDirectory);
                }
                root.transform.position = new Vector3(4000, 0, -6000);
                Physics.SyncTransforms();
                Assert.IsTrue(caves.TryFindGround(root.transform.TransformPoint(new Vector3(12, 10, 0)), 2, 3, out floor));
                Assert.That(floor.y, Is.EqualTo(10).Within(.15f));
                caves.Dispose();
                Assert.IsFalse(caves.TryFindGround(floor, 2, 3, out _));
            }
            finally
            {
                Object.DestroyImmediate(root);
                foreach (var data in dataObjects) Object.DestroyImmediate(data);
                foreach (var mesh in extraMeshes) Object.DestroyImmediate(mesh);
                Object.DestroyImmediate(terrainMaterial);
            }
        }

        [Test]
        public void GeneratedCaveSurvivesNativeSnapshotAndUnityCollision()
        {
            var path = Environment.GetEnvironmentVariable("MOTU_CAVE_SNAPSHOT");
            if (string.IsNullOrEmpty(path)) Assert.Ignore("Set MOTU_CAVE_SNAPSHOT to a cave probe snapshot containing a cave.");
            var handle = MotuNative.LoadMotuSnapshot(path, out var status);
            Assert.AreEqual(0, status);
            Assert.AreNotEqual(IntPtr.Zero, handle);
            var roundTrip = Path.Combine(Path.GetTempPath(), Guid.NewGuid() + ".motusnapshot");
            var root = new GameObject("Generated cave collision");
            var terrainData = new List<TerrainData>();
            var material = new Material(Shader.Find("Motu/Cave Surface"));
            try
            {
                var prepared = CavePreparation.Prepare(handle, 2000, CancellationToken.None);
                Assert.Greater(prepared.caves.Length, 0);
                Assert.AreEqual(0, MotuNative.SaveMotuSnapshot(handle, roundTrip));
                var restored = MotuNative.LoadMotuSnapshot(roundTrip, out status);
                try
                {
                    Assert.AreEqual(0, status);
                    var copy = CavePreparation.Prepare(restored, 2000, CancellationToken.None);
                    Assert.AreEqual(prepared.caves.Length, copy.caves.Length);
                    for (var i = 0; i < copy.caves.Length; i++)
                    {
                        Assert.AreEqual(prepared.caves[i].id, copy.caves[i].id);
                        for (var j = 0; j < copy.caves[i].chunks.Length; j++)
                        {
                            CollectionAssert.AreEqual(prepared.caves[i].chunks[j].vertices, copy.caves[i].chunks[j].vertices);
                            CollectionAssert.AreEqual(prepared.caves[i].chunks[j].triangles, copy.caves[i].chunks[j].triangles);
                        }
                    }
                }
                finally { if (restored != IntPtr.Zero) MotuNative.ReleaseMotu(restored); }
                for (var lod = 0; lod < 3; lod++)
                {
                    MotuNative.CreateMesh(handle, IntPtr.Zero, lod, 0, out var export);
                    try
                    {
                        Assert.AreNotEqual(IntPtr.Zero, export.handle);
                        var mesh = IslandMeshInterop.CopyTerrainMeshData(export, lod, 2000);
                        foreach (var cave in prepared.caves)
                        {
                            var hasRoof = false;
                            foreach (var vertex in mesh.vertices)
                                hasRoof |= cave.footprint.Contains(new Vector2(vertex.x, vertex.z)) && vertex.y > cave.entrance.y + 12;
                            Assert.IsTrue(hasRoof, $"LOD {lod} lost original terrain above the cave.");
                        }
                    }
                    finally { MotuNative.ReleaseMesh(ref export); }
                }
                var heightMap = TerrainPreparation.PrepareColliderHeightMap(handle, 2000);
                // Exported mesh buffers and Unity objects are independent of the island handle.
                MotuNative.ReleaseMotu(handle); handle = IntPtr.Zero;
                var caves = root.AddComponent<CaveStreamer>();
                caves.InitializeAsync(prepared, material, CancellationToken.None,
                    new UnityFrameBudget(100000)).GetAwaiter().GetResult();
                Physics.SyncTransforms();
                var tiles = new HashSet<Vector2Int>();
                foreach (var cave in prepared.caves)
                {
                    var centre = new Vector2Int(Mathf.FloorToInt((cave.entrance.x+1000)/31.25f), Mathf.FloorToInt((cave.entrance.z+1000)/31.25f));
                    for (var y = centre.y-1; y <= centre.y+1; y++) for (var x = centre.x-1; x <= centre.x+1; x++)
                    {
                        var key = new Vector2Int(x,y);
                        if (x<0 || y<0 || x>=64 || y>=64 || !tiles.Add(key)) continue;
                        var data = new TerrainData { heightmapResolution = 129, size = new Vector3(31.25f,heightMap.verticalSize,31.25f) };
                        terrainData.Add(data);
                        data.SetHeights(0,0,heightMap.CopyTileHeights(key));

                        var tile = new GameObject("Generated heightfield collider");
                        tile.transform.SetParent(root.transform,false);
                        tile.transform.localPosition = new Vector3(x*31.25f-1000,heightMap.verticalOrigin,y*31.25f-1000);
                        tile.AddComponent<TerrainCollider>().terrainData = data;
                    }
                }
                Physics.SyncTransforms();
                for (var i = 0; i < caves.CaveCount; i++)
                {
                    Assert.IsTrue(caves.TryGetEntrance(i, out var entrance, out var inward));
                    Assert.IsTrue(caves.TryFindGround(entrance, .5f, .5f, out var floor));
                    var player = new GameObject("Standing capsule");
                    player.transform.SetParent(root.transform, false);
                    player.transform.position = floor + Vector3.up * .05f;
                    var controller = player.AddComponent<CharacterController>();
                    controller.height = 1.6f; controller.radius = .3f;
                    controller.center = Vector3.up * .8f;
                    controller.stepOffset = .35f; controller.skinWidth = .05f;
                    Physics.SyncTransforms();
                    using var collision = new CaveCollisionScope(controller);
                    for (var step = 0; step < 100; step++)
                    {
                        collision.Update(player.transform.position, true);
                        controller.Move(inward * .04f - Vector3.up * .03f);
                    }
                    Assert.Greater(Vector3.Dot(player.transform.position - entrance, inward), 3.5f, "Generated mouth blocked traversal.");
                    Assert.That(player.transform.position.y, Is.EqualTo(entrance.y).Within(.4f));
                    Assert.IsTrue(caves.TryGetChamber(i, out var chamber));
                    Assert.That(chamber.y, Is.EqualTo(entrance.y).Within(.4f));
                    Object.DestroyImmediate(player);
                }
                caves.Dispose();
            }
            finally
            {
                if (handle != IntPtr.Zero) MotuNative.ReleaseMotu(handle);
                Object.DestroyImmediate(root); Object.DestroyImmediate(material);
                foreach (var data in terrainData) Object.DestroyImmediate(data);
                if (File.Exists(roundTrip)) File.Delete(roundTrip);
            }
        }

        [Test]
        public void CaveWallsUseTheStonePaletteWithRecipesEnabled()
        {
            var root = new GameObject("Stone palette render test");
            var wall = GameObject.CreatePrimitive(PrimitiveType.Quad);
            wall.transform.SetParent(root.transform, false);
            var material = new Material(Resources.Load<Shader>("CaveSurface"));
            var recipes = new Texture2DArray(1, 1, 3, TextureFormat.RGBA32, false);
            var target = new RenderTexture(32, 32, 24);
            var pixels = new Texture2D(32, 32, TextureFormat.RGB24, false);
            var previous = RenderTexture.active;
            var previousFog = RenderSettings.fog;
            try
            {
                for (var layer = 0; layer < 3; layer++) recipes.SetPixels(new[] { Color.blue }, layer);
                recipes.Apply();
                material.SetTexture("_TerrainAlbedoArray", recipes);
                material.SetFloat("_UseTextures", 1);
                material.SetFloat("_CliffNormalStrength", 0);
                material.SetColor("_RockColour", new Color(.8f, .02f, .01f));
                wall.GetComponent<MeshRenderer>().sharedMaterial = material;
                var cameraObject = new GameObject("Stone test camera");
                cameraObject.transform.SetParent(root.transform, false);
                var camera = cameraObject.AddComponent<Camera>();
                camera.transform.position = new Vector3(0, 0, -3);
                camera.orthographic = true;
                camera.orthographicSize = 1;
                camera.targetTexture = target;
                var lightObject = new GameObject("Stone test light");
                lightObject.transform.SetParent(wall.transform, false);
                var light = lightObject.AddComponent<Light>();
                light.type = LightType.Directional;
                light.intensity = 1;
                RenderSettings.fog = false;
                camera.Render();
                RenderTexture.active = target;
                pixels.ReadPixels(new Rect(0, 0, 32, 32), 0, 0);
                pixels.Apply();
                var colour = pixels.GetPixel(16, 16);
                Assert.Greater(colour.r, .1f);
                Assert.Greater(colour.r, colour.b * 4, "The blue recipe replaced the red stone palette on a wall.");
                Assert.IsFalse(UnityEditor.ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderSettings.fog = previousFog;
                RenderTexture.active = previous;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(recipes);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(pixels);
            }
        }

        [Test]
        public void GeneratedCaveExteriorUsesTerrainMaterial()
        {
            var path = Environment.GetEnvironmentVariable("MOTU_CAVE_SNAPSHOT");
            if (string.IsNullOrEmpty(path)) Assert.Ignore("Set MOTU_CAVE_SNAPSHOT to the generated cave fixture.");
            var handle = MotuNative.LoadMotuSnapshot(path, out var status);
            Assert.AreEqual(0, status);
            var root = new GameObject("Cave material validation");
            var material = new Material(Shader.Find("Motu/Terrain Unified"));
            var noise = ProceduralNoiseTextures.CreateCliffNoiseTexture();
            var weatherNoise = ProceduralNoiseTextures.CreateWeatherNoiseTexture();
            var colours = new IslandMaterialColours(new Color(.09f,.055f,.026f),
                new Color(.30f,.32f,.29f), new Color(.62f,.57f,.34f));
            using var arrays = new TerrainMaterialTextureArrays(MaterialPreparation.PrepareMaterialTextures(colours, 128));
            Mesh terrainMesh = null;
            try
            {
                arrays.BindTerrain(material);
                material.SetColor("_RockColor", colours.stone);
                material.SetColor("_GroundDirtColor", colours.dirt);
                material.SetColor("_SandColor", colours.sand);
                material.SetFloat("_SnowLine", 10000);
                material.SetFloat("_WorldNormalWeight", 0);
                material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                material.SetTexture("_CliffNoise3D", noise);
                material.SetTexture("_GrassPatchNoise", weatherNoise);
                var prepared = CavePreparation.Prepare(handle, 2000, CancellationToken.None);
                Assert.Greater(prepared.caves.Length, 0);
                var caves = root.AddComponent<CaveStreamer>();
                caves.InitializeAsync(prepared, material, CancellationToken.None,
                    new UnityFrameBudget(float.MaxValue)).GetAwaiter().GetResult();
                foreach (var renderer in root.GetComponentsInChildren<MeshRenderer>())
                {
                    if (!renderer.enabled) continue; // Entrance lip has collision only here; each terrain LOD renders its own exact join.
                    var caveMaterial = renderer.sharedMaterial;
                    Assert.AreSame(material.GetTexture("_TerrainAlbedoArray"), caveMaterial.GetTexture("_TerrainAlbedoArray"));
                    Assert.AreEqual(material.GetColor("_RockColor"), caveMaterial.GetColor("_RockColour"));
                    Assert.AreSame(noise, caveMaterial.GetTexture("_CliffNoise3D"));
                    Assert.AreEqual(material.GetFloat("_CliffNormalStrength"), caveMaterial.GetFloat("_CliffNormalStrength"));
                }
                foreach (var cave in prepared.caves)
                    foreach (var chunk in cave.chunks)
                        foreach (var vertex in chunk.vertices)
                            Assert.Less(vertex.y, cave.entrance.y + 12, "Cave geometry must not contain a copied terrain roof.");
                var directory = Environment.GetEnvironmentVariable("MOTU_CAVE_GENERATED_SCREENSHOT_DIR");
                if (!string.IsNullOrEmpty(directory))
                {
                    MotuNative.CreateMesh(handle, IntPtr.Zero, 0, 0, out var export);
                    try { terrainMesh = IslandMeshInterop.CopyTerrainMesh(export, 0, 2000); }
                    finally { MotuNative.ReleaseMesh(ref export); }
                    var terrain = new GameObject("Surrounding terrain");
                    terrain.transform.SetParent(root.transform, false);
                    terrain.AddComponent<MeshFilter>().sharedMesh = terrainMesh;
                    terrain.AddComponent<MeshRenderer>().sharedMaterial = material;
                    Capture(root.transform, directory, prepared.caves[0].entrance, prepared.caves[0].inward);
                }
                Assert.IsFalse(UnityEditor.ShaderUtil.ShaderHasError(Resources.Load<Shader>("CaveSurface")));
                caves.Dispose();
            }
            finally
            {
                MotuNative.ReleaseMotu(handle);
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(terrainMesh);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(noise);
                Object.DestroyImmediate(weatherNoise);
            }
        }

        private static IslandPreparedCaves Prepare(Fixture fixture)
        {
            var info = new CaveNative.Info { id = 1,
                minimum = new Vector2(fixture.minimum[0]+1000, fixture.minimum[1]+1000),
                maximum = new Vector2(fixture.maximum[0]+1000, fixture.maximum[1]+1000),
                entrance = new Vector3(fixture.entrance[0]+1000, fixture.entrance[2]+1000, fixture.entrance[1]),
                chamber = new Vector3(fixture.chamber[0]+1000, fixture.chamber[2]+1000, fixture.chamber[1]),
                inward = Vector2.right, chunkCount = (uint)fixture.chunks.Length };
            var chunks = new IslandPreparedMesh[fixture.chunks.Length];
            for (var i = 0; i < chunks.Length; i++)
            {
                chunks[i] = Unpack(fixture.chunks[i]);
            }
            return new IslandPreparedCaves(new[] { new IslandPreparedCave(info, chunks, 2000) }, new CaveNative.Stats { accepted = 1 });
        }
        private static IslandPreparedMesh Unpack(PackedMesh packed)
        {
            var vertices = new Vector3[packed.vertices.Length / 3];
            var normals = new Vector3[vertices.Length];
            var colours = new Color[vertices.Length];
            var caveAttributes = new Vector2[vertices.Length];
            for (var v = 0; v < vertices.Length; v++)
            {
                vertices[v] = new Vector3(packed.vertices[v*3], packed.vertices[v*3+1], packed.vertices[v*3+2]);
                normals[v] = new Vector3(packed.normals[v*3], packed.normals[v*3+1], packed.normals[v*3+2]);
                colours[v] = Color.white;
                var surface = 10 + Mathf.Clamp01(vertices[v].x / 4) * 30;
                var cover = Mathf.Max(0, surface - vertices[v].y);
                var ambient = Mathf.Max(Mathf.Clamp(1-cover/4, .08f, 1), Mathf.Clamp(1-(vertices[v].x+1)/15, .08f, 1));
                caveAttributes[v] = new Vector2(ambient, packed.collisionOnly ? 2 : 0);
            }
            return new IslandPreparedMesh(vertices, normals, packed.triangles, Array.Empty<Vector2>(), colours, Array.Empty<Vector2>(), caveAttributes);
        }

        private static void Capture(Transform root, string directory, Vector3? entrance = null, Vector3? inward = null)
        {
            Directory.CreateDirectory(directory);
            var cameraObject = new GameObject("Cave validation camera");
            cameraObject.transform.SetParent(root, false);
            var camera = cameraObject.AddComponent<Camera>();
            camera.nearClipPlane = .05f;
            camera.farClipPlane = 500;
            camera.clearFlags = CameraClearFlags.SolidColor;
            camera.backgroundColor = new Color(.12f, .18f, .25f);
            var lightObject = new GameObject("Validation sunlight");
            lightObject.transform.SetParent(root, false);
            var light = lightObject.AddComponent<Light>();
            light.type = LightType.Directional;
            light.intensity = 1.5f;
            light.transform.rotation = Quaternion.Euler(45, 70, 0);
            var target = new RenderTexture(1024, 768, 24);
            var image = new Texture2D(1024, 768, TextureFormat.RGB24, false);
            var previous = RenderTexture.active;
            try
            {
                camera.targetTexture = target;
                void Save(string name, Vector3 eye, Vector3 lookAt)
                {
                    camera.transform.position = eye;
                    camera.transform.LookAt(lookAt);
                    camera.Render();
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 1024, 768), 0, 0);
                    image.Apply();
                    File.WriteAllBytes(Path.Combine(directory, name + ".png"), image.EncodeToPNG());
                }
                if (entrance.HasValue && inward.HasValue)
                {
                    var mouth = entrance.Value;
                    var direction = inward.Value;
                    light.transform.rotation = Quaternion.LookRotation(direction - Vector3.up);
                    Save("mouth", mouth - direction * 8 + Vector3.up * 1.6f,
                        mouth + direction * 8 + Vector3.up * 1.6f);
                    Save("mouth-close", mouth - direction * 1.5f + Vector3.up * 1.6f,
                        mouth + direction * 8 + Vector3.up * 2.2f);
                    var right = Vector3.Cross(Vector3.up, direction);
                    Save("mouth-side", mouth - direction * 3 + right * 3 + Vector3.up * 1.6f,
                        mouth + direction * 2 + Vector3.up * 1.5f);
                    var seamEye = mouth - direction * .25f + right * 1.2f + Vector3.up * 2.2f;
                    var seamLook = mouth + direction * 3 + right * 2 + Vector3.up * 3.4f;
                    Save("mouth-seam", seamEye, seamLook);
                    var originalPosition = root.position;
                    var distantOffset = new Vector3(18000, 0, -18000);
                    root.position += distantOffset;
                    foreach (var renderer in root.GetComponentsInChildren<MeshRenderer>())
                        if (renderer.sharedMaterial.HasProperty("_WorldNormalWeight"))
                            renderer.sharedMaterial.SetMatrix("_IslandWorldToLocal", root.worldToLocalMatrix);
                    Save("mouth-seam-distant", seamEye + distantOffset, seamLook + distantOffset);
                    root.position = originalPosition;
                    foreach (var renderer in root.GetComponentsInChildren<MeshRenderer>())
                        if (renderer.sharedMaterial.HasProperty("_WorldNormalWeight"))
                            renderer.sharedMaterial.SetMatrix("_IslandWorldToLocal", root.worldToLocalMatrix);
                    Save("roof", mouth - direction * 15 + Vector3.up * 24,
                        mouth + direction * 10 + Vector3.up * 10);
                    return;
                }
                Save("entrance", new Vector3(-16, 14, -12), new Vector3(2, 13, 0));
                Save("roof", new Vector3(20, 55, -22), new Vector3(10, 32, 0));
                light.type = LightType.Point;
                light.range = 35;
                light.intensity = 3;
                light.transform.position = new Vector3(8, 12, 0);
                Save("interior", new Vector3(5, 11.6f, 0), new Vector3(22, 11.6f, 0));
            }
            finally
            {
                RenderTexture.active = previous;
                camera.targetTexture = null;
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(image);
            }
        }
    }
}
