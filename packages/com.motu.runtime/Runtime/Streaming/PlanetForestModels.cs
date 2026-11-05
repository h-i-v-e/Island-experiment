using System;
using System.Collections.Generic;
using Motu.Interop;
using Motu.Islands;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // One leased GPU catalog across the archipelago, plus five materials per noise texture.
    internal sealed class PlanetForestModels
    {
        private sealed class Palette
        {
            internal int references;
            internal Material[] materials;
            internal PlanetForestBark bark;
        }
        private static readonly Dictionary<ValueTuple<Texture>, Palette> Palettes = new Dictionary<ValueTuple<Texture>, Palette>();
        private static readonly Dictionary<(IslandPreparedMesh[][], Texture), PlanetForestModels> Catalogs
            = new Dictionary<(IslandPreparedMesh[][], Texture), PlanetForestModels>();
        private readonly (IslandPreparedMesh[][], Texture) key;
        private readonly Palette palette;
        private int references;
        internal readonly Mesh[][] meshes;
        internal readonly Bounds[] bounds;
        internal readonly int broadleafCount;
        internal readonly int vertexCount;
        internal bool IsPine(int prototype) => prototype >= broadleafCount && prototype < broadleafCount * 2;
        internal bool IsPalm(int prototype) => prototype >= broadleafCount * 2;
        internal Material Material(int kind) => palette.materials[
            kind == 0 || kind == 1 ? 0 :
            kind == 2 || kind == 6 ? 1 :
            kind == 3 ? 2 : kind == 4 ? 3 : 4];

        internal static PlanetForestModels Acquire(HybridForestData data, Texture noise)
        {
            var key = (data.prototypes, noise);
            if (!Catalogs.TryGetValue(key, out var models))
            {
                models = new PlanetForestModels(data, noise, key);
                Catalogs.Add(key, models);
            }
            ++models.references;
            return models;
        }
        private PlanetForestModels(HybridForestData data, Texture noise, (IslandPreparedMesh[][], Texture) key)
        {
            this.key = key;
            broadleafCount = data.broadleafCount;
            if (!Palettes.TryGetValue(ValueTuple.Create(key.Item2), out palette))
            {
                var shader = MotuShaders.Find("Motu/Planet Forest")
                    ?? throw new InvalidOperationException("Missing Planet forest shader.");
                var twigCard = Resources.Load<Texture2D>("Motu/TreeBranchLeafy");
                if (twigCard == null)
                    throw new InvalidOperationException("Missing shared leafy branch texture (Motu/TreeBranchLeafy).");
                var palmFrond = Resources.Load<Texture2D>("Motu/PalmFrond");
                if (palmFrond == null)
                    throw new InvalidOperationException("Missing shared palm frond texture (Motu/PalmFrond).");
                palette = new Palette { materials = new Material[5], bark = new PlanetForestBark() };
                for (var kind = 0; kind < 5; ++kind)
                {
                    var material = new Material(shader) { name = $"Shared Planet forest {kind}", enableInstancing = true };
                    material.SetFloat("_ForestKind", kind);
                    material.SetTexture("_PalmFrond", palmFrond);
                    material.SetFloat("_Cull", kind == 0 || kind == 3 ? (float)CullMode.Back : (float)CullMode.Off);
                    palette.bark.Bind(material);
                    if (noise != null) material.SetTexture("_ForestNoise", noise);
                    if (kind == 4)
                    {
                        material.SetTexture("_TwigCard", twigCard);
                        // Match the leafy image's stem to the fan's bottom-centre root.
                        const float rootU = 619.5f / 1254f, rootV = 1.5f / 1254f;
                        material.SetTextureOffset("_TwigCard", new Vector2(rootU - .5f, rootV));
                        material.SetTextureScale("_TwigCard", new Vector2(1, 1 - rootV));
                    }
                    palette.materials[kind] = material;
                }
                Palettes.Add(ValueTuple.Create(key.Item2), palette);
            }
            ++palette.references;
            meshes = new Mesh[data.prototypes.Length][];
            bounds = new Bounds[meshes.Length];
            try
            {
                for (var p = 0; p < meshes.Length; ++p)
                {
                    var source = data.prototypes[p];
                    var species = IsPalm(p) ? 2 : IsPine(p) ? 0 : 1;
                    var fineWood = Make(source[0], species);
                    var branches = Make(source[2], species, 1);
                    var leaves = Make(source[1], species);
                    var middle = species != 1 ? Make(source[3], species, 1) : null;
                    var shells = species != 2 && source.Length > 6 ? Make(source[6], species) : null;
                    var twigs = species != 2 && source.Length > 7 ? Make(source[7], species) : null;
                    var farWood = Make(source[4], species, 2);
                    var farCrown = Make(source[5], species, 2);
                    Mesh far;
                    try
                    {
                        if (farWood != null)
                        {
                            var colours = farWood.colors;
                            for (var i = 0; i < colours.Length; ++i) colours[i].g = 0;
                            farWood.colors = colours;
                        }
                        var parts = new List<CombineInstance>(2);
                        if (farWood != null) parts.Add(new CombineInstance { mesh = farWood });
                        if (farCrown != null) parts.Add(new CombineInstance { mesh = farCrown });
                        far = new Mesh { name = "Planet combined distant tree", indexFormat = IndexFormat.UInt32 };
                        far.CombineMeshes(parts.ToArray(), true, false);
                    }
                    finally { DestroyUnityObject(farWood); DestroyUnityObject(farCrown); }
                    // Keep existing mesh slots stable; middle pine/palm foliage is slot six.
                    meshes[p] = new[] { fineWood, branches, leaves, shells, far, twigs, middle };
                    var first = true;
                    foreach (var mesh in meshes[p])
                    {
                        if (mesh == null) continue;
                        if (first) { bounds[p] = mesh.bounds; first = false; } else bounds[p].Encapsulate(mesh.bounds);
                        vertexCount += mesh.vertexCount;
                        mesh.UploadMeshData(true);
                    }
                }
            }
            catch { Dispose(); throw; }
        }
        private static Mesh Make(IslandPreparedMesh source, int species, int stage = 0)
        {
            if (source == null) return null;
            var mesh = IslandMeshInterop.CreateRiverMesh(source);
            if (source.material.Length == 0 || species == 2)
            {
                var colours = source.material.Length == 0 ? new Color[mesh.vertexCount] : mesh.colors;
                for (var i = 0; i < colours.Length; ++i)
                {
                    if (source.material.Length == 0) colours[i] = new Color(0, 1, species, 1);
                    if (species == 2) colours[i].a = stage;
                }
                mesh.colors = colours;
            }
            return mesh;
        }
        internal void Release()
        {
            if (--references != 0) return;
            Catalogs.Remove(key);
            PlanetForestWorld.Forget(this);
            Dispose();
        }
        private void Dispose()
        {
            if (meshes != null) foreach (var prototype in meshes)
                if (prototype != null) foreach (var mesh in prototype) DestroyUnityObject(mesh);
            if (--palette.references == 0)
            {
                foreach (var material in palette.materials) DestroyUnityObject(material);
                palette.bark.Dispose();
                Palettes.Remove(ValueTuple.Create(key.Item2));
            }
        }
    }
}
