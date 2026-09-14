using System;
using System.Runtime.InteropServices;
using Motu.Interop;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class FallenLogTests
    {
        [Test]
        public void LogBoxesAndEndGrainUvsFollowTheirLod0WoodTile()
        {
            var parent = new GameObject("Fallen log streaming validation");
            var material = new Material(Shader.Find("Motu/Tree Wood"));
            var streamer = new ForestTileStreamer();
            try
            {
                var wood = new IslandPreparedMesh(
                    new[] { Vector3.zero, Vector3.right, Vector3.forward },
                    new[] { Vector3.up, Vector3.up, Vector3.up }, new[] { 0, 1, 2 },
                    new[] { Vector2.right, Vector2.right, Vector2.right },
                    new[] { new Color(.5f, .5f, .02f, 0), new Color(.5f, .5f, .02f, 0), new Color(.5f, .5f, .02f, 0) },
                    new[] { Vector2.zero, Vector2.right, Vector2.up });
                var lod0 = new IslandPreparedMesh[ForestTileStreamer.Lod1TileCount]; lod0[0] = wood;
                var logs = new IslandPreparedTreeCollider[ForestTileStreamer.Lod1TileCount][];
                var bottom = new Vector3(2, 3, 4); var top = new Vector3(6, 4, 7);
                logs[0] = new[] { new IslandPreparedTreeCollider(bottom, top, .3f) };
                var prepared = new IslandPreparedForestData(
                    new IslandPreparedMesh[ForestTileStreamer.Lod2TileCount],
                    new IslandPreparedMesh[ForestTileStreamer.Lod2TileCount],
                    new IslandPreparedMesh[ForestTileStreamer.Lod1TileCount],
                    new IslandPreparedMesh[ForestTileStreamer.Lod1TileCount],
                    new IslandPreparedMesh[ForestTileStreamer.Lod1TileCount], lod0,
                    new IslandPreparedTreeCollider[ForestTileStreamer.Lod1TileCount][], logs);
                streamer.Initialize(parent.transform, material, material, material, material, material, prepared, true);
                Assert.That(streamer.Root.GetComponentsInChildren<BoxCollider>(true), Is.Empty);
                streamer.UpdateLod1Neighborhood(Vector2Int.zero);
                streamer.UpdateLod0Neighborhood(Vector2Int.zero);
                var boxes = streamer.Root.GetComponentsInChildren<BoxCollider>();
                Assert.That(boxes.Length, Is.EqualTo(1));
                var box = boxes[0];
                Assert.That(box.transform.localPosition, Is.EqualTo((bottom + top) * .5f));
                Assert.That(box.size, Is.EqualTo(new Vector3(.6f, (top - bottom).magnitude, .6f)));
                Assert.That(Vector3.Dot(box.transform.up, (top - bottom).normalized), Is.GreaterThan(.9999f));
                Assert.That(streamer.Root.GetComponentsInChildren<CapsuleCollider>(), Is.Empty);
                var renderers = streamer.Root.GetComponentsInChildren<MeshRenderer>();
                Assert.That(renderers.Length, Is.EqualTo(1));
                Assert.That(renderers[0].sharedMaterials.Length, Is.EqualTo(1));
                var mesh = renderers[0].GetComponent<MeshFilter>().sharedMesh;
                Assert.That(mesh.uv2, Is.EqualTo(wood.environment));
                Assert.That(mesh.colors, Is.EqualTo(wood.material));
                streamer.SetVisible(false);
                Assert.That(box.gameObject.activeInHierarchy, Is.False);
                streamer.SetVisible(true);
                Assert.That(box.gameObject.activeInHierarchy, Is.True);
                streamer.UpdateLod0Neighborhood(new Vector2Int(ForestTileStreamer.Lod1Resolution - 1,
                    ForestTileStreamer.Lod1Resolution - 1));
                Assert.That(streamer.Root.GetComponentsInChildren<BoxCollider>(true), Is.Empty);
            }
            finally { streamer.Dispose(); Object.DestroyImmediate(material); Object.DestroyImmediate(parent); }
        }

        [Test]
        public void StandingTreeAndCanopyStreamingRetainsItsExistingBehaviour()
        {
            var material = new Material(Shader.Find("Motu/Tree Foliage"));
            var lod0Material = new Material(material);
            try { ForestRenderingValidation.ValidateLowPolyCanopyShadowProxy(material, lod0Material); }
            finally { Object.DestroyImmediate(material); Object.DestroyImmediate(lod0Material); }
        }

        [Test]
        public void LogsStayStationaryWhileLivingTreesBendInStrongWind()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Fallen Log Wind Probe"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat);
            var output = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            try
            {
                material.SetFloat("_WorldSize", 2000);
                material.SetVector("_MotuTreeWindHeights", new Vector4(0, 10, 0, 0));
                material.SetVector("_MotuWeatherWind", new Vector4(1, 0, 30, 3));
                material.SetVector("_MotuWindMaterial", new Vector4(5, 100, .2f, 0));
                material.SetTexture("_MotuWindNoise", Texture2D.whiteTexture);
                foreach (var tag in new[] { .5f, .25f, 0f })
                {
                    material.SetVector("_ProbeTreeData", new Vector4(.5f, .5f, .02f, tag));
                    Graphics.Blit(Texture2D.whiteTexture, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 1, 1), 0, 0); output.Apply();
                    var value = output.GetPixel(0, 0);
                    Assert.That(value.a, Is.EqualTo(1).Within(.0001f), "All forest surfaces retain their owning root.");
                    if (tag == .5f) Assert.That(value.r, Is.GreaterThan(1));
                    else Assert.That(new Vector3(value.r, value.g, value.b).magnitude, Is.LessThan(.0001f));
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Object.DestroyImmediate(material); Object.DestroyImmediate(target); Object.DestroyImmediate(output);
            }
        }

        [Test]
        public void LogExportEntryPointAndManagedLayoutMatchNativePlugin()
        {
            Assert.That(Marshal.SizeOf<MotuNative.ForestOptions>(), Is.EqualTo(32));
            Assert.That(Marshal.SizeOf<MotuNative.ForestTrunkColliderExport>(), Is.EqualTo(36));
            MotuNative.CreateForestLogColliders(IntPtr.Zero, out var export);
            Assert.That(export.handle, Is.EqualTo(IntPtr.Zero));
            Assert.That(export.length, Is.Zero);
            MotuNative.ReleaseForestTrunkColliders(ref export);
        }

        [TestCase(false)]
        [TestCase(true)]
        public void ExposedEndsUseEndGrainAndIgnoreBarkMaps(bool noParallax)
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var material = new Material(Shader.Find("Motu/Tree Wood"));
            var mesh = new Mesh();
            var host = new GameObject("End grain render fixture", typeof(MeshFilter), typeof(MeshRenderer));
            var cameraHost = new GameObject("End grain validation camera", typeof(Camera));
            var camera = cameraHost.GetComponent<Camera>();
            var target = new RenderTexture(128, 128, 24, RenderTextureFormat.ARGB32);
            var output = new Texture2D(128, 128, TextureFormat.RGB24, false, true);
            var oldFog = RenderSettings.fog;
            var oldMatrix = Shader.GetGlobalMatrix("_IslandWorldToLocal");
            var previousTarget = RenderTexture.active;
            try
            {
                RenderSettings.fog = false;
                Shader.SetGlobalMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                mesh.vertices = new[] { new Vector3(-1, -1, 0), new Vector3(1, -1, 0), new Vector3(1, 1, 0), new Vector3(-1, 1, 0) };
                mesh.normals = new[] { Vector3.back, Vector3.back, Vector3.back, Vector3.back };
                mesh.triangles = new[] { 0, 2, 1, 0, 3, 2 };
                mesh.uv = new[] { new Vector2(1, .5f), new Vector2(1, .5f), new Vector2(1, .5f), new Vector2(1, .5f) };
                mesh.uv2 = new[] { Vector2.zero, Vector2.right, Vector2.one, Vector2.up };
                void Tag(float alpha) => mesh.colors = new[] { new Color(.5f, .5f, 0, alpha), new Color(.5f, .5f, 0, alpha), new Color(.5f, .5f, 0, alpha), new Color(.5f, .5f, 0, alpha) };
                Tag(0);
                host.GetComponent<MeshFilter>().sharedMesh = mesh;
                host.GetComponent<MeshRenderer>().sharedMaterial = material;
                camera.enabled = false; camera.orthographic = true; camera.orthographicSize = 1.05f;
                camera.transform.position = Vector3.back * 3; camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.magenta; camera.targetTexture = target;
                if (noParallax) material.EnableKeyword("MOTU_TREE_BARK_NO_PARALLAX");
                material.SetFloat("_BarkAmbientFloor", 1);
                material.SetColor("_EndGrainColor", Color.green);
                material.SetTexture("_EndGrainMap", Texture2D.whiteTexture);
                Color Read()
                {
                    camera.Render(); RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 128, 128), 0, 0); output.Apply();
                    return output.GetPixel(64, 64);
                }
                var cap = Read();
                Assert.That(cap.g, Is.GreaterThan(.5f));
                Assert.That(cap.r, Is.LessThan(.1f));
                material.SetTexture("_BarkAlbedoMap", Texture2D.blackTexture);
                Assert.That(Read().g, Is.EqualTo(cap.g).Within(.02f), "Bark must not affect exposed ends.");
                Tag(.25f);
                Assert.That(Read().g, Is.LessThan(cap.g * .2f), "Side faces must still sample bark.");
                Tag(0);
                var grain = Resources.Load<Texture2D>("Motu/TreeEndGrain");
                Assert.IsNotNull(grain); Assert.That(grain.mipmapCount, Is.GreaterThan(1));
                material.SetTexture("_EndGrainMap", grain);
                material.SetColor("_EndGrainColor", new Color(.62f, .44f, .25f));
                Read();
                System.IO.File.WriteAllBytes(System.IO.Path.Combine(Application.temporaryCachePath, "motu-end-grain-preview.png"), output.EncodeToPNG());
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = previousTarget; RenderSettings.fog = oldFog;
                Shader.SetGlobalMatrix("_IslandWorldToLocal", oldMatrix);
                Object.DestroyImmediate(host); Object.DestroyImmediate(cameraHost); Object.DestroyImmediate(mesh);
                Object.DestroyImmediate(material); Object.DestroyImmediate(target); Object.DestroyImmediate(output);
            }
        }
    }
}
