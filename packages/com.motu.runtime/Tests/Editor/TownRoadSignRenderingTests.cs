using System.Linq;
using Motu.Islands;
using Motu.Towns;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.TextCore.Text;

namespace Motu.Tests
{
    public sealed class TownRoadSignRenderingTests
    {
        [Test]
        public void InstallsStreamedSignsWithSharedResourcesAndCleansUpOnUnload()
        {
            var host = new GameObject("Cart-track sign host");
            IslandRuntime runtime = null;
            try
            {
                var data = new TownPreparedData
                {
                    cartTracks = new TownCartTrackPlan
                    {
                        run_count = 4,
                        departures = new[]
                        {
                            Departure("Westeridge", 1020f, 1f),
                            Departure("Westeridge", 1040f, -1f),
                            Departure("\U0010FFFF", 1060f, 1f),
                            Departure("Whittingbourne Westercombe", 1080f, 1f),
                        },
                    },
                };
                var initialAtlases = PirataAtlases();
                for (var installation = 0; installation < 2; installation++)
                {
                    runtime = IslandRuntime.Create(default, host.transform);
                    var town = TownRenderer.Install(runtime.transform, data, 1000f);
                    runtime.SetTownRenderer(town);
                    Assert.That(town.CartTrackSignCount, Is.EqualTo(4));
                    var group = town.transform.Find("Cart-track signs");
                    var first = group.GetChild(0);
                    var second = group.GetChild(1);
                    var third = group.GetChild(2);
                    Assert.That(first.position, Is.EqualTo(new Vector3(10f, 3f, 5f)));
                    Assert.That(second.position, Is.EqualTo(new Vector3(20f, 3f, 5f)));

                    var firstPaint = first.Find("Painted destination").GetComponent<MeshRenderer>();
                    var secondPaint = second.Find("Painted destination").GetComponent<MeshRenderer>();
                    var thirdPaint = third.Find("Painted destination").GetComponent<MeshRenderer>();
                    var timberMesh = first.Find("Timber").GetComponent<MeshFilter>().sharedMesh;
                    var timberMaterial = first.Find("Timber").GetComponent<MeshRenderer>().sharedMaterial;
                    var textMesh = firstPaint.GetComponent<MeshFilter>().sharedMesh;
                    var textMaterial = firstPaint.sharedMaterial;
                    var textTexture = LabelTexture(firstPaint);
                    var replacementTexture = LabelTexture(thirdPaint);
                    var longTexture = LabelTexture(group.GetChild(3).Find("Painted destination").GetComponent<MeshRenderer>());
                    var point = timberMesh.vertices.OrderByDescending(vertex => vertex.x).First();
                    Assert.That(point.x, Is.GreaterThan(textMesh.bounds.max.x));
                    Assert.That(first.TransformPoint(point).x, Is.GreaterThan(first.position.x));
                    Assert.That(second.TransformPoint(point).x, Is.LessThan(second.position.x));
                    Assert.That(Vector3.Dot(first.right, Vector3.right), Is.GreaterThan(.999f));
                    Assert.That(Vector3.Dot(second.right, Vector3.left), Is.GreaterThan(.999f));
                    Assert.That(textMesh.normals[0], Is.EqualTo(Vector3.back));
                    Assert.That(textMesh.normals[4], Is.EqualTo(Vector3.forward));
                    Assert.That(textMesh.uv[0], Is.EqualTo(Vector2.zero));
                    Assert.That(textMesh.uv[4], Is.EqualTo(Vector2.right));
                    Assert.That(second.Find("Timber").GetComponent<MeshFilter>().sharedMesh, Is.SameAs(timberMesh));
                    Assert.That(secondPaint.sharedMaterial, Is.SameAs(textMaterial));
                    Assert.That(LabelTexture(secondPaint), Is.SameAs(textTexture));
                    Assert.That(textTexture.mipmapCount, Is.GreaterThan(1));
                    Assert.That(replacementTexture, Is.Not.Null.And.Not.SameAs(textTexture));
                    Assert.That(longTexture, Is.Not.Null.And.Not.SameAs(textTexture));
                    if (installation == 0 && SystemInfo.graphicsDeviceType != GraphicsDeviceType.Null)
                    {
                        var shortInk = InkBounds(textTexture);
                        var longInk = InkBounds(longTexture);
                        Assert.That(longInk.height, Is.GreaterThan(0).And.LessThan(shortInk.height));
                        Assert.That(longInk.xMin, Is.GreaterThanOrEqualTo(16));
                        Assert.That(longInk.xMax, Is.LessThanOrEqualTo(longTexture.width - 16));
                    }
                    Assert.That(PirataAtlases(), Is.EqualTo(initialAtlases));

                    runtime.Dispose();
                    Assert.That(timberMesh == null && timberMaterial == null && textMesh == null && textMaterial == null,
                        Is.True);
                    Assert.That(textTexture == null && replacementTexture == null && longTexture == null, Is.True);
                }
            }
            finally
            {
                if (runtime != null) runtime.Dispose();
                Object.DestroyImmediate(host);
            }
        }

        private static TownCartTrackDeparture Departure(string destination, float x, float heading) =>
            new TownCartTrackDeparture
            {
                destination_name = destination,
                position = new[] { x, 1010f, 6f },
                direction = new[] { heading, 0f },
            };

        private static Texture2D LabelTexture(MeshRenderer renderer)
        {
            var properties = new MaterialPropertyBlock();
            renderer.GetPropertyBlock(properties);
            return properties.GetTexture("_BaseMap") as Texture2D;
        }

        private static RectInt InkBounds(Texture2D texture)
        {
            var previous = RenderTexture.active;
            var target = RenderTexture.GetTemporary(texture.width, texture.height, 0, RenderTextureFormat.ARGB32);
            var image = new Texture2D(target.width, target.height, TextureFormat.RGBA32, false);
            try
            {
                Graphics.Blit(texture, target);
                RenderTexture.active = target;
                image.ReadPixels(new Rect(0, 0, target.width, target.height), 0, 0);
                var pixels = image.GetPixels32();
                var left = target.width;
                var bottom = target.height;
                var right = 0;
                var top = 0;
                for (var y = 0; y < target.height; y++)
                    for (var x = 0; x < target.width; x++)
                    {
                        if (pixels[y * target.width + x].a <= 16) continue;
                        left = Mathf.Min(left, x);
                        bottom = Mathf.Min(bottom, y);
                        right = Mathf.Max(right, x + 1);
                        top = Mathf.Max(top, y + 1);
                    }
                return new RectInt(left, bottom, right - left, top - bottom);
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(target);
                Object.DestroyImmediate(image);
            }
        }

        private static int PirataAtlases() => Resources.FindObjectsOfTypeAll<FontAsset>()
            .Count(font => font.name.StartsWith("Pirata One", System.StringComparison.Ordinal));
    }
}
