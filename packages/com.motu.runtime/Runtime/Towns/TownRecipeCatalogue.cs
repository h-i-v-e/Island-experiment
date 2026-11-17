using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using UnityEngine;

namespace Motu.Towns
{
    public enum TownPlacement { Centre, Other }

    [Serializable]
    public sealed class TownRecipeEntry
    {
        public string id;
        [Tooltip("Building Studio recipe JSON. Geometry, materials and doors remain native-owned.")]
        public TextAsset buildingRecipe;
        [Min(0)] public int minimumCount;
        [Min(0)] public int maximumCount = 1;
        public TownPlacement placement;
    }

    [Serializable]
    public sealed class TownMaterialDependency
    {
        [Tooltip("Exact material recipe path used by a Building Studio JSON *_recipe field.")]
        public string referencePath;
        public TextAsset recipe;
    }

    internal sealed class TownDependencyData
    {
        internal readonly string fileName, json;
        internal TownDependencyData(string fileName, string json) { this.fileName = fileName; this.json = json; }
    }

    [CreateAssetMenu(menuName = "Motu/Town Recipe Catalogue", fileName = "TownRecipeCatalogue")]
    public sealed class TownRecipeCatalogue : ScriptableObject
    {
        [SerializeField] private TownRecipeEntry[] entries = Array.Empty<TownRecipeEntry>();
        [SerializeField] private TownPlanningSettings settings = new TownPlanningSettings();
        [Tooltip("Portable TextAsset bindings for material recipe paths referenced by buildings.")]
        [SerializeField] private TownMaterialDependency[] materialDependencies = Array.Empty<TownMaterialDependency>();
        public TownRecipeEntry[] Entries => entries;
        public TownPlanningSettings Settings => settings;

        internal string CaptureJson() => CaptureJson(out _);

        internal string CaptureJson(out TownDependencyData[] dependencies, bool preview = false)
        {
            if (entries == null || entries.Length > 64) throw new ArgumentException("Town catalogues support up to 64 entries.");
            settings.Validate();
            var captured = new Dictionary<string, TownDependencyData>(StringComparer.Ordinal);
            var catalogue = new List<Entry>();
            var ids = new HashSet<string>(StringComparer.Ordinal);
            long maximumCopies = 0;
            foreach (var entry in entries)
            {
                if (entry == null || entry.buildingRecipe == null || string.IsNullOrWhiteSpace(entry.id))
                    throw new ArgumentException("Every town entry needs an ID and Building Studio JSON recipe.");
                if (!ids.Add(entry.id)) throw new ArgumentException("Duplicate town recipe ID: " + entry.id);
                if (entry.minimumCount < 0 || entry.maximumCount < entry.minimumCount)
                    throw new ArgumentException("Town recipe count range is invalid: " + entry.id);
                maximumCopies += entry.maximumCount;
                if (maximumCopies > 256) throw new ArgumentException("Town catalogues support up to 256 total maximum copies.");
                catalogue.Add(new Entry {
                    id = entry.id, building_json = BindMaterials(entry.buildingRecipe.text, captured),
                    min_count = preview ? 1 : entry.minimumCount, max_count = preview ? 1 : entry.maximumCount,
                    placement = entry.placement == TownPlacement.Centre ? "centre" : "other"
                });
            }
            dependencies = new List<TownDependencyData>(captured.Values).ToArray();
            return JsonUtility.ToJson(new Recipe { catalogue = catalogue.ToArray(), settings = settings });
        }

        private string BindMaterials(string buildingJson, Dictionary<string, TownDependencyData> captured)
        {
            return Regex.Replace(buildingJson,
                "(\\\"(?:wall|roof|door|chimney|timber|floor|foundation|stone_slab|ceiling)_recipe\\\"\\s*:\\s*)(\\\"(?:\\\\.|[^\\\"\\\\])*\\\")",
                match => {
                    var path = JsonUtility.FromJson<JsonString>("{\"value\":" + match.Groups[2].Value + "}").value;
                    if (string.IsNullOrEmpty(path)) return match.Value;
                    if (!captured.TryGetValue(path, out var dependency))
                    {
                        TextAsset source = null;
                        foreach (var binding in materialDependencies ?? Array.Empty<TownMaterialDependency>())
                            if (binding != null && binding.referencePath == path) { source = binding.recipe; break; }
                        if (source == null) throw new ArgumentException("Bind the material recipe TextAsset for: " + path);
                        using var hash = SHA256.Create();
                        var digest = BitConverter.ToString(hash.ComputeHash(Encoding.UTF8.GetBytes(source.text))).Replace("-", "").ToLowerInvariant();
                        dependency = new TownDependencyData(digest + ".json", source.text);
                        captured.Add(path, dependency);
                    }
                    return match.Groups[1].Value + "\"" + dependency.fileName + "\"";
                });
        }

        [Serializable] private sealed class JsonString { public string value; }

        [Serializable] private sealed class Recipe
        {
            public int schema_version = 1;
            public Entry[] catalogue;
            public TownPlanningSettings settings;
        }

        [Serializable] private sealed class Entry
        {
            public string id, building_json, placement;
            public int min_count, max_count;
        }
    }

    [Serializable]
    public sealed class TownPlanningSettings
    {
        [Range(1f, 1000f), InspectorName("Site Search Radius (m)")] public float site_search_radius = 100f;
        [Range(1f, 1000f), InspectorName("Town Radius (m)")] public float town_radius = 85f;
        [Range(0f, 100f), InspectorName("Building Gap (m)")] public float building_gap = 3f;
        [Range(0f, 100f), InspectorName("River Margin (m)")] public float river_margin = 3f;
        [Range(0f, 100f), InspectorName("Decoration Clearance Margin (m)")] public float clearance_margin = 2f;
        [Range(0f, 30f), InspectorName("Entrance Approach Length (m)")] public float approach_length = 3f;
        [Range(0.1f, 40f), InspectorName("Maximum Slope (degrees)")] public float max_slope_degrees = 14f;
        [Tooltip("Maximum actual foundation masonry depth including burial. Stairs use their native terrain-fitted geometry.")]
        [Range(0.05f, 10f), InspectorName("Maximum Foundation Depth (m)")] public float max_support_height = 4f;
        [Range(0.01f, 10f), InspectorName("Minimum Dry Height (m)")] public float min_dry_height = 0.35f;
        [Range(0.25f, 4f), InspectorName("Terrain Sample Spacing (m)")] public float sample_spacing = 0.75f;
        [Range(1, 512), InspectorName("Placement Attempts Per Copy")] public int attempts_per_copy = 96;
        [Range(1, 16384), InspectorName("Total Placement Attempt Limit")] public int max_total_attempts = 4096;

        internal void Validate()
        {
            Require("Site search radius", site_search_radius, 1, 1000);
            Require("Town radius", town_radius, 1, 1000);
            Require("Building gap", building_gap, 0, 100);
            Require("River margin", river_margin, 0, 100);
            Require("Clearance margin", clearance_margin, 0, 100);
            Require("Approach length", approach_length, 0, 30);
            Require("Maximum slope", max_slope_degrees, 0.1f, 40);
            Require("Maximum support height", max_support_height, 0.05f, 10);
            Require("Minimum dry height", min_dry_height, 0.01f, 10);
            Require("Sample spacing", sample_spacing, 0.25f, 4);
            Require("Attempts per copy", attempts_per_copy, 1, 512);
            Require("Total attempts", max_total_attempts, 1, 16384);
        }

        private static void Require(string name, float value, float min, float max)
        {
            if (!float.IsFinite(value) || value < min || value > max)
                throw new ArgumentException($"{name} must be finite and between {min} and {max}.");
        }
    }

    [Serializable]
    public sealed class CoastalTownSettings
    {
        [Tooltip("Place one native-planned town beside the longest river mouth. Riverless strips remain empty.")]
        public bool enabled = true;
        [Tooltip("Empty uses the packaged cathedral, cottage and workshop catalogue.")]
        public TownRecipeCatalogue catalogue;

        internal string CaptureJson() => CaptureJson(out _);

        internal string CaptureJson(out TownDependencyData[] dependencies)
        {
            dependencies = Array.Empty<TownDependencyData>();
            if (!enabled) return null;
            var source = catalogue != null ? catalogue : Resources.Load<TownRecipeCatalogue>("Towns/DefaultTownCatalogue");
            if (source == null) throw new InvalidOperationException("The default Motu town catalogue is missing.");
            return source.CaptureJson(out dependencies);
        }

        internal CoastalTownSettings Copy() => (CoastalTownSettings)MemberwiseClone();
    }
}
