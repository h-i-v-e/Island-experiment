using System;
using System.Collections.Generic;
using Unity.Collections;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;
using UnityEngine.TextCore.Text;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Towns
{
    public sealed partial class TownRenderer
    {
        private const string CartTrackSignFontPath = "Towns/Fonts/PirataOne-Regular";
        private const float SignBoardFront = -.075f;
        private const float SignBoardBack = .033f;
        private const float SignBoardCentre = 1.66f;
        private static readonly int SignAlbedo = Shader.PropertyToID("_BaseMap");

        internal int CartTrackSignCount { get; private set; }

        private void BuildCartTrackSigns(TownCartTrackDeparture[] departures, Font font)
        {
            if (departures == null || departures.Length == 0) return;

            var labels = new Dictionary<string, Texture2D>(StringComparer.Ordinal);
            var characters = new HashSet<uint> { '?' };
            foreach (var departure in departures)
            {
                if (string.IsNullOrWhiteSpace(departure?.destination_name)) continue;
                if (!labels.TryAdd(departure.destination_name, null)) continue;
                var label = departure.destination_name;
                for (var i = 0; i < label.Length; i++) characters.Add(ReadSignCodePoint(label, ref i));
            }
            if (labels.Count == 0) return;

            var atlas = CreateSignFontAtlas(font);
            if (atlas == null)
            {
                var bundled = Resources.Load<Font>(CartTrackSignFontPath);
                if (bundled != font) atlas = CreateSignFontAtlas(bundled);
            }
            if (atlas == null)
            {
                Debug.LogWarning("Could not load the cart-track sign font.", this);
                return;
            }

            try
            {
                var unicodes = new uint[characters.Count];
                characters.CopyTo(unicodes);
                Array.Sort(unicodes);
                atlas.TryAddCharacters(unicodes);
                InstallCartTrackSigns(departures, labels, atlas);
            }
            finally { DestroyUnityObject(atlas); }
        }

        private static FontAsset CreateSignFontAtlas(Font font) => font == null ? null :
            FontAsset.CreateFontAsset(font, 104, 2, GlyphRenderMode.SMOOTH_HINTED, 1024, 1024);

        private void InstallCartTrackSigns(TownCartTrackDeparture[] departures, Dictionary<string, Texture2D> labels,
            FontAsset atlas)
        {
            var group = new GameObject("Cart-track signs").transform;
            group.SetParent(transform, false);
            var timberMesh = Own(CreateSignTimberMesh());
            var letteringMesh = Own(CreateSignLetteringMesh());
            var timber = CreatePierTimber();
            timber.name = "Town roadside oak";
            timber.SetColor("_BaseColor", new Color(.83f, .82f, .80f));
            var paint = CreateMaterial(null, "roadside sign paint");
            paint.SetColor("_BaseColor", new Color(.96f, .88f, .71f, .96f));
            paint.SetFloat("_SrcBlend", (float)BlendMode.SrcAlpha);
            paint.SetFloat("_DstBlend", (float)BlendMode.OneMinusSrcAlpha);
            paint.SetFloat("_ZWrite", 0);
            paint.renderQueue = (int)RenderQueue.Transparent;
            paint.SetShaderPassEnabled("ShadowCaster", false);
            var properties = new MaterialPropertyBlock();

            foreach (var departure in departures)
            {
                if (departure?.position?.Length != 3 || departure.direction?.Length != 2 ||
                    string.IsNullOrWhiteSpace(departure.destination_name)) continue;
                var heading = HorizontalDirection(departure.direction);
                if (heading == Vector3.zero) continue;

                var label = departure.destination_name;
                if (labels[label] == null) labels[label] = CreateSignLabelTexture(atlas, label);
                if (labels[label] == null) continue;
                var sign = new GameObject(label).transform;
                sign.SetParent(group, false);
                sign.SetLocalPositionAndRotation(Position(departure.position, NativeWorldSizeMetres),
                    Quaternion.LookRotation(Vector3.Cross(heading, Vector3.up)));
                AddMesh(sign, timberMesh, timber, "Timber");
                var lettering = AddMesh(sign, letteringMesh, paint, "Painted destination").GetComponent<MeshRenderer>();
                lettering.shadowCastingMode = ShadowCastingMode.Off;
                properties.SetTexture(SignAlbedo, labels[label]);
                lettering.SetPropertyBlock(properties);
                CartTrackSignCount++;
            }
        }

        private Texture2D CreateSignLabelTexture(FontAsset atlas, string label)
        {
            const int width = 512, height = 128;
            var glyphs = new List<SignGlyph>(label.Length);
            // The new metrics-only API does not provide the raster's atlas rectangle.
#pragma warning disable CS0618
            var characters = atlas.characterLookupTable;
#pragma warning restore CS0618
            var fallback = characters.GetValueOrDefault('?');
            var min = new Vector2(float.PositiveInfinity, float.PositiveInfinity);
            var max = new Vector2(float.NegativeInfinity, float.NegativeInfinity);
            var pen = 0f;
            var space = characters.GetValueOrDefault(' ')?.glyph?.metrics.horizontalAdvance ?? atlas.faceInfo.pointSize * .25f;
            for (var i = 0; i < label.Length; i++)
            {
                var codePoint = ReadSignCodePoint(label, ref i);
                if (codePoint == ' ')
                {
                    pen += space;
                    continue;
                }
                var glyph = (characters.GetValueOrDefault(codePoint) ?? fallback)?.glyph;
                if (glyph == null) continue;
                var metrics = glyph.metrics;
                var rect = glyph.glyphRect;
                if (rect.width > 0 && rect.height > 0)
                {
                    var bottomLeft = new Vector2(pen + metrics.horizontalBearingX, metrics.horizontalBearingY - rect.height);
                    min = Vector2.Min(min, bottomLeft);
                    max = Vector2.Max(max, bottomLeft + new Vector2(rect.width, rect.height));
                    glyphs.Add(new SignGlyph(glyph, bottomLeft));
                }
                pen += metrics.horizontalAdvance;
            }
            if (glyphs.Count == 0) return null;

            var bounds = max - min;
            var scale = Mathf.Min(1f, Mathf.Min((width - 32f) / bounds.x, (height - 36f) / bounds.y));
            var origin = (new Vector2(width, height) - bounds * scale) * .5f - min * scale;
            var coverage = new Color32[width * height];
            Array.Fill(coverage, new Color32(255, 255, 255, 0));
            foreach (var glyph in glyphs)
                PaintSignGlyph(glyph, atlas.atlasTextures[glyph.Glyph.atlasIndex], origin, scale, coverage, width, height);

            var texture = Own(new Texture2D(width, height, TextureFormat.RGBA32, true, true)
            {
                name = $"Cart-track sign: {label}",
                wrapMode = TextureWrapMode.Clamp,
                filterMode = FilterMode.Trilinear,
                anisoLevel = 4,
            });
            texture.SetPixels32(coverage);
            texture.Apply(true, true);
            return texture;
        }

        private static uint ReadSignCodePoint(string text, ref int index)
        {
            var character = text[index];
            if (!char.IsSurrogatePair(text, index)) return character;
            return (uint)char.ConvertToUtf32(character, text[++index]);
        }

        private static void PaintSignGlyph(SignGlyph glyph, Texture2D atlas, Vector2 origin, float scale,
            Color32[] coverage, int width, int height)
        {
            var rect = glyph.Glyph.glyphRect;
            var pixels = atlas.GetPixelData<byte>(0);
            var start = origin + glyph.BottomLeft * scale;
            var end = start + new Vector2(rect.width, rect.height) * scale;
            var left = Mathf.Max(0, Mathf.FloorToInt(start.x));
            var bottom = Mathf.Max(0, Mathf.FloorToInt(start.y));
            var right = Mathf.Min(width, Mathf.CeilToInt(end.x));
            var top = Mathf.Min(height, Mathf.CeilToInt(end.y));
            for (var y = bottom; y < top; y++)
                for (var x = left; x < right; x++)
                {
                    var sampleX = (x + .5f - start.x) / scale - .5f;
                    var sampleY = (y + .5f - start.y) / scale - .5f;
                    var value = SampleSignGlyph(pixels, atlas.width, rect, sampleX, sampleY);
                    var index = y * width + x;
                    coverage[index].a = Math.Max(coverage[index].a, (byte)Mathf.RoundToInt(value));
                }
        }

        private static float SampleSignGlyph(NativeArray<byte> pixels, int stride, GlyphRect rect, float x, float y)
        {
            var left = Mathf.FloorToInt(x);
            var bottom = Mathf.FloorToInt(y);
            var lower = Mathf.Lerp(Pixel(left, bottom), Pixel(left + 1, bottom), x - left);
            var upper = Mathf.Lerp(Pixel(left, bottom + 1), Pixel(left + 1, bottom + 1), x - left);
            return Mathf.Lerp(lower, upper, y - bottom);

            byte Pixel(int column, int row) => (uint)column < (uint)rect.width && (uint)row < (uint)rect.height
                ? pixels[(rect.y + row) * stride + rect.x + column] : (byte)0;
        }

        private readonly struct SignGlyph
        {
            internal readonly Glyph Glyph;
            internal readonly Vector2 BottomLeft;

            internal SignGlyph(Glyph glyph, Vector2 bottomLeft) { Glyph = glyph; BottomLeft = bottomLeft; }
        }

        private static Mesh CreateSignTimberMesh()
        {
            var vertices = new List<Vector3>();
            var uvs = new List<Vector2>();
            var triangles = new List<int>();

            // Recess the post below the terrain sample so the foot stays buried on a sloping verge.
            AddPrism(new[] { new Vector2(-.105f, -.23f), new Vector2(.105f, -.23f), new Vector2(.086f, 1.43f),
                new Vector2(.046f, 1.49f), new Vector2(-.064f, 1.485f), new Vector2(-.091f, 1.455f) },
                -.005f, .162f, true);
            AddPrism(new[] { new Vector2(-.82f, 1.45f), new Vector2(-.78f, 1.411f), new Vector2(.49f, 1.411f),
                new Vector2(.84f, SignBoardCentre), new Vector2(.49f, 1.908f),
                new Vector2(-.78f, 1.908f), new Vector2(-.82f, 1.868f) }, SignBoardFront, SignBoardBack, false);

            var mesh = new Mesh { name = "Hewn oak cart-track sign" };
            mesh.SetVertices(vertices);
            mesh.SetUVs(0, uvs);
            mesh.SetTriangles(triangles, 0);
            mesh.RecalculateNormals();
            return mesh;

            void AddPrism(Vector2[] profile, float front, float back, bool verticalGrain)
            {
                AddFace(front, true);
                AddFace(back, false);
                for (var i = 0; i < profile.Length; i++)
                {
                    var start = profile[i];
                    var end = profile[(i + 1) % profile.Length];
                    var index = vertices.Count;
                    var length = Vector2.Distance(start, end);
                    var depth = back - front;
                    vertices.Add(new Vector3(start.x, start.y, front));
                    vertices.Add(new Vector3(end.x, end.y, front));
                    vertices.Add(new Vector3(end.x, end.y, back));
                    vertices.Add(new Vector3(start.x, start.y, back));
                    uvs.AddRange(new[] { Vector2.zero, new Vector2(length, 0f),
                        new Vector2(length, depth), new Vector2(0f, depth) });
                    triangles.AddRange(new[] { index, index + 1, index + 2, index, index + 2, index + 3 });
                }

                void AddFace(float depth, bool reverse)
                {
                    var first = vertices.Count;
                    foreach (var point in profile)
                    {
                        vertices.Add(new Vector3(point.x, point.y, depth));
                        uvs.Add(verticalGrain ? new Vector2(point.y, point.x) : point);
                    }
                    for (var i = 1; i < profile.Length - 1; i++)
                    {
                        triangles.Add(first);
                        triangles.Add(first + i + (reverse ? 1 : 0));
                        triangles.Add(first + i + (reverse ? 0 : 1));
                    }
                }
            }
        }

        private static Mesh CreateSignLetteringMesh()
        {
            const float left = -.76f, right = .46f, halfHeight = (right - left) / 8f;
            const float bottom = SignBoardCentre - halfHeight, top = SignBoardCentre + halfHeight;
            const float front = SignBoardFront - .004f, back = SignBoardBack + .004f;
            return new Mesh
            {
                name = "Cart-track sign lettering",
                vertices = new[] {
                    new Vector3(left, bottom, front), new Vector3(left, top, front),
                    new Vector3(right, top, front), new Vector3(right, bottom, front),
                    new Vector3(left, bottom, back), new Vector3(left, top, back),
                    new Vector3(right, top, back), new Vector3(right, bottom, back),
                },
                uv = new[] { Vector2.zero, Vector2.up, Vector2.one, Vector2.right,
                    Vector2.right, Vector2.one, Vector2.up, Vector2.zero },
                normals = new[] { Vector3.back, Vector3.back, Vector3.back, Vector3.back,
                    Vector3.forward, Vector3.forward, Vector3.forward, Vector3.forward },
                triangles = new[] { 0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6 },
            };
        }
    }
}
