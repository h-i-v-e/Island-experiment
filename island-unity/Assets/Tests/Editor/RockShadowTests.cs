using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Editor
{
    public sealed class RockShadowTests
    {
        [Test]
        public void RockDecorationReceivesMainAndAdditionalLightShadows()
        {
            var root = new GameObject("Rock shadow probe");
            var rock = new Material(Shader.Find("Motu/Rock Decoration"));
            var lit = new Material(Shader.Find("Universal Render Pipeline/Lit"));
            var target = RenderTexture.GetTemporary(128, 128, 24);
            var image = new Texture2D(128, 128, TextureFormat.RGBA32, false);
            var previousTarget = RenderTexture.active;
            var previousSun = RenderSettings.sun;
            var previousFog = RenderSettings.fog;
            var previousAmbient = RenderSettings.ambientLight;
            var previousAmbientMode = RenderSettings.ambientMode;
            try
            {
                RenderSettings.fog = false;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = Color.black;
                rock.SetColor("_RockColor", Color.white);
                rock.SetFloat("_CliffNormalStrength", 0f);
                lit.SetColor("_BaseColor", Color.white);

                var floor = GameObject.CreatePrimitive(PrimitiveType.Cube);
                floor.transform.SetParent(root.transform);
                floor.transform.position = new Vector3(0f, -0.1f, 0f);
                floor.transform.localScale = new Vector3(8f, 0.2f, 8f);
                floor.layer = 30;
                var floorRenderer = floor.GetComponent<MeshRenderer>();
                floorRenderer.receiveShadows = true;

                var blocker = GameObject.CreatePrimitive(PrimitiveType.Cube);
                blocker.transform.SetParent(root.transform);
                blocker.transform.position = new Vector3(0f, 1.5f, 0f);
                blocker.transform.localScale = new Vector3(1.5f, 1.5f, 1.5f);
                blocker.layer = 30;
                blocker.GetComponent<MeshRenderer>().shadowCastingMode = ShadowCastingMode.ShadowsOnly;

                var lightObject = new GameObject("Rock shadow probe sun");
                lightObject.transform.SetParent(root.transform);
                lightObject.transform.rotation = Quaternion.Euler(45f, 0f, 0f);
                var sun = lightObject.AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 2f;
                sun.shadows = LightShadows.Hard;
                sun.shadowStrength = 1f;
                RenderSettings.sun = sun;

                var cameraObject = new GameObject("Rock shadow probe camera");
                cameraObject.transform.SetParent(root.transform);
                cameraObject.transform.position = new Vector3(0f, 10f, 0f);
                cameraObject.transform.rotation = Quaternion.Euler(90f, 0f, 0f);
                var camera = cameraObject.AddComponent<Camera>();
                camera.enabled = false;
                camera.orthographic = true;
                camera.orthographicSize = 5f;
                camera.cullingMask = 1 << 30;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.blue;
                camera.targetTexture = target;
                camera.allowHDR = false;

                Color[] Capture(Material material, Light light, LightShadows shadows)
                {
                    floorRenderer.sharedMaterial = material;
                    light.shadows = shadows;
                    UrpTestCamera.Render(camera);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0f, 0f, 128f, 128f), 0, 0);
                    image.Apply();
                    return image.GetPixels();
                }

                int DarkenedPixels(Material material, Light light)
                {
                    var unshadowed = Capture(material, light, LightShadows.None);
                    var shadowed = Capture(material, light, LightShadows.Hard);
                    var count = 0;
                    for (var index = 0; index < unshadowed.Length; index++)
                        if (unshadowed[index].r > 0.2f
                            && unshadowed[index].r - shadowed[index].r > 0.08f)
                            count++;
                    return count;
                }

                Assert.That(DarkenedPixels(lit, sun), Is.GreaterThan(50),
                    "The URP test light must cast a visible shadow on its reference material.");
                Assert.That(DarkenedPixels(rock, sun), Is.GreaterThan(50),
                    "The rock shader must receive the same main-light shadow.");

                float ShadowContrastAt(float cameraHeight)
                {
                    camera.transform.position = new Vector3(0f, cameraHeight, 0f);
                    var unshadowed = Capture(rock, sun, LightShadows.None);
                    var shadowed = Capture(rock, sun, LightShadows.Hard);
                    var contrast = 0f;
                    for (var index = 0; index < unshadowed.Length; index++)
                        if (unshadowed[index].r > 0.2f)
                            contrast += Mathf.Max(0f, unshadowed[index].r - shadowed[index].r);
                    return contrast;
                }

                var nearShadow = ShadowContrastAt(120f);
                var fadingShadow = ShadowContrastAt(125f);
                var beyondShadow = ShadowContrastAt(130f);
                Assert.That(nearShadow, Is.GreaterThan(fadingShadow + 1f),
                    "Main-light shadows should fade as the rock approaches the shadow distance.");
                Assert.That(fadingShadow, Is.GreaterThan(beyondShadow + 1f),
                    "The rock should not jump from a full shadow to no shadow at the range limit.");

                camera.transform.position = new Vector3(0f, 10f, 0f);
                sun.intensity = 0f;
                sun.shadows = LightShadows.None;
                var spotObject = new GameObject("Rock shadow probe spot light");
                spotObject.transform.SetParent(root.transform);
                spotObject.transform.position = new Vector3(0f, 5f, 0f);
                spotObject.transform.rotation = Quaternion.Euler(90f, 0f, 0f);
                var spot = spotObject.AddComponent<Light>();
                spot.type = LightType.Spot;
                spot.range = 20f;
                spot.spotAngle = 80f;
                spot.intensity = 10f;
                spot.shadows = LightShadows.Hard;
                spot.cullingMask = 1 << 30;

                Assert.That(DarkenedPixels(lit, spot), Is.GreaterThan(50),
                    "The URP spot light must cast a visible shadow on its reference material.");
                Assert.That(DarkenedPixels(rock, spot), Is.GreaterThan(50),
                    "The rock shader must receive additional-light shadows.");
            }
            finally
            {
                RenderSettings.sun = previousSun;
                RenderSettings.fog = previousFog;
                RenderSettings.ambientLight = previousAmbient;
                RenderSettings.ambientMode = previousAmbientMode;
                RenderTexture.active = previousTarget;
                RenderTexture.ReleaseTemporary(target);
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(rock);
                Object.DestroyImmediate(lit);
                Object.DestroyImmediate(image);
            }
        }
    }
}
