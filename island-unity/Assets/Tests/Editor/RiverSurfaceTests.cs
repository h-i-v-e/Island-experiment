using System;
using System.IO;
using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class RiverSurfaceTests
    {
        private static Color[] Read(Material material)
        {
            const int size = 64;
            var target = new RenderTexture(size, size, 0, RenderTextureFormat.ARGBFloat);
            var image = new Texture2D(size, size, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                Graphics.Blit(Texture2D.blackTexture, target, material);
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, size, size), 0, 0);
                image.Apply();
                return image.GetPixels();
            }
            finally { RenderTexture.active = previous; Object.DestroyImmediate(target); Object.DestroyImmediate(image); }
        }

        private static float Difference(Color[] a, Color[] b)
        {
            var result = 0f;
            for (var i = 0; i < a.Length; i++) result += Mathf.Abs(a[i].r-b[i].r) + Mathf.Abs(a[i].g-b[i].g) + Mathf.Abs(a[i].b-b[i].b);
            return result / a.Length;
        }

        [Test]
        public void RipplesTravelDownstreamAndFollowRotatedAndVerticalSurfaces()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/River Surface"));
            var noise = ProceduralNoiseTextures.CreateRiverNoiseTexture();
            try
            {
                Assert.That(noise.mipmapCount, Is.GreaterThan(1));
                material.SetTexture("_NoiseTex", noise);
                material.SetFloat("_CoarseNoiseWorldSize", 1.2f);
                material.SetFloat("_FineNoiseWorldSize", .65f);
                material.SetFloat("_CoarseFlowSpeed", 1);
                material.SetFloat("_FineFlowSpeed", 1);
                material.SetFloat("_RippleStrength", .1f);
                material.SetFloat("_SurfaceRoughness", .18f);
                material.SetFloat("_RapidRoughness", .18f);
                material.SetFloat("_ProbeSpan", 4);
                material.SetMatrix("_ProbeRotation", Matrix4x4.identity);
                var initial = Read(material);
                material.SetFloat("_ProbeTime", 1);
                var later = Read(material);
                Assert.That(Difference(initial, later), Is.GreaterThan(.005));
                material.SetVector("_ProbeOffset", new Vector4(0, 1, 0, 0));
                Assert.That(Difference(initial, Read(material)), Is.LessThan(.0001), "Patterns must move in increasing downstream distance.");
                material.SetFloat("_ProbeTime", 0);
                material.SetVector("_ProbeOffset", Vector4.zero);
                foreach (var rotation in new[] { Quaternion.Euler(0, 73, 0), Quaternion.Euler(90, 0, 0) })
                {
                    material.SetMatrix("_ProbeRotation", Matrix4x4.Rotate(rotation));
                    var turned = Read(material);
                    for (var i = 0; i < turned.Length; i++)
                    {
                        var normal = new Vector3(turned[i].r, turned[i].g, turned[i].b);
                        var expected = rotation * new Vector3(initial[i].r, initial[i].g, initial[i].b);
                        Assert.That(Vector3.Distance(normal, expected), Is.LessThan(.001));
                        Assert.That(normal.magnitude, Is.EqualTo(1).Within(.001));
                    }
                }
                material.SetMatrix("_ProbeRotation", Matrix4x4.identity);
                material.SetFloat("_ProbeSpan", 512);
                var distant = Read(material);
                for (var i = 0; i < distant.Length; i++) Assert.That(distant[i].g, Is.EqualTo(1).Within(.001));
                material.SetFloat("_ProbeSpan", 4);
                material.SetFloat("_ProbeMode", 1);
                material.SetFloat("_FoamWorldSize", .7f);
                material.SetFloat("_FoamStretch", 3);
                material.SetFloat("_WaterfallFlowSpeed", 9);
                var calmFoam = Read(material);
                material.SetFloat("_ProbeRapids", 1);
                var rapidFoam = Read(material);
                float calm = 0, rapid = 0;
                for (var i = 0; i < calmFoam.Length; i++) { calm += calmFoam[i].r; rapid += rapidFoam[i].r; }
                Assert.That(rapid, Is.GreaterThan(calm * 20));
                material.SetFloat("_ProbeTime", 1);
                Assert.That(Difference(rapidFoam, Read(material)), Is.GreaterThan(.005));
                material.SetVector("_ProbeOffset", new Vector4(0, 1, 0, 0));
                material.SetFloat("_ProbeRapids", 0);
                Assert.That(Difference(calmFoam, Read(material)), Is.LessThan(.0001), "Calm foam follows the channel flow speed.");
                material.SetFloat("_ProbeMode", 2);
                material.SetFloat("_ProbeTime", 0);
                material.SetVector("_ProbeOffset", Vector4.zero);
                var falling = Read(material);
                material.SetFloat("_ProbeTime", .1f);
                Assert.That(Difference(falling, Read(material)), Is.GreaterThan(.05), "The fine waterfall pattern must change rapidly.");
                material.SetVector("_ProbeOffset", new Vector4(0, .9f, 0, 0));
                var fallen = Read(material);
                for (var i = 0; i < fallen.Length; i++)
                    Assert.That(fallen[i].g, Is.EqualTo(falling[i].g).Within(.001), "Fine waterfall noise travels 0.9 m downstream in 0.1 s.");
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally { Object.DestroyImmediate(material); Object.DestroyImmediate(noise); }
        }

        private static float RiverHeight(float z) => 1 + 5 * (1 - Mathf.SmoothStep(0, 1, Mathf.InverseLerp(-3, 5, z)));

        private static Mesh ChannelMesh(bool bed)
        {
            const int rows = 96, columns = 24;
            var positions = new Vector3[(rows+1)*(columns+1)];
            var uv = new Vector2[positions.Length];
            var triangles = new int[rows*columns*6];
            var index = 0;
            for (var row = 0; row <= rows; row++)
            {
                var z = Mathf.Lerp(-24, 24, row / (float)rows);
                var centre = Mathf.Sin(z * .075f) * 3;
                for (var col = 0; col <= columns; col++)
                {
                    var x = Mathf.Lerp(bed ? -11 : -4.5f, bed ? 11 : 4.5f, col / (float)columns);
                    var height = RiverHeight(z);
                    if (bed) height += -.7f + 1.8f * Mathf.SmoothStep(0, 1, Mathf.InverseLerp(3.2f, 6.5f, Mathf.Abs(x)));
                    var vertex = row*(columns+1)+col;
                    positions[vertex] = new Vector3(x+centre, height, z);
                    uv[vertex] = bed ? new Vector2(x, z) * .18f : new Vector2(4.5f-Mathf.Abs(x), z);
                    if (row == rows || col == columns) continue;
                    foreach (var item in new[] { vertex, vertex+columns+1, vertex+1, vertex+1, vertex+columns+1, vertex+columns+2 })
                        triangles[index++] = item;
                }
            }
            var mesh = new Mesh { vertices = positions, uv = uv, triangles = triangles };
            mesh.RecalculateNormals(); mesh.RecalculateBounds();
            return mesh;
        }

        [Test]
        public void RenderBendRapidsBanksAndForegroundRocks()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var river = new GameObject("River rendering fixture");
            var bed = new GameObject("Channel and banks");
            var cameraObject = new GameObject("River camera");
            var sunObject = new GameObject("River sun");
            var rock = GameObject.CreatePrimitive(PrimitiveType.Sphere);
            var waterMesh = ChannelMesh(false);
            var bedMesh = ChannelMesh(true);
            var material = new Material(Shader.Find("Motu/River Water"));
            var coastMaterial = new Material(Shader.Find("Motu/Coastal Water Overlay"));
            var bedMaterial = new Material(Shader.Find("Standard"));
            var rockMaterial = new Material(Shader.Find("Standard"));
            var noise = ProceduralNoiseTextures.CreateRiverNoiseTexture();
            var target = new RenderTexture(768, 512, 24, RenderTextureFormat.ARGBFloat);
            var image = new Texture2D(768, 512, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            var oldCloud = Shader.GetGlobalFloat("_MotuCloudEnabled");
            var oldReflection = Shader.GetGlobalFloat("_PlanarReflectionAvailable");
            var oldFog = RenderSettings.fog;
            var oldAmbient = RenderSettings.ambientLight;
            var oldAmbientMode = RenderSettings.ambientMode;
            var oldSun = RenderSettings.sun;
            try
            {
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_PlanarReflectionAvailable", 0);
                RenderSettings.fog = false;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = new Color(.2f, .23f, .26f);
                var sun = sunObject.AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.transform.rotation = Quaternion.LookRotation(new Vector3(.3f, -.7f, -.4f));
                sun.intensity = 1.2f;
                RenderSettings.sun = sun;
                material.SetTexture("_NoiseTex", noise);
                material.SetFloat("_WorldSize", 1);
                material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                material.SetColor("_Color", new Color(.06f, .25f, .17f));
                bedMaterial.color = new Color(.32f, .25f, .14f);
                bedMaterial.SetFloat("_Glossiness", .05f);
                rockMaterial.color = new Color(.25f, .27f, .28f);
                rockMaterial.SetFloat("_Glossiness", .1f);
                river.AddComponent<MeshFilter>().sharedMesh = waterMesh;
                river.AddComponent<MeshRenderer>().sharedMaterial = material;
                bed.AddComponent<MeshFilter>().sharedMesh = bedMesh;
                bed.AddComponent<MeshRenderer>().sharedMaterial = bedMaterial;
                rock.GetComponent<Renderer>().sharedMaterial = rockMaterial;
                rock.transform.position = new Vector3(Mathf.Sin(9 * .075f) * 3 + .7f, RiverHeight(9), 9);
                rock.transform.localScale = new Vector3(2.5f, 2, 3);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.transform.position = new Vector3(13, 23, -16);
                camera.transform.LookAt(new Vector3(0, 2, 1));
                camera.depthTextureMode = DepthTextureMode.Depth;
                camera.backgroundColor = new Color(.5f, .65f, .75f);
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.targetTexture = target;
                Color[] Capture(string filename)
                {
                    camera.Render(); RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 768, 512), 0, 0); image.Apply();
                    var pixels = image.GetPixels();
                    foreach (var pixel in pixels) Assert.IsTrue(float.IsFinite(pixel.r) && float.IsFinite(pixel.g) && float.IsFinite(pixel.b));
                    var directory = Environment.GetEnvironmentVariable("MOTU_RIVER_IMAGES");
                    if (!string.IsNullOrEmpty(directory))
                    {
                        Directory.CreateDirectory(directory);
                        var png = new Texture2D(768, 512, TextureFormat.RGB24, false);
                        if (QualitySettings.activeColorSpace == ColorSpace.Linear)
                            for (var i = 0; i < pixels.Length; i++) pixels[i] = pixels[i].gamma;
                        png.SetPixels(pixels); png.Apply();
                        File.WriteAllBytes(Path.Combine(directory, filename), png.EncodeToPNG());
                        Object.DestroyImmediate(png);
                    }
                    return image.GetPixels();
                }
                var flowing = Capture("river-bend-and-rapids.png");
                material.SetFloat("_WhitewaterStrength", 0);
                var noFoam = Capture("river-without-foam.png");
                Assert.That(Difference(flowing, noFoam), Is.GreaterThan(.0001));
                material.SetFloat("_RippleStrength", 0);
                Assert.That(Difference(noFoam, Capture("river-without-detail.png")), Is.GreaterThan(.0001));
                coastMaterial.SetTexture("_SeaMask", Texture2D.whiteTexture);
                coastMaterial.SetFloat("_WorldSize", 64);
                coastMaterial.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                river.GetComponent<MeshRenderer>().sharedMaterial = coastMaterial;
                Capture("coastal-tint-only.png");
                Assert.IsFalse(ShaderUtil.ShaderHasError(coastMaterial.shader));
            }
            finally
            {
                RenderTexture.active = previous;
                Shader.SetGlobalFloat("_MotuCloudEnabled", oldCloud);
                Shader.SetGlobalFloat("_PlanarReflectionAvailable", oldReflection);
                RenderSettings.fog = oldFog;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.ambientMode = oldAmbientMode;
                RenderSettings.sun = oldSun;
                foreach (var item in new Object[] { river, bed, cameraObject, sunObject, rock, waterMesh, bedMesh, material, coastMaterial, bedMaterial, rockMaterial, noise, target, image }) Object.DestroyImmediate(item);
            }
        }

        [Test]
        public void RiverCompositionPreservesShallowsAndAbsorbsWithDepth()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var river = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var bed = GameObject.CreatePrimitive(PrimitiveType.Plane);
            var cameraObject = new GameObject("River optics camera");
            var material = new Material(Shader.Find("Motu/River Water"));
            var bedMaterial = new Material(Shader.Find("Standard"));
            var target = new RenderTexture(128, 128, 24, RenderTextureFormat.ARGBFloat);
            var image = new Texture2D(128, 128, TextureFormat.RGBAFloat, false, true);
            var noise = ProceduralNoiseTextures.CreateRiverNoiseTexture();
            var checker = new Texture2D(32, 32, TextureFormat.RGB24, false, true) { filterMode = FilterMode.Bilinear };
            for (var y = 0; y < 32; y++) for (var x = 0; x < 32; x++)
                checker.SetPixel(x, y, ((x / 2 + y / 2) & 1) == 0 ? Color.white : Color.black);
            checker.Apply();
            var previous = RenderTexture.active;
            var oldFog = RenderSettings.fog;
            try
            {
                RenderSettings.fog = false;
                river.GetComponent<Renderer>().sharedMaterial = material;
                bed.GetComponent<Renderer>().sharedMaterial = bedMaterial;
                bedMaterial.color = Color.black;
                bedMaterial.EnableKeyword("_EMISSION");
                bedMaterial.SetColor("_EmissionColor", Color.white);
                bedMaterial.SetFloat("_Glossiness", 0);
                material.SetColor("_Color", Color.black);
                material.SetColor("_SeaColor", Color.black);
                material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                material.SetFloat("_RippleStrength", 0);
                material.SetFloat("_WorldSize", 10);
                material.SetTexture("_NoiseTex", noise);
                material.SetFloat("_ReflectionStrength", 0);
                material.SetFloat("_SunGlintStrength", 0);
                material.SetFloat("_WhitewaterStrength", 0);
                material.SetFloat("_RefractionStrength", 0);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.transform.position = Vector3.up * 8;
                camera.transform.rotation = Quaternion.LookRotation(Vector3.down, Vector3.forward);
                camera.depthTextureMode = DepthTextureMode.Depth;
                camera.targetTexture = target;
                Color Capture(float depth)
                {
                    bed.transform.position = Vector3.down * depth;
                    camera.Render();
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 128, 128), 0, 0);
                    image.Apply();
                    return image.GetPixel(64, 64);
                }
                var shallow = Capture(.2f);
                var deep = Capture(2);
                Assert.That(shallow.r, Is.GreaterThan(deep.r));
                Assert.That(shallow.g, Is.GreaterThan(deep.g));
                Assert.That(deep.g, Is.GreaterThan(deep.b));
                Assert.That(deep.b, Is.GreaterThan(deep.r));
                Assert.That(deep.r / shallow.r, Is.EqualTo(Mathf.Exp(-.32f * 1.8f)).Within(.02));
                material.SetFloat("_AbsorptionStrength", 0);
                var clearWater = Capture(2);
                river.SetActive(false);
                var unobstructedBed = Capture(2);
                Assert.That(clearWater.r, Is.EqualTo(unobstructedBed.r).Within(.002));
                void SaveBed(string name)
                {
                    var directory = Environment.GetEnvironmentVariable("MOTU_RIVER_IMAGES");
                    if (string.IsNullOrEmpty(directory)) return;
                    Directory.CreateDirectory(directory);
                    var png = new Texture2D(128, 128, TextureFormat.RGB24, false);
                    var pixels = image.GetPixels();
                    if (QualitySettings.activeColorSpace == ColorSpace.Linear)
                        for (var i = 0; i < pixels.Length; i++) pixels[i] = pixels[i].gamma;
                    png.SetPixels(pixels); png.Apply();
                    File.WriteAllBytes(Path.Combine(directory, name), png.EncodeToPNG());
                    Object.DestroyImmediate(png);
                }
                river.SetActive(true);
                bedMaterial.SetTexture("_EmissionMap", checker);
                Capture(2);
                var undistorted = image.GetPixels();
                SaveBed("bed-without-distortion.png");
                material.SetFloat("_RefractionStrength", .012f);
                Capture(2);
                var distorted = image.GetPixels();
                SaveBed("bed-with-distortion.png");
                var depthDistortion = Difference(undistorted, distorted);
                Assert.That(depthDistortion, Is.GreaterThan(.01), "The bed must visibly distort even when surface ripple normals are disabled.");
                material.SetFloat("_RefractionStrength", 0);
                Capture(.005f);
                var contact = image.GetPixels();
                material.SetFloat("_RefractionStrength", .012f);
                Capture(.005f);
                Assert.That(Difference(contact, image.GetPixels()), Is.LessThan(.001), "Distortion fades out at bank contact.");
                Debug.Log($"River patterned-bed distortion: {depthDistortion:F5}");
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderSettings.fog = oldFog;
                RenderTexture.active = previous;
                foreach (var item in new Object[] { river, bed, cameraObject, material, bedMaterial, target, image, noise, checker }) Object.DestroyImmediate(item);
            }
        }
    }
}
