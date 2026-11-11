using System;
using System.IO;
using Motu.Rendering;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class OceanHullNormalTests
    {
        sealed class Fixture : IDisposable
        {
            readonly EnvironmentHostState baseline = new EnvironmentHostState();
            readonly GameObject seaRoot = new GameObject("Hull normal sea");
            readonly GameObject hull = new GameObject("Convex exclusion normal fixture");
            readonly OceanHullVolume volume = ScriptableObject.CreateInstance<OceanHullVolume>();
            readonly Mesh boundary = new Mesh();
            readonly Material probe;
            internal readonly OceanSurfaceController Sea;

            internal Fixture(float yaw)
            {
                Sea = seaRoot.AddComponent<OceanSurfaceController>();
                Sea.Install(new Material(Shader.Find("Motu/Sea Water")), 128, false);
                var weather = Sea.Weather;
                weather.Enabled = true;
                weather.Wave0 = new Motu.Settings.OceanWaveComponent(Vector2.right, 120, 6, 0);
                weather.Wave0.Choppiness = 0;
                weather.Wave1.AmplitudeMetres = weather.Wave2.AmplitudeMetres = weather.Wave3.AmplitudeMetres = 0;
                weather.DomainWarpMetres = weather.AmplitudeVariation = 0; weather.OnshoreWaveEnabled = false;
                Sea.ApplyWaveWeather(weather);
                Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                Shader.SetGlobalFloat("_MotuShipWaveEnabled", 0);
                hull.transform.SetPositionAndRotation(Vector3.right * 30, Quaternion.Euler(0, yaw, 0));
                boundary.vertices = new[] { new Vector3(-4,-5,-10),new Vector3(4,-5,-10),new Vector3(-4,5,-10),new Vector3(4,5,-10),
                    new Vector3(-4,-5,10),new Vector3(4,-5,10),new Vector3(-4,5,10),new Vector3(4,5,10) };
                boundary.RecalculateBounds();
                volume.SetGeometry(new[] { new Vector4(1,0,0,-4),new Vector4(-1,0,0,-4),
                    new Vector4(0,1,0,-5),new Vector4(0,-1,0,-5),new Vector4(0,0,1,-10),new Vector4(0,0,-1,-10) }, boundary);
                hull.AddComponent<OceanDeckWaveClamp>().ConfigureConvexHull(volume);
                probe = new Material(Shader.Find("Hidden/Motu/Tests/Hull Surface Normal"));
                Assert.That(ShaderUtil.ShaderHasError(probe.shader), Is.False);
                probe.CopyPropertiesFromMaterial(Sea.SurfaceMaterial);
                probe.SetVector("_OceanGeometryLod", new Vector4(100, .5f, 8, 8));
            }

            internal Color[] Read(Vector3[] local, bool geometry = false)
            {
                probe.CopyPropertiesFromMaterial(Sea.SurfaceMaterial);
                probe.SetVector("_OceanGeometryLod", new Vector4(100, .5f, 8, 8));
                var input = new Texture2D(local.Length, 1, TextureFormat.RGBAFloat, false, true) { filterMode = FilterMode.Point };
                var target = new RenderTexture(local.Length, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
                var readback = new Texture2D(local.Length, 1, TextureFormat.RGBAFloat, false, true);
                var previous = RenderTexture.active;
                try
                {
                    var points = new Color[local.Length];
                    for (var i = 0; i < points.Length; ++i)
                    {
                        var point = hull.transform.TransformPoint(local[i]);
                        points[i] = new Color(point.x, point.y, point.z, 1);
                    }
                    input.SetPixels(points); input.Apply();
                    probe.SetTexture("_ProbePositions", input);
                    probe.SetFloat("_ProbeOutput", geometry ? 1 : 0);
                    Graphics.Blit(Texture2D.blackTexture, target, probe, 0); RenderTexture.active = target;
                    readback.ReadPixels(new Rect(0,0,local.Length,1),0,0); readback.Apply();
                    return readback.GetPixels();
                }
                finally { RenderTexture.active = previous; Object.DestroyImmediate(input); Object.DestroyImmediate(target); Object.DestroyImmediate(readback); }
            }

            internal void MoveToTrough()
            {
                hull.transform.position = Vector3.right * 90;
                OceanDeckWaveClamp.BindGlobals();
            }

            internal void Capture()
            {
                var mesh = new Mesh();
                var target = new RenderTexture(640, 512, 24, RenderTextureFormat.ARGB32);
                var image = new Texture2D(640, 512, TextureFormat.RGB24, false);
                var previous = RenderTexture.active;
                using var cmd = new CommandBuffer();
                try
                {
                    const int width = 33, depth = 65;
                    var points = new Vector3[width * depth]; var triangles = new int[(width - 1) * (depth - 1) * 6];
                    for (var z = 0; z < depth; ++z)
                    for (var x = 0; x < width; ++x) points[z * width + x] = new Vector3(22 + x * .5f, 0, -16 + z * .5f);
                    var index = 0;
                    for (var z = 0; z < depth - 1; ++z)
                    for (var x = 0; x < width - 1; ++x)
                    {
                        var a = z * width + x; triangles[index++] = a; triangles[index++] = a + width; triangles[index++] = a + 1;
                        triangles[index++] = a + 1; triangles[index++] = a + width; triangles[index++] = a + width + 1;
                    }
                    mesh.vertices = points; mesh.triangles = triangles;
                    var eye = new Vector3(48, 21, -25);
                    var camera = Matrix4x4.TRS(eye, Quaternion.LookRotation(new Vector3(30,0,0) - eye), Vector3.one);
                    var view = Matrix4x4.Scale(new Vector3(1,1,-1)) * camera.inverse;
                    probe.SetMatrix("_ProbeViewProjection", GL.GetGPUProjectionMatrix(Matrix4x4.Perspective(50,640f/512,.1f,200),true) * view);
                    probe.SetVector("_ProbeEye", eye);
                    target.Create();
                    cmd.SetRenderTarget(target); cmd.ClearRenderTarget(true,true,new Color(.025f,.035f,.045f));
                    cmd.DrawMesh(mesh, Matrix4x4.identity, probe, 0, 1); Graphics.ExecuteCommandBuffer(cmd);
                    RenderTexture.active = target; image.ReadPixels(new Rect(0,0,640,512),0,0); image.Apply();
                    var label = Environment.GetEnvironmentVariable("MOTU_HULL_NORMAL_CAPTURE") ?? "current";
                    File.WriteAllBytes("/tmp/ocean-hull-normal-smoothing-20261110/" + label + ".png", image.EncodeToPNG());
                }
                finally { RenderTexture.active = previous; Object.DestroyImmediate(mesh); Object.DestroyImmediate(target); Object.DestroyImmediate(image); }
            }

            public void Dispose()
            {
                Object.DestroyImmediate(hull); Object.DestroyImmediate(seaRoot); Object.DestroyImmediate(volume);
                Object.DestroyImmediate(boundary); Object.DestroyImmediate(probe); baseline.Restore();
            }
        }

        [TestCase(0)]
        [TestCase(37)]
        [TestCase(90)]
        public void UnclampedSideAndDiagonalNeighboursReceiveSmoothHullLighting(float yaw)
        {
            using var fixture = new Fixture(yaw);
            var points = new[] { new Vector3(4.6f,0,0),new Vector3(4.45f,0,10.45f),new Vector3(8,0,14) };
            var surface = fixture.Read(points, true); var normals = fixture.Read(points);
            for (var i = 0; i < 2; ++i)
            {
                Assert.That(surface[i].a, Is.Zero.Within(.001f), "Neighbouring vertices remain outside the exclusion and are never pulled down.");
                Assert.That(normals[i].a, Is.GreaterThan(.001f), "Shading must borrow the bend from nearby lowered vertices, including diagonal neighbours.");
                Assert.That(new Vector3(normals[i].r,normals[i].g,normals[i].b).magnitude, Is.EqualTo(1).Within(.001f));
            }
            Assert.That(normals[2].a, Is.Zero, "Ordinary waves outside the local smoothing band keep their analytic normals.");
            Assert.That(ShaderUtil.ShaderHasError(Shader.Find("Motu/Sea Water")), Is.False);
        }

        [Test]
        public void NormalSmoothingPreservesHullExclusionGeometryAndLeavesTroughsAlone()
        {
            using var fixture = new Fixture(0);
            fixture.Capture();
            var points = new[] { Vector3.zero,new Vector3(4.6f,0,0) };
            var geometry = fixture.Read(points, true);
            Assert.That(geometry[0].r, Is.EqualTo(30).Within(.001f), "The geometry probe returns unclipped world-space data.");
            Assert.That(geometry[0].g, Is.EqualTo(-4.97f).Within(.003f), "The crest is still projected to the unchanged hull underside.");
            Assert.That(geometry[0].a, Is.GreaterThan(10));
            Assert.That(geometry[1].g, Is.GreaterThan(5), "Outside vertices keep their original crest height.");
            fixture.MoveToTrough();
            var trough = fixture.Read(points, true); var troughNormals = fixture.Read(points);
            for (var i = 0; i < trough.Length; ++i)
            {
                Assert.That(trough[i].g, Is.LessThan(-5), "Troughs still pass below the unchanged exclusion.");
                Assert.That(trough[i].a, Is.Zero); Assert.That(troughNormals[i].a, Is.Zero);
            }
            var weather = fixture.Sea.Weather; weather.Wave0.AmplitudeMetres = 0;
            fixture.Sea.ApplyWaveWeather(weather);
            var flat = fixture.Read(points); var unchanged = fixture.Read(points,true);
            for (var i = 0; i < flat.Length; ++i)
            {
                Assert.That(flat[i].a, Is.Zero); Assert.That(flat[i].g, Is.EqualTo(1).Within(.001f));
                Assert.That(unchanged[i].a, Is.Zero); Assert.That(unchanged[i].g, Is.Zero.Within(.001f));
            }
        }
    }
}
