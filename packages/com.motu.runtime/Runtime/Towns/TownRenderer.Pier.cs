using System;
using System.Collections.Generic;
using UnityEngine;

namespace Motu.Towns
{
    /// <summary>Reserved cargo placement on the pier, in world space.</summary>
    public readonly struct TownPierPropPoint
    {
        public Vector3 Position { get; }
        public Vector3 Forward { get; }
        public string Kind { get; }

        internal TownPierPropPoint(Vector3 position, Vector3 forward, string kind)
        {
            Position = position;
            Forward = forward;
            Kind = kind;
        }
    }

    public sealed partial class TownRenderer
    {
        private Mesh pierNavigation;
        private TownPierPropPointData[] pierPropPoints = Array.Empty<TownPierPropPointData>();
        private Vector3 pierOrigin;

        public TownPierBerth CapitalPierBerth { get; private set; }
        public string CapitalPierDiagnostic { get; private set; }
        public int CapitalPierPropPointCount => pierPropPoints.Length;
        public IReadOnlyList<TownPierPropPoint> CapitalPierPropPoints
        {
            get
            {
                if (pierPropPoints.Length == 0) return Array.Empty<TownPierPropPoint>();
                var points = new TownPierPropPoint[pierPropPoints.Length];
                for (var i = 0; i < points.Length; i++) TryGetCapitalPierPropPoint(i, out points[i]);
                return points;
            }
        }

        public bool TryGetCapitalPierPropPoint(int index, out TownPierPropPoint propPoint)
        {
            propPoint = default;
            if ((uint)index >= (uint)pierPropPoints.Length || CapitalPierBerth == null) return false;
            var point = pierPropPoints[index];
            var pier = CapitalPierBerth.transform;
            propPoint = new TownPierPropPoint(pier.TransformPoint(Vector(point.position) - pierOrigin),
                pier.TransformDirection(HorizontalDirection(point.heading)), point.kind);
            return true;
        }

        internal static Vector3 HorizontalDirection(float[] direction) =>
            new Vector3(direction[0], 0f, direction[1]).normalized;

        private void BuildCapitalPier(TownCapitalPier pier, string diagnostic, Material churchStone)
        {
            CapitalPierDiagnostic = diagnostic;
            if (pier == null) return;

            var root = new GameObject("Capital Pier");
            root.transform.SetParent(transform, false);
            pierOrigin = (Vector(pier.start) + Vector(pier.end)) * .5f;
            root.transform.localPosition = pierOrigin - new Vector3(NativeWorldSizeMetres * .5f, 0f, NativeWorldSizeMetres * .5f);

            var stone = churchStone ?? CreateMaterial(TownNative.ChurchStone, "pier stone");
            var timber = CreatePierTimber();
            var metal = CreateMaterial(null, "pier iron");
            metal.SetColor("_BaseColor", new Color(.22f, .19f, .16f));
            metal.SetFloat("_Metallic", .63f);
            metal.SetFloat("_Roughness", .68f);

            Renderer Surface(string label, TownMesh geometry, Material material, float uvScale = 1f)
            {
                var mesh = CreateMesh(label, pierOrigin, uvScale, geometry);
                return AddMesh(root.transform, mesh, material, label).GetComponent<MeshRenderer>();
            }

            var lod = root.AddComponent<LODGroup>();
            lod.SetLODs(new[] {
                new LOD(.15f, new[] {
                    Surface("PierTimber", pier.timber, timber), Surface("PierStone", pier.stone, stone, .5f),
                    Surface("PierMetal", pier.metal, metal),
                }),
                new LOD(0f, new[] {
                    Surface("PierTimberCoarse", pier.coarse_timber, timber), Surface("PierStoneCoarse", pier.coarse_stone, stone, .5f),
                }),
            });
            lod.RecalculateBounds();
            root.AddComponent<MeshCollider>().sharedMesh = CreateMesh(pier.collision, "Capital pier collision", pierOrigin);
            pierNavigation = CreateMesh(pier.navigation, "Capital pier navigation", pierOrigin);
            CapitalPierBerth = root.AddComponent<TownPierBerth>();
            CapitalPierBerth.Configure(pier, pierOrigin);
            pierPropPoints = pier.prop_points ?? Array.Empty<TownPierPropPointData>();
            InstallPierClutter(root.transform, pier.id);
        }

        private Material CreatePierTimber()
        {
            const int width = 128, height = 128;
            var albedo = new Color32[width * height];
            var grain = new float[albedo.Length];
            var normals = new Color32[albedo.Length];
            var low = new Color(.34f, .27f, .20f);
            var high = new Color(.65f, .55f, .43f);
            for (var y = 0; y < height; y++)
                for (var x = 0; x < width; x++)
                {
                    var u = (x + .5f) / width;
                    var v = (y + .5f) / height;
                    var broad = RepeatNoise(u, v, 3f, 5f);
                    var fibre = RepeatNoise(u, Mathf.Repeat(v + .04f * broad, 1f), 5f, 45f);
                    var rings = Mathf.Sin((v * 17f + broad * 1.2f) * (2f * Mathf.PI));
                    var index = y * width + x;
                    grain[index] = .7f * fibre + .12f * rings;
                    albedo[index] = Color.Lerp(low, high, .2f + .47f * grain[index] + .43f * broad);
                }
            for (var y = 0; y < height; y++)
                for (var x = 0; x < width; x++)
                {
                    var along = grain[y * width + (x + width - 1) % width] - grain[y * width + (x + 1) % width];
                    var across = grain[((y + height - 1) % height) * width + x] - grain[((y + 1) % height) * width + x];
                    var normal = new Vector3(along * .25f, across * .65f, 1f).normalized;
                    normals[y * width + x] = new Color(normal.x * .5f + .5f, normal.y * .5f + .5f, normal.z * .5f + .5f);
                }

            Texture2D Texture(string label, Color32[] pixels, bool linear)
            {
                var texture = Own(new Texture2D(width, height, TextureFormat.RGBA32, true, linear)
                    { name = label, wrapMode = TextureWrapMode.Repeat, anisoLevel = 4 });
                texture.SetPixels32(pixels);
                texture.Apply(true, true);
                return texture;
            }
            var timber = CreateMaterial(null, "pier timber");
            timber.SetTexture("_BaseMap", Texture("Weathered pier oak", albedo, false));
            timber.SetTexture("_NormalMap", Texture("Pier oak grain", normals, true));
            // Native UVs are metres, with U along the grain of each board or beam.
            timber.SetTextureScale("_BaseMap", new Vector2(1f / 4.8f, 1f / .82f));
            timber.SetFloat("_Roughness", .86f);
            timber.SetVector("_Grub0", new Vector4(.24f, .12f, 1.6f, 2.9f));
            SetGrubDirtColour(timber, new Vector4(.082f, .059f, .037f, 1f));
            timber.SetVector("_Grub2", new Vector4(.095f, .107f, .061f, 1f));
            return timber;
        }

        private static float RepeatNoise(float u, float v, float scaleX, float scaleY)
        {
            var x = 23.7f + u * scaleX;
            var y = 19.2f + v * scaleY;
            var a = Mathf.Lerp(Mathf.PerlinNoise(x, y), Mathf.PerlinNoise(x - scaleX, y), u);
            var b = Mathf.Lerp(Mathf.PerlinNoise(x, y - scaleY), Mathf.PerlinNoise(x - scaleX, y - scaleY), u);
            return Mathf.Lerp(a, b, v);
        }

        private void InstallPierClutter(Transform pier, string pierId)
        {
            if (pierPropPoints.Length == 0) return;
            var catalogue = Resources.Load<CapitalPierClutterCatalogue>(CapitalPierClutterCatalogue.ResourcePath);
            if (catalogue == null) return;

            var optionsByKind = new Dictionary<string, IReadOnlyList<GameObject>>(StringComparer.Ordinal);
            Transform clutter = null;
            for (var index = 0; index < pierPropPoints.Length; index++)
            {
                var point = pierPropPoints[index];
                if (string.IsNullOrEmpty(point.kind)) continue;
                if (!optionsByKind.TryGetValue(point.kind, out var prefabs))
                {
                    prefabs = catalogue.MatchingPrefabs(point.kind);
                    optionsByKind.Add(point.kind, prefabs);
                }
                if (prefabs.Count == 0) continue;
                if (clutter == null)
                {
                    clutter = new GameObject("Pier cargo").transform;
                    clutter.SetParent(pier, false);
                    clutter.gameObject.SetActive(false);
                }
                var stableHash = PierClutterIndex(pierId, index);
                var cargo = Instantiate(prefabs[(int)(stableHash % (uint)prefabs.Count)], clutter, false);
                cargo.transform.SetLocalPositionAndRotation(Vector(point.position) - pierOrigin,
                    Quaternion.LookRotation(HorizontalDirection(point.heading)));
                foreach (var collider in cargo.GetComponentsInChildren<Collider>(true)) collider.enabled = false;
            }
            if (clutter != null) clutter.gameObject.SetActive(true);
        }

        private static uint PierClutterIndex(string pierId, int index)
        {
            unchecked
            {
                var hash = 2166136261u;
                foreach (var character in pierId) hash = (hash ^ character) * 16777619u;
                return (hash ^ (uint)index) * 16777619u;
            }
        }
    }
}
