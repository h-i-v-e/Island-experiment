using System;
using Motu.Islands;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class WheatClumpRenderingTests
    {
        [Test]
        public void ClumpMaterialRetainsInstancingAndCompilesAllPasses()
        {
            var material = Resources.Load<Material>("Motu/Variants/WheatClumps");
            Assert.IsNotNull(material);
            Assert.AreEqual("Motu/Wheat Clumps", material.shader.name);
            Assert.IsTrue(material.enableInstancing);
            foreach (var name in new[] { "Forward", "ShadowCaster", "DepthOnly", "Reflection" })
            {
                var pass = material.FindPass(name);
                Assert.GreaterOrEqual(pass, 0, name);
                ShaderUtil.CompilePass(material, pass, true);
            }
            Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
        }

        [Test]
        public void TileBatchesFollowIslandAndRespondToFadeAndSharedWind()
        {
            using var scene = new RenderScene();
            var key = new Vector2Int(3, 4);
            var plants = new WheatPlant[132];
            for (var i = 0; i < 128; ++i)
                plants[i] = new WheatPlant(new Vector3(-80f + i % 4, 0, 0), 0f, .75f, .4f, .5f);
            for (var i = 128; i < plants.Length; ++i)
                plants[i] = new WheatPlant(new Vector3((i - 129.5f) * .48f, 0, 0), .4f, .8f, .4f, (i - 128) * .33f);
            scene.Renderer.SetTile(key, plants);
            Assert.AreEqual(1, scene.Owner.GetComponentsInChildren<Transform>().Length);
            var rest = scene.Render();
            Assert.Greater(rest.count, 100, "The visible second batch must render golden wheat.");

            Shader.SetGlobalVector("_MotuWindMaterial", new Vector4(1.4f, 24f, .35f, 0));
            Shader.SetGlobalVector("_MotuWeatherWind", new Vector4(1f, 0, 10f, 1f));
            var blown = scene.Render();
            Assert.Greater(blown.x, rest.x + 5f, "The shared wind bends the clump tops along +X.");

            scene.Renderer.SetTileFade(key, .5f);
            var fading = scene.Render();
            Assert.That(fading.count, Is.InRange(blown.count * .30f, blown.count * .70f));
            scene.Renderer.SetTileFade(key, 0f);
            Assert.AreEqual(0, scene.Render().count);
            scene.Renderer.SetTileFade(key, 1f);

            var shift = new Vector3(-90f, 12f, 8f);
            scene.Owner.transform.position += shift;
            Assert.AreEqual(0, scene.Render().count);
            scene.Camera.transform.position += shift;
            Assert.Greater(scene.Render().count, 100, "Floating-origin moves must refresh the GPU matrices and bounds.");

            scene.Renderer.SetTile(key, null);
            Assert.AreEqual(0, scene.Render().count);
            scene.Renderer.SetTile(key, plants);
            Assert.Greater(scene.Render().count, 100);
            scene.Renderer.Clear();
            Assert.AreEqual(0, scene.Render().count);
        }

        [Test]
        public void DetailedWheatGivesWayToTheCanopyAcrossTheFadeBand()
        {
            using var scene = new RenderScene();
            var key = Vector2Int.zero;
            scene.Renderer.SetTile(key, new[] {
                new WheatPlant(new Vector3(-.48f, 0, 0), 0f, .78f, .4f, .3f),
                new WheatPlant(Vector3.zero, .4f, .8f, .4f, .6f),
                new WheatPlant(new Vector3(.48f, 0, 0), .8f, .82f, .4f, .9f),
            });
            scene.Renderer.SetTileBlend(key, true);
            var canopy = GameObject.CreatePrimitive(PrimitiveType.Quad);
            canopy.layer = scene.Owner.layer;
            canopy.transform.SetParent(scene.Owner.transform, false);
            canopy.transform.localPosition = Vector3.up * .4f;
            canopy.transform.localRotation = Quaternion.Euler(90f, 0f, 0f);
            canopy.transform.localScale = Vector3.one * 1.6f;
            var material = new Material(Resources.Load<Shader>("WheatCanopy"));
            canopy.GetComponent<MeshRenderer>().sharedMaterial = material;
            material.SetFloat("_WheatPorosity", 0f);
            material.SetFloat("_WheatDetailPresence", 1f);
            try
            {
                (int plants, int canopy) Sample(float distance)
                {
                    var origin = scene.Owner.transform.position;
                    var blend = new Vector4(origin.x + distance, origin.z, 10f, 20f);
                    scene.Renderer.DetailBlend = blend;
                    material.SetVector("_WheatDetailBlend", blend);
                    canopy.SetActive(false);
                    var plants = scene.Render().count;
                    canopy.SetActive(true);
                    scene.Renderer.SetTileFade(key, 0f);
                    var cover = scene.Render(goldOnly: false).count;
                    scene.Renderer.SetTileFade(key, 1f);
                    return (plants, cover);
                }

                var near = Sample(0f);
                var middle = Sample(15f);
                var far = Sample(24f);
                Assert.That(near.plants, Is.GreaterThan(100));
                Assert.That(near.canopy, Is.Zero);
                Assert.That(far.plants, Is.Zero);
                Assert.That(far.canopy, Is.GreaterThan(100));
                Assert.That(middle.plants, Is.InRange(near.plants * .30f, near.plants * .70f));
                Assert.That(middle.canopy, Is.InRange(far.canopy * .30f, far.canopy * .70f));

                canopy.SetActive(false);
                scene.Renderer.SetTileBlend(key, false);
                Assert.That(scene.Render().count, Is.EqualTo(near.plants), "Keep clumps if no canopy mesh is resident.");
            }
            finally
            {
                Object.DestroyImmediate(canopy);
                Object.DestroyImmediate(material);
            }
        }

        private sealed class RenderScene : IDisposable
        {
            private const int Layer = 30;
            private readonly Color savedAmbient = RenderSettings.ambientLight;
            private readonly AmbientMode savedAmbientMode = RenderSettings.ambientMode;
            private readonly bool savedFog = RenderSettings.fog;
            private readonly Light savedSun = RenderSettings.sun;
            private readonly RenderTexture savedTarget = RenderTexture.active;
            private readonly Vector4 savedWind = Shader.GetGlobalVector("_MotuWeatherWind");
            private readonly Vector4 savedWindMaterial = Shader.GetGlobalVector("_MotuWindMaterial");
            private readonly float savedCloud = Shader.GetGlobalFloat("_MotuCloudEnabled");
            private readonly float savedNight = Shader.GetGlobalFloat("_MotuCloudNightStrength");
            private readonly RenderTexture target = new RenderTexture(480, 320, 24) { antiAliasing = 1 };
            private readonly Texture2D pixels = new Texture2D(480, 320, TextureFormat.RGB24, false);
            private readonly Light sun = new GameObject("Wheat test sun").AddComponent<Light>();
            internal readonly GameObject Owner = new GameObject("Wheat test island") { layer = Layer };
            internal readonly Camera Camera = new GameObject("Wheat test camera").AddComponent<Camera>();
            internal readonly WheatClumpRenderer Renderer;

            internal RenderScene()
            {
                Renderer = new WheatClumpRenderer(Owner.transform, Resources.Load<Material>("Motu/Variants/WheatClumps"));
                Owner.transform.SetPositionAndRotation(new Vector3(21000, 6f, -18000), Quaternion.Euler(0, 26, 0));
                Camera.enabled = false;
                Camera.cullingMask = 1 << Layer;
                Camera.clearFlags = CameraClearFlags.SolidColor;
                Camera.backgroundColor = Color.black;
                Camera.fieldOfView = 54;
                Camera.targetTexture = target;
                Camera.transform.SetPositionAndRotation(Owner.transform.position + new Vector3(0, .65f, -2.8f),
                    Quaternion.Euler(5, 0, 0));
                sun.type = LightType.Directional;
                sun.intensity = 1.5f;
                sun.transform.rotation = Quaternion.Euler(36, -45, 0);
                RenderSettings.sun = sun;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = new Color(.5f, .5f, .5f);
                RenderSettings.fog = false;
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0f);
                Shader.SetGlobalFloat("_MotuCloudNightStrength", 0f);
                Shader.SetGlobalVector("_MotuWeatherWind", Vector4.zero);
            }

            internal (int count, float x) Render(bool goldOnly = true)
            {
                Camera.Render();
                RenderTexture.active = target;
                pixels.ReadPixels(new Rect(0, 0, pixels.width, pixels.height), 0, 0);
                var count = 0;
                var totalX = 0;
                var colours = pixels.GetPixels32();
                for (var i = 0; i < colours.Length; ++i)
                {
                    var pixel = colours[i];
                    if (pixel.r < 15 || (goldOnly && (pixel.r <= pixel.g * 1.05f || pixel.g <= pixel.b * 1.3f))) continue;
                    ++count;
                    totalX += i % pixels.width;
                }
                return (count, count == 0 ? 0f : totalX / (float)count);
            }

            public void Dispose()
            {
                Renderer.Dispose();
                RenderSettings.ambientMode = savedAmbientMode;
                RenderSettings.ambientLight = savedAmbient;
                RenderSettings.sun = savedSun;
                RenderSettings.fog = savedFog;
                Shader.SetGlobalVector("_MotuWeatherWind", savedWind);
                Shader.SetGlobalVector("_MotuWindMaterial", savedWindMaterial);
                Shader.SetGlobalFloat("_MotuCloudEnabled", savedCloud);
                Shader.SetGlobalFloat("_MotuCloudNightStrength", savedNight);
                RenderTexture.active = savedTarget;
                Camera.targetTexture = null;
                target.Release();
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(pixels);
                Object.DestroyImmediate(Camera.gameObject);
                Object.DestroyImmediate(sun.gameObject);
                Object.DestroyImmediate(Owner);
            }
        }
    }
}
