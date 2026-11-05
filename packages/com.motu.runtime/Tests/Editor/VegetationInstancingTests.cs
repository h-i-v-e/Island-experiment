using System;
using System.Linq;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class VegetationInstancingTests
    {
        [TestCase("Motu/Riverbank Reeds", 1f)]
        [TestCase("Motu/Riverbank Reeds", 15f)]
        [TestCase("Motu/Riverbank Reeds", 35f)]
        [TestCase("Motu/Forest Ferns", 1f)]
        [TestCase("Motu/Forest Ferns", 15f)]
        [TestCase("Motu/Forest Ferns", 35f)]
        public void InstancedVegetationRemainsVisibleAwayFromOriginWhileTurning(string shaderName, float distance)
        {
            using var scene = new VegetationScene(shaderName, distance);
            for (var yaw = -25; yaw <= 25; yaw += 5)
            {
                scene.Camera.transform.rotation = Quaternion.Euler(10, yaw, 0);
                var reference = scene.Render(false);
                var instanced = scene.Render(true);
                Assert.Greater(reference, 20, "The non-instanced control must contain visible vegetation.");
                Assert.That(instanced, Is.EqualTo(reference).Within(Mathf.Max(3, reference * .05f)),
                    $"{shaderName} at {distance} m, yaw {yaw}: instancing must use the island transform, not the origin.");
            }
            TestContext.Out.WriteLine($"{shaderName} at {distance} m: all eleven camera angles retain the same instanced silhouette.");
        }

        [TestCase("Motu/Riverbank Reeds")]
        [TestCase("Motu/Forest Ferns")]
        public void InstancedColourAndShadowPassesCompile(string shaderName)
        {
            var material = new Material(Shader.Find(shaderName)) { enableInstancing = true };
            try
            {
                for (var pass = 0; pass < material.passCount; pass++) ShaderUtil.CompilePass(material, pass, true);
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader), shaderName);
            }
            finally { Object.DestroyImmediate(material); }
        }

        private sealed class VegetationScene : IDisposable
        {
            private readonly GameObject root;
            private readonly Material material;
            private readonly Mesh mesh;
            private readonly Light sun;
            private readonly RenderTexture target;
            private readonly Texture2D image;
            private readonly Vector4 oldWind;
            private readonly Color oldAmbient;
            private readonly AmbientMode oldAmbientMode;
            private readonly RenderTexture oldTarget;
            internal Camera Camera { get; }

            internal VegetationScene(string shaderName, float distance)
            {
                oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
                oldAmbient = RenderSettings.ambientLight; oldAmbientMode = RenderSettings.ambientMode;
                oldTarget = RenderTexture.active;
                var origin = new Vector3(20000, 0, -16000);
                root = new GameObject("Instanced vegetation away from origin") { layer = 30 };
                root.transform.position = origin;
                material = new Material(Shader.Find(shaderName));
                material.SetFloat("_WorldSize", 2000);
                material.SetVector("_GrassPlayerPosition", origin + Vector3.up * 1.5f);
                mesh = new Mesh
                {
                    vertices = new[] { new Vector3(-.65f, 0, distance), new Vector3(.65f, 0, distance),
                        new Vector3(-.65f, 2, distance), new Vector3(.65f, 2, distance) },
                    normals = Enumerable.Repeat(Vector3.back, 4).ToArray(),
                    uv = new[] { Vector2.zero, Vector2.right, Vector2.up, Vector2.one },
                    uv2 = Enumerable.Repeat(new Vector2(.5f, .5f), 4).ToArray(),
                    colors = Enumerable.Repeat(new Color(0, .5f, 1, .3f), 4).ToArray(),
                    triangles = new[] { 0, 2, 1, 1, 2, 3 },
                };
                mesh.RecalculateBounds();
                root.AddComponent<MeshFilter>().sharedMesh = mesh;
                root.AddComponent<MeshRenderer>().sharedMaterial = material;
                Camera = new GameObject("Vegetation instancing camera").AddComponent<Camera>();
                Camera.transform.position = origin + Vector3.up * 1.5f;
                Camera.cullingMask = 1 << 30; Camera.fieldOfView = 70;
                Camera.clearFlags = CameraClearFlags.SolidColor; Camera.backgroundColor = Color.black;
                sun = new GameObject("Vegetation instancing sun").AddComponent<Light>();
                sun.type = LightType.Directional; sun.intensity = 3;
                sun.transform.rotation = Quaternion.Euler(30, -20, 0);
                RenderSettings.ambientMode = AmbientMode.Flat; RenderSettings.ambientLight = Color.white;
                Shader.SetGlobalVector("_MotuWeatherWind", Vector4.zero);
                target = new RenderTexture(480, 360, 24) { antiAliasing = 2 };
                image = new Texture2D(480, 360, TextureFormat.RGB24, false);
                Camera.targetTexture = target;
            }

            internal int Render(bool instancing)
            {
                material.enableInstancing = instancing;
                Camera.Render(); RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0); image.Apply();
                return image.GetPixels32().Count(pixel => pixel.g > pixel.r * 1.2f && pixel.g > pixel.b * 1.2f && pixel.g > 10);
            }

            public void Dispose()
            {
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                RenderSettings.ambientLight = oldAmbient; RenderSettings.ambientMode = oldAmbientMode;
                RenderTexture.active = oldTarget; Camera.targetTexture = null;
                target.Release(); Object.DestroyImmediate(target); Object.DestroyImmediate(image);
                Object.DestroyImmediate(root); Object.DestroyImmediate(mesh); Object.DestroyImmediate(material);
                Object.DestroyImmediate(Camera.gameObject); Object.DestroyImmediate(sun.gameObject);
            }
        }
    }
}
