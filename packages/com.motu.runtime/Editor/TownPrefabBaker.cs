using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Motu.Towns;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public static class TownPrefabBaker
    {
        [MenuItem("Motu/Towns/Bake Default Preview Prefabs")]
        public static void BakeDefaultPreview()
        {
            var paths = Bake(Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue"), 42);
            Debug.Log("MOTU TOWN PREFABS: " + string.Join(", ", paths));
        }

        /// <summary>Bakes reusable shells; terrain-fitted supports remain per-town runtime geometry.</summary>
        public static string[] Bake(TownRecipeCatalogue catalogue, int previewSeed,
            string destinationFolder = "Assets/Generated/Towns", TownDecorationCatalogue decorationCatalogue = null)
        {
            if (catalogue == null) throw new ArgumentNullException(nameof(catalogue));
            if (!destinationFolder.StartsWith("Assets/", StringComparison.Ordinal) || destinationFolder.Contains(".."))
                throw new ArgumentException("Town prefabs must be baked into an Assets subfolder.", nameof(destinationFolder));
            var recipe = catalogue.CaptureJson(out var dependencies, preview: true);
            var data = TownNative.Generate(recipe, PreviewInput(previewSeed), dependencies);
            if (data.prototypes.Length == 0) throw new ArgumentException("The catalogue has no building recipes to bake.");
            EnsureFolder(destinationFolder);
            var folder = AssetDatabase.GenerateUniqueAssetPath(destinationFolder + "/Preview-" + previewSeed);
            AssetDatabase.CreateFolder(destinationFolder, Path.GetFileName(folder));
            var assets = Object.Instantiate(catalogue);
            assets.name = catalogue.name + " preview resources";
            AssetDatabase.CreateAsset(assets, folder + "/BuildingResources.asset");
            var copied = new Dictionary<Object, Object>();
            var owner = new GameObject("Town prefab bake source");
            var paths = new List<string>();
            try
            {
                var library = TownRenderer.Install(owner.transform, data, 2000f, includeUnplacedPrototypes: true);
                for (var index = 0; index < library.SharedPrototypes.Length; index++)
                {
                    var instance = Object.Instantiate(library.SharedPrototypes[index]);
                    try
                    {
                        instance.name = data.prototypes[index].entry_id;
                        instance.GetComponent<TownDecorationAnchors>()?.ConfigureBaked(decorationCatalogue, data.seed, instance.name);
                        PersistResources(instance, assets, copied);
                        var safeName = string.Join("_", instance.name.Split(Path.GetInvalidFileNameChars()));
                        var path = AssetDatabase.GenerateUniqueAssetPath(folder + "/" + safeName + ".prefab");
                        PrefabUtility.SaveAsPrefabAsset(instance, path);
                        paths.Add(path);
                    }
                    finally { Object.DestroyImmediate(instance); }
                }
                AssetDatabase.SaveAssets();
                return paths.ToArray();
            }
            catch { AssetDatabase.DeleteAsset(folder); throw; }
            finally { Object.DestroyImmediate(owner); }
        }

        private static void PersistResources(GameObject root, Object container, Dictionary<Object, Object> copied)
        {
            foreach (var filter in root.GetComponentsInChildren<MeshFilter>(true))
                filter.sharedMesh = Copy(filter.sharedMesh, container, copied);
            foreach (var collider in root.GetComponentsInChildren<MeshCollider>(true))
                collider.sharedMesh = Copy(collider.sharedMesh, container, copied);
            foreach (var renderer in root.GetComponentsInChildren<MeshRenderer>(true))
                renderer.sharedMaterials = renderer.sharedMaterials.Select(source => {
                    var material = Copy(source, container, copied);
                    foreach (var name in material.GetTexturePropertyNames())
                    {
                        var texture = material.GetTexture(name);
                        if (texture != null && !EditorUtility.IsPersistent(texture))
                        {
                            var persistentTexture = Copy(texture, container, copied, material.name + " " + name);
                            material.SetTexture(name, persistentTexture);
                        }
                    }
                    EditorUtility.SetDirty(material);
                    return material;
                }).ToArray();
        }

        private static T Copy<T>(T source, Object container, Dictionary<Object, Object> copied, string assetName = null) where T : Object
        {
            if (source == null || EditorUtility.IsPersistent(source)) return source;
            if (copied.TryGetValue(source, out var existing)) return (T)existing;
            if (source is Texture2D texture)
            {
                var persistent = PersistTexture(texture, container, assetName ?? source.name);
                copied.Add(source, persistent);
                return (T)(Object)persistent;
            }
            var clone = Object.Instantiate(source);
            clone.name = source.name;
            clone.hideFlags = HideFlags.None;
            AssetDatabase.AddObjectToAsset(clone, container);
            copied.Add(source, clone);
            return clone;
        }

        private static Texture2D PersistTexture(Texture2D source, Object container, string name)
        {
            var folder = Path.GetDirectoryName(AssetDatabase.GetAssetPath(container)).Replace('\\', '/') + "/Textures";
            EnsureFolder(folder);
            var safeName = string.Join("_", name.Split(Path.GetInvalidFileNameChars()));
            var path = AssetDatabase.GenerateUniqueAssetPath(folder + "/" + safeName + ".png");
            File.WriteAllBytes(path, source.EncodeToPNG());
            AssetDatabase.ImportAsset(path, ImportAssetOptions.ForceSynchronousImport);
            var importer = (TextureImporter)AssetImporter.GetAtPath(path);
            // Gamma projects can expose a linear graphics format for sRGB data.
            // The native material contract explicitly assigns colour to BaseMap.
            importer.sRGBTexture = name.EndsWith("_BaseMap", StringComparison.Ordinal);
            importer.textureType = TextureImporterType.Default;
            importer.textureCompression = TextureImporterCompression.Uncompressed;
            importer.mipmapEnabled = source.mipmapCount > 1;
            importer.wrapMode = source.wrapMode;
            importer.filterMode = source.filterMode;
            importer.anisoLevel = source.anisoLevel;
            importer.maxTextureSize = Mathf.Max(source.width, source.height);
            importer.SaveAndReimport();
            return AssetDatabase.LoadAssetAtPath<Texture2D>(path);
        }

        private static void EnsureFolder(string path)
        {
            if (AssetDatabase.IsValidFolder(path)) return;
            var parent = Path.GetDirectoryName(path).Replace('\\', '/');
            EnsureFolder(parent);
            AssetDatabase.CreateFolder(parent, Path.GetFileName(path));
        }

        internal static string PreviewInput(int seed)
        {
            var heights = string.Join(",", Enumerable.Range(0, 441).Select(i => i < 21 ? "-2" : "2"));
            return "{\"tile_seed\":" + unchecked((uint)seed) + ",\"sea_level\":0,\"terrain\":{\"origin\":[0,0],\"spacing\":100,\"width\":21,\"height\":21,\"heights\":[" + heights
                + "]},\"rivers\":[{\"points\":[[1000,1600],[1000,0]],\"widths\":[3,8]}]}";
        }
    }
}
