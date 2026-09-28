using System.Linq;
using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using UnityEngine.SceneManagement;

namespace Motu.Editor
{
    public sealed class UrpRenderingTests
    {
        [Test]
        public void OpenSeaWorldEnablesAmbientOcclusionOnBothPlayableCameras()
        {
            var scene = EditorSceneManager.OpenScene(
                "Assets/Scenes/OpenSeaWorld.unity", OpenSceneMode.Additive);
            try
            {
                var cameras = scene.GetRootGameObjects()
                    .SelectMany(root => root.GetComponentsInChildren<Camera>(true))
                    .ToArray();
                foreach (var name in new[] { "Main Camera", "Ship Bridge Camera" })
                {
                    var camera = cameras.Single(candidate => candidate.name == name);
                    var effects = camera.GetComponents<RealTimeAmbientOcclusion>();
                    Assert.That(effects.Length, Is.EqualTo(1), name);
                    Assert.IsTrue(effects[0].enabled, name);
                }
            }
            finally { EditorSceneManager.CloseScene(scene, true); }
        }

        [Test]
        public void EveryQualityLevelUsesTheConfiguredPipeline()
        {
            var pipeline = GraphicsSettings.defaultRenderPipeline as UniversalRenderPipelineAsset;
            Assert.NotNull(pipeline);
            Assert.IsTrue(pipeline.supportsCameraDepthTexture);
            Assert.IsTrue(pipeline.supportsCameraOpaqueTexture);
            var previous = QualitySettings.GetQualityLevel();
            try
            {
                for (var index = 0; index < QualitySettings.names.Length; index++)
                {
                    QualitySettings.SetQualityLevel(index, false);
                    Assert.AreSame(pipeline, QualitySettings.renderPipeline);
                }
            }
            finally { QualitySettings.SetQualityLevel(previous, false); }
        }

        [Test]
        public void AmbientOcclusionDarkensContactsWithoutDarkeningTheClearBackground()
        {
            var root = new GameObject("URP AO test");
            var material = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
            material.SetColor("_BaseColor", Color.white);
            var target = RenderTexture.GetTemporary(128, 128, 24);
            var image = new Texture2D(128, 128, TextureFormat.RGBA32, false);
            var previous = RenderTexture.active;
            try
            {
                var floor = GameObject.CreatePrimitive(PrimitiveType.Cube);
                floor.transform.SetParent(root.transform);
                floor.layer = 31;
                floor.transform.position = new Vector3(0, -.2f, 0);
                floor.transform.localScale = new Vector3(8, .4f, 8);
                floor.GetComponent<Renderer>().sharedMaterial = material;
                var box = GameObject.CreatePrimitive(PrimitiveType.Cube);
                box.transform.SetParent(root.transform);
                box.layer = 31;
                box.transform.position = Vector3.up;
                box.transform.localScale = new Vector3(2, 2, 2);
                box.GetComponent<Renderer>().sharedMaterial = material;
                var cameraObject = new GameObject("AO camera");
                cameraObject.transform.SetParent(root.transform);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.blue;
                camera.transform.position = new Vector3(4, 4, -6);
                camera.transform.LookAt(Vector3.up);
                camera.targetTexture = target;
                camera.allowHDR = false;
                var ao = cameraObject.AddComponent<RealTimeAmbientOcclusion>();
                Color[] Capture(bool enabled)
                {
                    ao.enabled = enabled;
                    UrpTestCamera.Render(camera);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 128, 128), 0, 0);
                    image.Apply();
                    return image.GetPixels();
                }
                var dry = Capture(false);
                var shaded = Capture(true);
                var darker = 0;
                for (var index = 0; index < dry.Length; index++)
                {
                    if (dry[index].r > .9f && dry[index].r - shaded[index].r > .02f) darker++;
                    if (dry[index].b > .9f && dry[index].r < .01f)
                        Assert.That(shaded[index].b, Is.GreaterThan(.98f), "AO must not darken the sky.");
                }
                Assert.That(darker, Is.GreaterThan(100), "The contact region must receive occlusion.");
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(material);
                Object.DestroyImmediate(image);
                RenderTexture.ReleaseTemporary(target);
            }
        }

        [Test]
        public void ShellsRespectOpaqueDepthAndRefreshCameraDepthForAmbientOcclusion()
        {
            var shellShader = Shader.Find("Hidden/Motu/Tests/Shell Depth");
            var readbackShader = Shader.Find("Hidden/Motu/Tests/Camera Depth Readback");
            Assert.NotNull(shellShader);
            Assert.NotNull(readbackShader);
            var root = new GameObject("Shell depth test");
            var shellMaterial = new Material(shellShader);
            var readbackMaterial = new Material(readbackShader);
            var occluderMaterial = new Material(Shader.Find("Universal Render Pipeline/Unlit"));
            occluderMaterial.SetColor("_BaseColor", Color.blue);
            var target = RenderTexture.GetTemporary(64, 64, 24);
            var image = new Texture2D(64, 64, TextureFormat.RGBA32, false);
            var previous = RenderTexture.active;
            try
            {
                var shell = GameObject.CreatePrimitive(PrimitiveType.Quad);
                shell.transform.SetParent(root.transform);
                shell.transform.position = Vector3.forward * 4;
                shell.transform.localScale = Vector3.one * 4;
                shell.layer = 31;
                shell.GetComponent<Renderer>().sharedMaterial = shellMaterial;

                var occluder = GameObject.CreatePrimitive(PrimitiveType.Cube);
                occluder.transform.SetParent(root.transform);
                occluder.transform.position = Vector3.forward * 3;
                occluder.layer = 31;
                occluder.GetComponent<Renderer>().sharedMaterial = occluderMaterial;

                var probe = GameObject.CreatePrimitive(PrimitiveType.Quad);
                probe.transform.SetParent(root.transform);
                probe.transform.position = Vector3.forward * 2;
                probe.transform.localScale = Vector3.one * 4;
                probe.layer = 31;
                probe.GetComponent<Renderer>().sharedMaterial = readbackMaterial;
                probe.SetActive(false);

                var cameraObject = new GameObject("Shell depth camera");
                cameraObject.transform.SetParent(root.transform);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.cullingMask = 1 << 31;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.targetTexture = target;
                camera.allowHDR = false;
                cameraObject.AddComponent<RealTimeAmbientOcclusion>();

                Color CenterPixel()
                {
                    UrpTestCamera.Render(camera);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 64, 64), 0, 0);
                    image.Apply();
                    return image.GetPixel(32, 32);
                }

                var blocked = CenterPixel();
                Assert.That(blocked.b, Is.GreaterThan(blocked.r * 2),
                    "An opaque object in front must hide a shell pass.");

                occluder.SetActive(false);
                probe.SetActive(true);
                var copiedDepth = CenterPixel();
                Assert.That(copiedDepth.r, Is.InRange(0.5f, 0.7f),
                    "The sampled camera depth must contain the shell at four metres.");
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(shellMaterial);
                Object.DestroyImmediate(readbackMaterial);
                Object.DestroyImmediate(occluderMaterial);
                Object.DestroyImmediate(image);
                RenderTexture.ReleaseTemporary(target);
            }
        }

        [Test]
        public void CustomShaderPassesCompileOnTheCurrentGraphicsDevice()
        {
            var previous = RenderTexture.active;
            var target = RenderTexture.GetTemporary(16, 16, 24);
            RenderTexture.active = target;
            try
            {
                foreach (var guid in AssetDatabase.FindAssets("t:Shader", new[] { "Packages/com.motu.runtime/Runtime/Shaders" }))
                {
                    var shader = AssetDatabase.LoadAssetAtPath<Shader>(AssetDatabase.GUIDToAssetPath(guid));
                    var material = new Material(shader);
                    try
                    {
                        Assert.IsTrue(shader.isSupported, shader.name);
                        for (var pass = 0; pass < material.passCount; pass++)
                            Assert.IsTrue(material.SetPass(pass), $"{shader.name} pass {pass}");
                        var errors = ShaderUtil.GetShaderMessages(shader)
                            .Where(message => message.severity == UnityEditor.Rendering.ShaderCompilerMessageSeverity.Error);
                        Assert.IsEmpty(errors.Select(message => $"{shader.name}: {message.message} ({message.file}:{message.line})"));
                    }
                    finally { Object.DestroyImmediate(material); }
                }
            }
            finally { RenderTexture.active = previous; RenderTexture.ReleaseTemporary(target); }
        }
    }
}
