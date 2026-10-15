using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class TreeNightLightingTests
    {
        [TestCase("Motu/Tree Foliage", false, false)]
        [TestCase("Motu/Tree Foliage Distant", false, false)]
        [TestCase("Motu/Tree Wood", false, false)]
        [TestCase("Motu/Tree Wood", true, false)]
        [TestCase("Motu/Tree Foliage", false, true)]
        [TestCase("Motu/Tree Foliage Distant", false, true)]
        [TestCase("Motu/Tree Wood", false, true)]
        public void TreeLightingDarkensAtMidnightAndStillReceivesMoonlight(
            string shaderName, bool noParallax, bool reflected)
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            var root = new GameObject("Tree night lighting fixture");
            var material = new Material(Shader.Find(shaderName));
            var target = new RenderTexture(64, 64, 24, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var pixels = new Texture2D(64, 64, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            var oldSun = RenderSettings.sun;
            var oldMatrix = Shader.GetGlobalMatrix("_IslandWorldToLocal");
            var oldFog = RenderSettings.fog;
            try
            {
                var tree = GameObject.CreatePrimitive(PrimitiveType.Cube);
                tree.transform.SetParent(root.transform);
                tree.transform.position = reflected ? Vector3.up * 2f : Vector3.zero;
                tree.layer = 31;
                tree.GetComponent<Renderer>().sharedMaterial = material;
                var cameraObject = new GameObject("Tree lighting camera");
                cameraObject.transform.SetParent(root.transform);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.cullingMask = 1 << 31;
                camera.transform.position = Vector3.back * 3;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.targetTexture = target;
                camera.allowHDR = true;
                camera.allowMSAA = false;
                PlanarWaterReflection reflection = null;
                if (reflected)
                {
                    // Exercise the simplified pass without the normal opaque pass overwriting it.
                    material.SetShaderPassEnabled("UniversalForwardOnly", false);
                    var plane = new GameObject("Tree reflection plane");
                    plane.transform.SetParent(root.transform);
                    reflection = cameraObject.AddComponent<PlanarWaterReflection>();
                    reflection.Configure(plane.transform);
                    var serialized = new SerializedObject(reflection);
                    serialized.FindProperty("frameInterval").intValue = 1;
                    serialized.ApplyModifiedPropertiesWithoutUndo();
                }
                var lightObject = new GameObject("Tree lighting sun");
                lightObject.transform.SetParent(root.transform);
                lightObject.layer = 31;
                var sun = lightObject.AddComponent<Light>();
                sun.type = LightType.Directional;
                material.SetColor("_BaseColor", Color.white);
                material.SetColor("_LightColor", Color.white);
                if (shaderName == "Motu/Tree Wood")
                {
                    material.SetTexture("_BarkAlbedoMap", Texture2D.whiteTexture);
                    material.SetTexture("_EndGrainMap", Texture2D.whiteTexture);
                    material.SetColor("_EndGrainColor", Color.white);
                    material.SetColor("_GroundDirtColor", Color.white);
                    material.SetFloat("_BarkNormalMapStrength", 0);
                    material.SetFloat("_BarkOcclusionStrength", 0);
                    if (noParallax) material.EnableKeyword("MOTU_TREE_BARK_NO_PARALLAX");
                }
                else
                {
                    material.SetFloat("_CanopyCoverage", 1);
                    material.SetFloat("_AlphaCutoff", 0);
                    material.SetFloat("_FoliageLeafCoverage", 0);
                    material.SetFloat("_FoliageTranslucency", 0);
                    material.SetFloat("_CullMode", (float)CullMode.Off);
                }
                material.SetFloat("_TreeNormalStrength", 0);
                material.SetFloat("_TreeHueVariationDegrees", 0);
                Shader.SetGlobalMatrix("_IslandWorldToLocal", Matrix4x4.identity);
                float Brightness(Color colour) => colour.r * .2126f + colour.g * .7152f + colour.b * .0722f;

                Color Capture(float hour, float moonIntensity)
                {
                    var environmentRoot = new GameObject("Tree lighting environment");
                    environmentRoot.transform.SetParent(root.transform);
                    try
                    {
                        var settings = new WorldEnvironmentSettings();
                        settings.AssignSceneReferences(sun, null);
                        JsonUtility.FromJsonOverwrite($"{{\"startingSolarTimeHours\":{hour},\"middaySunIntensity\":0,"
                            + $"\"startingMoonPhase\":0.5,\"fullMoonLightIntensity\":{moonIntensity}}}", settings);
                        var environment = environmentRoot.AddComponent<WorldEnvironmentController>();
                        environment.Initialize(settings, new IslandCloudSettings(), 64f, 128f, null);
                        environment.MoonLight.gameObject.layer = 31;
                        environment.MoonLight.shadows = LightShadows.None;
                        var moonDirection = -environment.MoonLight.transform.forward;
                        if (reflected)
                        {
                            camera.transform.position = Vector3.ProjectOnPlane(moonDirection, Vector3.up).normalized * 6f
                                + Vector3.up * 2f;
                            camera.transform.LookAt(Vector3.down * 2f);
                        }
                        else
                        {
                            camera.transform.position = moonDirection * 3f;
                            camera.transform.LookAt(Vector3.zero);
                        }
                        Shader.SetGlobalVector("_MotuWeatherWind", Vector4.zero);
                        RenderSettings.fog = false;
                        if (reflected) reflection.PrepareReflection();
                        else UrpTestCamera.Render(camera);
                        RenderTexture.active = reflected ? reflection.ReflectionCamera.targetTexture : target;
                        pixels.ReadPixels(new Rect(0, 0, 64, 64), 0, 0); pixels.Apply();
                        if (reflected)
                        {
                            var brightest = Color.black;
                            foreach (var pixel in pixels.GetPixels())
                                if (Brightness(pixel) > Brightness(brightest)) brightest = pixel;
                            return brightest;
                        }
                        return pixels.GetPixel(32, 32);
                    }
                    finally { Object.DestroyImmediate(environmentRoot); }
                }

                var day = Capture(12f, 0f);
                var night = Capture(0f, 0f);
                var moon = Capture(0f, .2f);
                Debug.Log($"{shaderName} no-parallax={noParallax} reflected={reflected}: day={day}, night={night}, moon={moon}");
                Assert.That(Brightness(day), Is.GreaterThan(.05f));
                Assert.That(Brightness(night), Is.LessThan(Brightness(day) * .1f),
                    "Tree ambient floors must not keep daytime brightness after sunset.");
                Assert.That(Brightness(moon), Is.GreaterThan(Brightness(night) * 2f),
                    "Moonlight must still illuminate the tree.");
                if (reflected) Assert.IsTrue(reflection.LastRenderUsedSimplifiedShader);
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally
            {
                RenderTexture.active = oldTarget;
                RenderSettings.sun = oldSun;
                RenderSettings.fog = oldFog;
                Shader.SetGlobalMatrix("_IslandWorldToLocal", oldMatrix);
                Object.DestroyImmediate(root); Object.DestroyImmediate(material);
                Object.DestroyImmediate(target); Object.DestroyImmediate(pixels);
            }
        }
    }
}
