using Motu.Islands;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class IslandLod3Tests
    {
        internal static void ValidateInstalledLod3(IslandRuntime runtime, WorldEnvironmentController environment)
        {
            Debug.Log($"Island LOD3: {runtime.Lod3TriangleCount} triangles; tiled LOD2: {runtime.TerrainStreamer.BaseTriangleCount} triangles.");
            Assert.That(runtime.Lod3TriangleCount, Is.GreaterThan(0));
            Assert.That(runtime.Lod3TriangleCount, Is.LessThan(runtime.TerrainStreamer.BaseTriangleCount));
            var centre = runtime.transform.position;
            runtime.SetViewPosition(centre + Vector3.right * 2000);
            Assert.IsFalse(runtime.IsLod3Visible, "Exactly 2 km retains the detailed island.");
            Assert.IsTrue(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            runtime.SetViewPosition(centre + Vector3.right * 2000.1f);
            Assert.IsTrue(runtime.IsLod3Visible);
            Assert.IsFalse(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            Assert.IsFalse(runtime.Caves.gameObject.activeInHierarchy);
            var renderers = runtime.GetComponentsInChildren<MeshRenderer>();
            Assert.That(renderers.Length, Is.EqualTo(1), "LOD3 must replace every detailed renderer with one unsliced draw.");
            Assert.That(renderers[0].sharedMaterial.shader.name, Is.EqualTo("Motu/Island Horizon"));
            Assert.IsFalse(renderers[0].receiveShadows);
            Assert.IsNull(renderers[0].GetComponent<Collider>());
            var mesh = renderers[0].GetComponent<MeshFilter>().sharedMesh;
            Assert.That(mesh.subMeshCount, Is.EqualTo(1));
            Assert.IsFalse(mesh.HasVertexAttribute(UnityEngine.Rendering.VertexAttribute.Normal));
            runtime.SetDormant(true);
            Assert.IsTrue(runtime.gameObject.activeInHierarchy);
            Assert.IsTrue(runtime.IsLod3Visible, "Dormant resident islands must retain their silhouettes.");
            runtime.SetDormant(false);
            Assert.IsTrue(runtime.IsLod3Visible, "Waking a distant island must not briefly reveal LOD2.");
            runtime.SetViewPosition(centre + Vector3.right * 1999);
            Assert.IsFalse(runtime.IsLod3Visible);
            Assert.IsTrue(runtime.TerrainStreamer.gameObject.activeInHierarchy);
            Assert.IsTrue(runtime.Caves.gameObject.activeInHierarchy);
            runtime.SetViewPosition(centre + Vector3.up * 2001);
            Assert.IsTrue(runtime.IsLod3Visible, "Distance is measured to the island centre in world space.");
            runtime.transform.position += new Vector3(8000, 0, -7000);
            runtime.SetViewPosition(runtime.transform.position + Vector3.forward * 100);
            Assert.IsFalse(runtime.IsLod3Visible, "Translated islands must measure from their own centre.");
            runtime.transform.position = centre;
            runtime.SetViewPosition(centre);
            Assert.That(environment.GetComponent<OceanSurfaceController>().CoastalWaveBindingCount, Is.EqualTo(1));
        }

        [Test]
        public void HorizonShaderRendersOnlyTheCurrentFlatColour()
        {
            var shader = Resources.Load<Shader>("IslandHorizon");
            Assert.IsNotNull(shader);
            var material = new Material(shader);
            var target = new RenderTexture(32, 32, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var image = new Texture2D(32, 32, TextureFormat.RGBAFloat, false, true);
            var previousTarget = RenderTexture.active;
            var oldColour = Shader.GetGlobalColor("_MotuIslandHorizonColour");
            var oldFog = RenderSettings.fog;
            var oldAmbient = RenderSettings.ambientLight;
            try
            {
                foreach (var colour in new[] { new Color(.12f, .35f, .6f, 1), new Color(.015f, .02f, .04f, 1) })
                {
                    Shader.SetGlobalColor("_MotuIslandHorizonColour", colour);
                    RenderSettings.fog = true;
                    RenderSettings.ambientLight = Color.red;
                    Graphics.Blit(Texture2D.whiteTexture, target, material);
                    RenderTexture.active = target;
                    image.ReadPixels(new Rect(0, 0, 32, 32), 0, 0); image.Apply();
                    foreach (var pixel in image.GetPixels())
                    {
                        Assert.That(pixel.r, Is.EqualTo(colour.r).Within(.001));
                        Assert.That(pixel.g, Is.EqualTo(colour.g).Within(.001));
                        Assert.That(pixel.b, Is.EqualTo(colour.b).Within(.001));
                        Assert.That(pixel.a, Is.EqualTo(1));
                    }
                }
                Assert.IsFalse(ShaderUtil.ShaderHasError(shader));
            }
            finally
            {
                RenderTexture.active = previousTarget;
                Shader.SetGlobalColor("_MotuIslandHorizonColour", oldColour);
                RenderSettings.fog = oldFog;
                RenderSettings.ambientLight = oldAmbient;
                Object.DestroyImmediate(image);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(material);
            }
        }
    }
}
