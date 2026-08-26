using System;
using System.Collections.Generic;
using System.Globalization;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

/// <summary>
/// Small adapter around Rust-owned JSON Schema/editor metadata. It intentionally
/// keeps unknown schema fields in <see cref="Raw"/> so a newer baker can add
/// metadata without requiring a Unity package update.
/// </summary>
public sealed class ProceduralMaterialSchema
{
    private readonly Dictionary<string, PropertyMetadata> properties =
        new Dictionary<string, PropertyMetadata>(StringComparer.Ordinal);

    public JObject Raw { get; }
    public IReadOnlyDictionary<string, PropertyMetadata> Properties => properties;

    private ProceduralMaterialSchema(JObject raw)
    {
        Raw = raw ?? new JObject();
        var schema = (Raw["schema"] as JObject) ?? (Raw["json_schema"] as JObject) ?? Raw;
        Flatten(schema, string.Empty);
        ReadMetadataTable(Raw["metadata"] as JArray ?? Raw["fields"] as JArray);
    }

    public static ProceduralMaterialSchema FromJson(string json)
    {
        if (string.IsNullOrWhiteSpace(json)) return CreateFallback();
        try
        {
            var token = JToken.Parse(json);
            if (!(token is JObject objectToken)) return CreateFallback();
            return new ProceduralMaterialSchema(objectToken);
        }
        catch (JsonException)
        {
            return CreateFallback();
        }
    }

    public static ProceduralMaterialSchema CreateFallback()
    {
        var schema = new ProceduralMaterialSchema(new JObject { ["type"] = "object" });
        schema.AddFallback("/name", "Name", "Recipe display name", "string");
        schema.AddFallback("/seed", "Seed", "Deterministic recipe seed", "integer");
        schema.AddFallback("/width", "Width", "Final output width", "integer", 1, 8192, "px");
        schema.AddFallback("/height", "Height", "Final output height", "integer", 1, 8192, "px");
        schema.AddFallback("/physical_tile_width_m", "Tile width", "Physical width of one repeating tile", "number", 0.001, 1000, "m");
        schema.AddFallback("/physical_tile_height_m", "Tile height", "Physical height of one repeating tile", "number", 0.001, 1000, "m");
        schema.AddFallback("/material/kind", "Base material", "Rust-owned base height generator", "string");
        schema.AddFallback("/normal_scale", "Normal scale", "Normal strength", "number", 0, 8);
        schema.AddFallback("/displacement/minimum_m", "Minimum displacement", "Minimum displacement", "number", -100, 100, "m");
        schema.AddFallback("/displacement/maximum_m", "Maximum displacement", "Maximum displacement", "number", -100, 100, "m");
        schema.AddFallback("/displacement/base_m", "Base displacement", "Base displacement", "number", -100, 100, "m");
        schema.AddFallback("/displacement/displacement_map", "Displacement map", "Emit displacement map", "boolean");
        schema.AddFallback("/occlusion/directions", "AO directions", "Ambient occlusion ray directions", "integer", 1, 64);
        schema.AddFallback("/occlusion/samples", "AO samples", "Ambient occlusion samples per direction", "integer", 1, 64);
        schema.AddFallback("/occlusion/radius", "AO radius", "Occlusion sampling radius", "number", 0, 100, "px");
        schema.AddFallback("/occlusion/power", "AO power", "Occlusion response", "number", 0, 8);
        schema.AddFallback("/albedo/variation", "Albedo variation", "Base albedo variation", "number", 0, 1);
        schema.AddFallback("/albedo/crack_darkening", "Crack darkening", "Darkening applied to cracks", "number", 0, 1);
        schema.AddFallback("/albedo/occlusion_influence", "AO influence", "Optional AO influence on albedo", "number", 0, 1);

        AddLayerFallbacks(schema);
        return schema;
    }

    public PropertyMetadata Get(string pointer)
    {
        if (pointer == null) return null;
        if (properties.TryGetValue(pointer, out var exact)) return exact;
        var wildcard = ReplaceArrayIndices(pointer);
        if (properties.TryGetValue(wildcard, out var value)) return value;
        return properties.TryGetValue(wildcard.Replace("[]", "*"), out value) ? value : null;
    }

    public string Label(string pointer, string fallback)
    {
        var metadata = Get(pointer);
        return metadata == null || string.IsNullOrWhiteSpace(metadata.Label)
            ? fallback
            : metadata.Label;
    }

    private void Flatten(JObject schema, string pointer)
    {
        var propertiesToken = schema["properties"] as JObject;
        if (propertiesToken == null) return;
        foreach (var property in propertiesToken.Properties())
        {
            var propertyPointer = pointer + "/" + Escape(property.Name);
            var propertyObject = property.Value as JObject;
            if (propertyObject == null) continue;
            var metadata = new PropertyMetadata
            {
                Pointer = propertyPointer,
                Label = propertyObject["label"]?.Value<string>()
                    ?? propertyObject["title"]?.Value<string>()
                    ?? Humanize(property.Name),
                Tooltip = propertyObject["tooltip"]?.Value<string>()
                    ?? propertyObject["description"]?.Value<string>()
                    ?? string.Empty,
                Type = propertyObject["type"]?.Value<string>() ?? "string",
                Unit = propertyObject["unit"]?.Value<string>()
                    ?? propertyObject["units"]?.Value<string>()
                    ?? string.Empty,
                Minimum = Number(propertyObject["minimum"])
                    ?? Number((propertyObject["range"] as JObject)?["minimum"])
                    ?? Number((propertyObject["range"] as JObject)?["min"]),
                Maximum = Number(propertyObject["maximum"])
                    ?? Number((propertyObject["range"] as JObject)?["maximum"])
                    ?? Number((propertyObject["range"] as JObject)?["max"]),
                Default = propertyObject["default"]?.DeepClone(),
            };
            var enumToken = propertyObject["enum"] as JArray;
            if (enumToken != null)
            {
                foreach (var option in enumToken)
                {
                    metadata.EnumValues.Add(option.Value<string>() ?? option.ToString(Formatting.None));
                }
            }
            properties[propertyPointer] = metadata;
            Flatten(propertyObject, propertyPointer);
        }
    }

    private void ReadMetadataTable(JArray table)
    {
        if (table == null) return;
        foreach (var token in table)
        {
            if (!(token is JObject objectToken)) continue;
            var pointer = objectToken["pointer"]?.Value<string>() ?? objectToken["path"]?.Value<string>();
            if (string.IsNullOrWhiteSpace(pointer)) continue;
            var metadata = properties.TryGetValue(pointer, out var existing)
                ? existing
                : new PropertyMetadata { Pointer = pointer };
            metadata.Label = objectToken["label"]?.Value<string>() ?? metadata.Label ?? Humanize(pointer);
            metadata.Tooltip = objectToken["tooltip"]?.Value<string>() ?? metadata.Tooltip ?? string.Empty;
            metadata.Unit = objectToken["unit"]?.Value<string>()
                ?? objectToken["units"]?.Value<string>()
                ?? metadata.Unit
                ?? string.Empty;
            metadata.Type = objectToken["type"]?.Value<string>() ?? metadata.Type ?? "string";
            var range = objectToken["range"] as JObject;
            metadata.Minimum = Number(objectToken["minimum"] ?? objectToken["min"])
                ?? Number(range?["minimum"] ?? range?["min"])
                ?? metadata.Minimum;
            metadata.Maximum = Number(objectToken["maximum"] ?? objectToken["max"])
                ?? Number(range?["maximum"] ?? range?["max"])
                ?? metadata.Maximum;
            metadata.Default = objectToken["default"]?.DeepClone() ?? metadata.Default;
            if (objectToken["enum"] is JArray enumToken)
            {
                metadata.EnumValues.Clear();
                foreach (var value in enumToken) metadata.EnumValues.Add(value.Value<string>() ?? value.ToString(Formatting.None));
            }
            properties[pointer] = metadata;
        }
    }

    private void AddFallback(string pointer, string label, string tooltip, string type, double? minimum = null, double? maximum = null, string unit = "")
    {
        properties[pointer] = new PropertyMetadata
        {
            Pointer = pointer,
            Label = label,
            Tooltip = tooltip,
            Type = type,
            Minimum = minimum,
            Maximum = maximum,
            Unit = unit,
        };
    }

    private static void AddLayerFallbacks(ProceduralMaterialSchema schema)
    {
        schema.AddFallback("/layers/[]/name", "Layer name", "Stable artist-facing layer name", "string");
        schema.AddFallback("/layers/[]/enabled", "Enabled", "Include this layer in Rust evaluation", "boolean");
        schema.AddFallback("/layers/[]/source/kind", "Source", "Scalar source kind", "string");
        schema.AddFallback("/layers/[]/source/frequency", "Frequency", "Cells per tile", "integer", 1, 4096, "cells/tile");
        schema.AddFallback("/layers/[]/source/octaves", "Octaves", "Fractal octave count", "integer", 1, 16);
        schema.AddFallback("/layers/[]/source/lacunarity", "Lacunarity", "Frequency multiplier per octave", "number", 0.01, 16);
        schema.AddFallback("/layers/[]/source/gain", "Gain", "Amplitude multiplier per octave", "number", -4, 4);
        schema.AddFallback("/layers/[]/source/offset/0", "X offset", "Periodic source offset", "number", -10000, 10000);
        schema.AddFallback("/layers/[]/source/offset/1", "Y offset", "Periodic source offset", "number", -10000, 10000);
        schema.AddFallback("/layers/[]/source/seed_domain", "Seed domain", "Independent deterministic seed domain", "integer");
        schema.AddFallback("/layers/[]/source/cellular_jitter", "Cellular jitter", "Cellular feature jitter", "number", 0, 1);
        schema.AddFallback("/layers/[]/source/domain_warp", "Domain warp", "Optional explicit source domain warp", "object");
        schema.AddFallback("/layers/[]/remap/input_min", "Input minimum", "Raw source lower bound", "number", -100, 100);
        schema.AddFallback("/layers/[]/remap/input_max", "Input maximum", "Raw source upper bound", "number", -100, 100);
        schema.AddFallback("/layers/[]/remap/invert", "Invert", "Invert remapped scalar", "boolean");
        schema.AddFallback("/layers/[]/remap/contrast", "Contrast", "Monotonic remap contrast", "number", -8, 8);
        schema.AddFallback("/layers/[]/remap/bias", "Bias", "Monotonic remap bias", "number", -1, 1);
        schema.AddFallback("/layers/[]/remap/clamp", "Clamp", "Clamp the remapped value", "boolean");
        schema.AddFallback("/layers/[]/outputs/height/enabled", "Height", "Route this layer to height", "boolean");
        schema.AddFallback("/layers/[]/outputs/height/blend/kind", "Height blend", "Height blend operation", "string");
        schema.AddFallback("/layers/[]/outputs/height/strength_m", "Height strength", "Physical height contribution", "number", -100, 100, "m");
        schema.AddFallback("/layers/[]/outputs/albedo/enabled", "Albedo", "Route this layer to albedo", "boolean");
        schema.AddFallback("/layers/[]/outputs/albedo/blend/kind", "Albedo blend", "Albedo blend operation", "string");
        schema.AddFallback("/layers/[]/outputs/albedo/strength", "Albedo strength", "Albedo contribution strength", "number", 0, 1);
        schema.AddFallback("/layers/[]/mask/kind", "Mask", "Layer mask source", "string");
        schema.AddFallback("/layers/[]/mask/layer_id", "Mask layer", "Earlier layer stable ID", "string");
    }

    private static string ReplaceArrayIndices(string pointer)
    {
        var segments = pointer.Split('/');
        for (var index = 0; index < segments.Length; index++)
        {
            if (int.TryParse(segments[index], NumberStyles.Integer, CultureInfo.InvariantCulture, out _)) segments[index] = "[]";
        }
        return string.Join("/", segments);
    }

    private static double? Number(JToken token)
    {
        if (token == null) return null;
        if (double.TryParse(token.ToString(), NumberStyles.Float, CultureInfo.InvariantCulture, out var value)) return value;
        return null;
    }

    private static string Escape(string value)
    {
        return (value ?? string.Empty).Replace("~", "~0").Replace("/", "~1");
    }

    private static string Humanize(string value)
    {
        if (string.IsNullOrWhiteSpace(value)) return "Property";
        var result = value.Replace('_', ' ');
        return char.ToUpperInvariant(result[0]) + result.Substring(1);
    }

    public sealed class PropertyMetadata
    {
        public string Pointer;
        public string Label;
        public string Tooltip;
        public string Type;
        public string Unit;
        public double? Minimum;
        public double? Maximum;
        public JToken Default;
        public List<string> EnumValues { get; } = new List<string>();
    }
}
