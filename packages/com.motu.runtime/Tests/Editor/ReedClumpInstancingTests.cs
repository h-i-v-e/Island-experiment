using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using Motu.Streaming;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class ReedClumpInstancingTests
    {
        [Test]
        public void PlayerBuildRetainsClumpInstancingVariant()
        {
            var material = Resources.Load<Material>("Motu/Variants/ReedClumps");
            Assert.IsNotNull(material);
            Assert.AreEqual("Motu/Riverbank Reeds", material.shader.name);
            Assert.IsTrue(material.enableInstancing);
            Assert.IsTrue(material.IsKeywordEnabled("MOTU_REED_CLUMPS"));
        }

        [Test]
        public void CompactReedInstanceAbiRemainsFortyEightBytes()
        {
            Assert.AreEqual(48, Marshal.SizeOf<ReedInstancesNative.Instance>());
        }

        [Test]
        public void ClumpColourShadowAndReflectionVariantsCompile()
        {
            var material = new Material(Shader.Find("Motu/Riverbank Reeds")) { enableInstancing = true };
            try
            {
                material.EnableKeyword("MOTU_REED_CLUMPS");
                Assert.GreaterOrEqual(material.FindPass("ReflectionReeds"), 0);
                for (var pass = 0; pass < material.passCount; pass++) ShaderUtil.CompilePass(material, pass, true);
                Assert.IsFalse(ShaderUtil.ShaderHasError(material.shader));
            }
            finally { Object.DestroyImmediate(material); }
        }

        [TestCase(false)]
        [TestCase(true)]
        public void RealClumpBatchesMatchBakedSlopedReedsAwayFromOriginWhileTurning(bool simplifiedReflection)
        {
            using var pipeline = new ReedPipeline();
            using var scene = new ReedScene(simplifiedReflection);
            foreach (var yaw in new[] { -20, -10, 0, 10, 20 })
            {
                scene.Aim(yaw);
                var baked = scene.Render(false);
                var instanced = scene.Render(true);
                var visible = baked.Count(IsGreen);
                Assert.Greater(visible, 50, "The baked native-card control must be visible.");
                var difference = baked.Zip(instanced, (a, b) => IsGreen(a) != IsGreen(b)).Count(changed => changed);
                Assert.LessOrEqual(difference, Mathf.Max(15, visible * .08f),
                    $"Reflection={simplifiedReflection}, yaw={yaw}: per-instance transforms, slope and silhouette data must match baked cards.");
                var sharedPixels = baked.Zip(instanced, (a, b) => (a, b)).Where(pair => IsGreen(pair.a) && IsGreen(pair.b)).ToArray();
                Assert.IsNotEmpty(sharedPixels);
                var colourDifference = sharedPixels.Average(pair => Math.Abs(pair.a.r - pair.b.r)
                    + Math.Abs(pair.a.g - pair.b.g) + Math.Abs(pair.a.b - pair.b.b));
                Assert.LessOrEqual(colourDifference, 15, "Instanced normals, card winding and tint must preserve the baked colour.");
                Assert.Greater(scene.Streamer.SubmittedBatchCount, 0,
                    "Camera.Render must submit the actual streamer through beginCameraRendering.");
            }
            Assert.AreEqual(8, scene.Streamer.ActiveInstanceCount);
            Assert.AreEqual(32, scene.Streamer.SharedVertexCount,
                "All placements share the native four-plane, four-level card prototype.");
            Assert.AreEqual(0, scene.Island.GetComponentsInChildren<MeshRenderer>().Count(renderer => renderer != scene.Control),
                "Instanced tiles must not allocate a MeshRenderer per tile or clump.");
        }

        [Test]
        public void RealClumpShadowBatchesMatchBakedReedShadowsOnReceiver()
        {
            using var pipeline = new ReedPipeline(ensureShadows: true);
            using var scene = new ReedScene(false);
            scene.ConfigureShadowReceiver();
            var baseline = scene.Render(false, reedsVisible: false);
            var baked = scene.Render(false);
            var instanced = scene.Render(true);
            var bakedShadows = ReceiverShadowMask(baseline, baked);
            var instancedShadows = ReceiverShadowMask(baseline, instanced);
            var shadowPixels = bakedShadows.Count(pixel => pixel);
            Assert.Greater(shadowPixels, 50,
                "The baked control must cast a real directional-light shadow on the white URP/Lit receiver.");
            Assert.Greater(instancedShadows.Count(pixel => pixel), 50,
                "Actual instanced shadow submissions must darken the receiver.");
            var difference = bakedShadows.Zip(instancedShadows, (a, b) => a != b).Count(changed => changed);
            Assert.LessOrEqual(difference, Mathf.Max(15, shadowPixels * .08f),
                "Instanced shadow silhouettes must retain each clump's dimensions, bank slope and material variation.");
            var sharedShadows = Enumerable.Range(0, baked.Length)
                .Where(index => bakedShadows[index] && instancedShadows[index]).ToArray();
            Assert.IsNotEmpty(sharedShadows);
            var colourDifference = sharedShadows.Average(index => Math.Abs(baked[index].r - instanced[index].r)
                + Math.Abs(baked[index].g - instanced[index].g) + Math.Abs(baked[index].b - instanced[index].b));
            Assert.LessOrEqual(colourDifference, 15, "Receiver shadow darkness must match the baked control.");
            Assert.Greater(scene.Streamer.SubmittedBatchCount, 0);
            TestContext.Out.WriteLine($"Reed receiver shadows: baked={shadowPixels}, instanced={instancedShadows.Count(pixel => pixel)}, differing={difference}.");
        }

        private static bool[] ReceiverShadowMask(Color32[] baseline, Color32[] shadowed)
        {
            return baseline.Zip(shadowed, (light, dark) => light.r > 60 && light.r - dark.r > 25
                && Math.Abs(dark.r - dark.g) < 15 && Math.Abs(dark.r - dark.b) < 15).ToArray();
        }

        private sealed class ReedScene : IDisposable
        {
            internal readonly GameObject Island;
            internal readonly ReedTileStreamer Streamer = new ReedTileStreamer();
            internal readonly MeshRenderer Control;
            private readonly Camera camera;
            private readonly Light sun;
            private readonly Mesh bakedMesh;
            private readonly Material instancedMaterial, bakedMaterial;
            private readonly RenderTexture target, oldTarget;
            private readonly Texture2D image;
            private readonly Vector4 oldWind;
            private readonly Color oldAmbient;
            private readonly AmbientMode oldAmbientMode;
            private readonly bool oldFog;
            private readonly Light oldSun;
            private readonly float oldCloudEnabled, oldNightStrength;
            private Material receiverMaterial;

            internal ReedScene(bool simplifiedReflection)
            {
                oldTarget = RenderTexture.active;
                oldWind = Shader.GetGlobalVector("_MotuWeatherWind");
                oldAmbient = RenderSettings.ambientLight;
                oldAmbientMode = RenderSettings.ambientMode;
                oldFog = RenderSettings.fog;
                oldSun = RenderSettings.sun;
                oldCloudEnabled = Shader.GetGlobalFloat("_MotuCloudEnabled");
                oldNightStrength = Shader.GetGlobalFloat("_MotuCloudNightStrength");
                Island = new GameObject("Sloped instanced reed test island") { layer = 30 };
                Island.transform.SetPositionAndRotation(new Vector3(20000, 0, -16000), Quaternion.Euler(0, 31, 0));
                Island.transform.localScale = Vector3.one * 1.15f;
                instancedMaterial = NewMaterial();
                bakedMaterial = NewMaterial();
                var placements = Placements();
                var tiles = new IslandPreparedReedTile[ReedTileStreamer.TileCount];
                tiles[32 * ReedTileStreamer.Resolution + 32] = new IslandPreparedReedTile(placements);
                Streamer.Initialize(Island.transform, instancedMaterial, tiles, true);
                Streamer.UpdateLod0Neighborhood(new Vector2Int(32, 32));
                bakedMesh = BakeNativeCards(placements);
                var controlObject = new GameObject("Native baked reed control") { layer = 30 };
                controlObject.transform.SetParent(Island.transform, false);
                controlObject.AddComponent<MeshFilter>().sharedMesh = bakedMesh;
                Control = controlObject.AddComponent<MeshRenderer>();
                Control.sharedMaterial = bakedMaterial;
                camera = new GameObject("Clump render camera").AddComponent<Camera>();
                camera.enabled = false;
                camera.cullingMask = 1 << 30;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = Color.black;
                camera.fieldOfView = 70;
                camera.GetUniversalAdditionalCameraData().SetRenderer(simplifiedReflection ? 1 : 0);
                sun = new GameObject("Clump render sun").AddComponent<Light>();
                sun.type = LightType.Directional;
                sun.intensity = 3;
                sun.transform.rotation = Quaternion.Euler(30, -20, 0);
                RenderSettings.sun = sun;
                RenderSettings.ambientMode = AmbientMode.Flat;
                RenderSettings.ambientLight = Color.white;
                RenderSettings.fog = false;
                Shader.SetGlobalVector("_MotuWeatherWind", Vector4.zero);
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalFloat("_MotuCloudNightStrength", 0);
                target = new RenderTexture(480, 360, 24) { antiAliasing = 1 };
                image = new Texture2D(480, 360, TextureFormat.RGB24, false);
                camera.targetTexture = target;
            }

            internal void Aim(float yaw)
            {
                camera.transform.SetPositionAndRotation(Island.transform.TransformPoint(new Vector3(0, 1.5f, 0)),
                    Island.transform.rotation * Quaternion.Euler(4, yaw, 0));
                instancedMaterial.SetVector("_GrassPlayerPosition", camera.transform.position);
                bakedMaterial.SetVector("_GrassPlayerPosition", camera.transform.position);
            }

            internal void ConfigureShadowReceiver()
            {
                var receiver = GameObject.CreatePrimitive(PrimitiveType.Plane);
                receiver.name = "White URP reed shadow receiver";
                receiver.layer = 30;
                receiver.transform.SetParent(Island.transform, false);
                receiver.transform.localPosition = new Vector3(0, -.5f, 12);
                receiver.transform.localScale = new Vector3(2.4f, 1, 2.4f);
                receiverMaterial = new Material(Shader.Find("Universal Render Pipeline/Lit"));
                receiverMaterial.SetColor("_BaseColor", Color.white);
                receiverMaterial.SetFloat("_Metallic", 0);
                receiverMaterial.SetFloat("_Smoothness", 0);
                var renderer = receiver.GetComponent<MeshRenderer>();
                renderer.sharedMaterial = receiverMaterial;
                renderer.shadowCastingMode = ShadowCastingMode.Off;
                renderer.receiveShadows = true;
                camera.transform.SetPositionAndRotation(Island.transform.TransformPoint(new Vector3(0, 15, 12)),
                    Island.transform.rotation * Quaternion.Euler(90, 0, 0));
                camera.orthographic = true;
                camera.orthographicSize = 10;
                camera.farClipPlane = 60;
                camera.allowHDR = false;
                camera.GetUniversalAdditionalCameraData().renderShadows = true;
                sun.transform.rotation = Island.transform.rotation * Quaternion.Euler(45, -20, 0);
                sun.intensity = 1;
                sun.shadows = LightShadows.Hard;
                sun.shadowStrength = 1;
                RenderSettings.ambientLight = Color.white * .08f;
                instancedMaterial.SetVector("_GrassPlayerPosition", camera.transform.position);
                bakedMaterial.SetVector("_GrassPlayerPosition", camera.transform.position);
            }

            internal Color32[] Render(bool instancing, bool reedsVisible = true)
            {
                Streamer.SetVisible(instancing && reedsVisible);
                Control.enabled = !instancing && reedsVisible;
                camera.Render();
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, image.width, image.height), 0, 0);
                image.Apply();
                return image.GetPixels32();
            }

            public void Dispose()
            {
                Streamer.Dispose();
                Shader.SetGlobalVector("_MotuWeatherWind", oldWind);
                Shader.SetGlobalFloat("_MotuCloudEnabled", oldCloudEnabled);
                Shader.SetGlobalFloat("_MotuCloudNightStrength", oldNightStrength);
                RenderSettings.sun = oldSun;
                RenderSettings.ambientLight = oldAmbient;
                RenderSettings.ambientMode = oldAmbientMode;
                RenderSettings.fog = oldFog;
                RenderTexture.active = oldTarget;
                camera.targetTexture = null;
                target.Release();
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(camera.gameObject);
                Object.DestroyImmediate(sun.gameObject);
                Object.DestroyImmediate(Island);
                Object.DestroyImmediate(bakedMesh);
                Object.DestroyImmediate(instancedMaterial);
                Object.DestroyImmediate(bakedMaterial);
                Object.DestroyImmediate(receiverMaterial);
            }
        }

        // Own the two renderer definitions: package consumers may use any
        // renderer index for their simplified reflection camera.
        private sealed class ReedPipeline : IDisposable
        {
            private readonly RenderPipelineAsset previousDefault = GraphicsSettings.defaultRenderPipeline;
            private readonly RenderPipelineAsset previousQuality = QualitySettings.renderPipeline;
            private readonly UniversalRenderPipelineAsset pipeline;
            private readonly ScriptableRendererData forward, reflection;
            private readonly MotuRendererFeature reflectionFeature;

            internal ReedPipeline(bool ensureShadows = false)
            {
                var source = GraphicsSettings.currentRenderPipeline as UniversalRenderPipelineAsset;
                Assert.IsNotNull(source, "Reed rendering requires the package's URP consumer.");
                pipeline = Object.Instantiate(source);
                var settings = new SerializedObject(pipeline);
                var renderers = settings.FindProperty("m_RendererDataList");
                var defaultIndex = settings.FindProperty("m_DefaultRendererIndex").intValue;
                var template = renderers.GetArrayElementAtIndex(defaultIndex).objectReferenceValue;
                Assert.IsInstanceOf<UniversalRendererData>(template);
                forward = Object.Instantiate((ScriptableRendererData)template);
                reflection = Object.Instantiate((ScriptableRendererData)template);
                forward.rendererFeatures.Clear();
                reflection.rendererFeatures.Clear();
                reflectionFeature = ScriptableObject.CreateInstance<MotuRendererFeature>();
                var featureSettings = new SerializedObject(reflectionFeature);
                featureSettings.FindProperty("simplifiedReflections").boolValue = true;
                featureSettings.ApplyModifiedPropertiesWithoutUndo();
                reflection.rendererFeatures.Add(reflectionFeature);
                renderers.arraySize = 2;
                renderers.GetArrayElementAtIndex(0).objectReferenceValue = forward;
                renderers.GetArrayElementAtIndex(1).objectReferenceValue = reflection;
                settings.FindProperty("m_DefaultRendererIndex").intValue = 0;
                if (ensureShadows)
                {
                    settings.FindProperty("m_MainLightRenderingMode").intValue = (int)LightRenderingMode.PerPixel;
                    settings.FindProperty("m_MainLightShadowsSupported").boolValue = true;
                    settings.FindProperty("m_AnyShadowsSupported").boolValue = true;
                    settings.FindProperty("m_MainLightShadowmapResolution").intValue = 2048;
                    settings.FindProperty("m_ShadowDistance").floatValue = 60;
                    settings.FindProperty("m_ShadowCascadeCount").intValue = 1;
                    settings.FindProperty("m_ShadowDepthBias").floatValue = .1f;
                    settings.FindProperty("m_ShadowNormalBias").floatValue = .05f;
                }
                settings.ApplyModifiedPropertiesWithoutUndo();
                GraphicsSettings.defaultRenderPipeline = pipeline;
                QualitySettings.renderPipeline = pipeline;
            }

            public void Dispose()
            {
                GraphicsSettings.defaultRenderPipeline = previousDefault;
                QualitySettings.renderPipeline = previousQuality;
                Object.DestroyImmediate(pipeline);
                Object.DestroyImmediate(forward);
                Object.DestroyImmediate(reflection);
                Object.DestroyImmediate(reflectionFeature);
            }
        }

        private static bool IsGreen(Color32 pixel) => pixel.g > pixel.r * 1.2f
            && pixel.g > pixel.b * 1.2f && pixel.g > 10;

        private static Material NewMaterial()
        {
            var material = new Material(Shader.Find("Motu/Riverbank Reeds"));
            material.SetFloat("_WorldSize", 2000);
            material.SetFloat("_ReedFadeStart", 90);
            material.SetFloat("_ReedFadeEnd", 100);
            material.SetColor("_BaseColor", new Color(.08f, .7f, .03f));
            material.SetColor("_TipColor", new Color(.2f, .9f, .04f));
            return material;
        }

        private static IslandPreparedReedPlacement[] Placements()
        {
            var placements = new IslandPreparedReedPlacement[8];
            for (var i = 0; i < placements.Length; i++)
                placements[i] = new IslandPreparedReedPlacement(new Vector3((i % 4 - 1.5f) * 2.3f, 0, 10 + i / 4 * 4),
                    new Vector3((i % 3 - 1) * .45f, 1, (i % 2 == 0 ? 1 : -1) * .3f).normalized,
                    1.7f + i * .13f, .9f + i * .04f, new Vector4(i % 2, i / 7f, 1 - i / 9f, i * .117f));
            return placements;
        }

        // Independent control follows the authored native append_clump geometry,
        // baking each ground plane and material value into the legacy tile mesh.
        private static Mesh BakeNativeCards(IslandPreparedReedPlacement[] placements)
        {
            var positions = new List<Vector3>();
            var normals = new List<Vector3>();
            var uv = new List<Vector2>();
            var roots = new List<Vector2>();
            var colours = new List<Color>();
            var triangles = new List<int>();
            foreach (var placement in placements)
                for (var plane = 0; plane < 4; plane++)
                {
                    var angle = plane * Mathf.PI / 4;
                    var tangent = new Vector3(Mathf.Cos(angle), 0, Mathf.Sin(angle));
                    var first = positions.Count;
                    for (var level = 0; level < 4; level++)
                    {
                        var v = level / 3f;
                        for (var side = -1; side <= 1; side += 2)
                        {
                            var horizontal = tangent * (side * placement.widthMetres * .5f);
                            var ground = -(placement.groundNormal.x * horizontal.x + placement.groundNormal.z * horizontal.z)
                                / placement.groundNormal.y;
                            positions.Add(placement.localRoot + horizontal + Vector3.up * (v * placement.heightMetres + ground * (1 - v)));
                            normals.Add(new Vector3(-tangent.z, 0, tangent.x));
                            uv.Add(new Vector2((side + 1) * .5f, v));
                            roots.Add(new Vector2(placement.localRoot.x / 2000 + .5f, placement.localRoot.z / 2000 + .5f));
                            colours.Add(placement.data);
                        }
                    }
                    for (var level = 0; level < 3; level++)
                    {
                        var row = first + level * 2;
                        triangles.AddRange(new[] { row, row + 2, row + 1, row + 1, row + 2, row + 3 });
                    }
                }
            var mesh = new Mesh { name = "Baked native sloped reed control" };
            mesh.SetVertices(positions);
            mesh.SetNormals(normals);
            mesh.SetUVs(0, uv);
            mesh.SetUVs(1, roots);
            mesh.SetColors(colours);
            mesh.SetTriangles(triangles, 0);
            mesh.RecalculateBounds();
            return mesh;
        }
    }
}
